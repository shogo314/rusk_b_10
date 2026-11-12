//! Walk every syntactic scope without type checking or specializing bodies.
use super::*;
use crate::frontend::ast::{Condition, Expr, ExprKind, Initializer};

pub(super) struct Declarations<'a, F> {
    origin: Origin,
    add: &'a mut F,
    scope: Vec<String>,
    children: Vec<BTreeMap<(String, String), usize>>,
}
impl<'a, F: FnMut(Entry)> Declarations<'a, F> {
    pub(super) fn new(origin: Origin, add: &'a mut F) -> Self {
        Self {
            origin,
            add,
            scope: vec![],
            children: vec![BTreeMap::new()],
        }
    }
    pub(super) fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement, "");
        }
    }
    fn emit(&mut self, mut entry: Entry) {
        entry.scope = self.scope.clone();
        entry.internal |= self.origin == Origin::Standard && !entry.scope.is_empty();
        (self.add)(entry);
    }
    fn within(&mut self, file: &str, base: &str, numbered: bool, visit: impl FnOnce(&mut Self)) {
        let count = self
            .children
            .last_mut()
            .unwrap()
            .entry((file.into(), base.into()))
            .or_default();
        *count += 1;
        let label = if numbered {
            format!("{base} {count}")
        } else if *count > 1 {
            format!("{base} [{count}]")
        } else {
            base.into()
        };
        self.scope.push(label);
        self.children.push(BTreeMap::new());
        visit(self);
        self.children.pop();
        self.scope.pop();
    }
    fn statement(&mut self, statement: &Stmt, inherited_file: &str) {
        let file = statement_file(statement).unwrap_or(inherited_file);
        match statement {
            Stmt::Struct(name, generics, fields) => {
                self.emit(type_entry(
                    self.origin,
                    name.clone(),
                    "struct",
                    generics.clone(),
                ));
                for (field, ty) in fields {
                    self.emit(entry(
                        self.origin,
                        field.clone(),
                        "field",
                        format!("{}: {}", field.text, type_text(&ty.text)),
                        Some(name.text.clone()),
                    ));
                }
            }
            Stmt::Enum(name, generics, variants) => {
                self.emit(type_entry(
                    self.origin,
                    name.clone(),
                    "enum",
                    generics.clone(),
                ));
                for (variant, payload) in variants {
                    let signature = if payload.is_empty() {
                        variant.text.clone()
                    } else {
                        format!(
                            "{}({})",
                            variant.text,
                            payload
                                .iter()
                                .map(|ty| type_text(&ty.text))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    self.emit(entry(
                        self.origin,
                        variant.clone(),
                        "variant",
                        signature,
                        Some(name.text.clone()),
                    ));
                }
            }
            Stmt::Type(name, _) => {
                self.emit(type_entry(self.origin, name.clone(), "extern type", vec![]))
            }
            Stmt::GenericExternType(name, generics, _) => self.emit(type_entry(
                self.origin,
                name.clone(),
                "extern type",
                generics.clone(),
            )),
            Stmt::Symbol(name, _) => self.emit(entry(
                self.origin,
                name.clone(),
                "extern symbol",
                format!("extern symbol {}", name.text),
                None,
            )),
            Stmt::Function(name, params, result, _) => self.emit(function(
                self.origin,
                name.clone(),
                "extern function",
                vec![],
                params.clone(),
                result.clone(),
            )),
            Stmt::GenericExternFunction(name, generics, params, result, properties) => {
                let mut entry = function(
                    self.origin,
                    name.clone(),
                    "extern function",
                    generics.clone(),
                    params.clone(),
                    result.clone(),
                );
                entry.internal |=
                    self.origin == Origin::Standard && properties.contains_key("derive");
                self.emit(entry);
            }
            Stmt::UserFunction(name, generics, params, result, body) => {
                let entry = function(
                    self.origin,
                    name.clone(),
                    "function",
                    generics.clone(),
                    params.clone(),
                    result.clone(),
                );
                let signature = entry.signature.clone();
                self.emit(entry);
                self.within(file, &signature, false, |this| {
                    this.function_body(body, file)
                });
            }
            Stmt::Block(statements, tail) => self.within(file, "block", true, |this| {
                this.block(statements, tail.as_deref(), file)
            }),
            Stmt::Labeled(_, body) | Stmt::Loop(body) => self.statement(body, file),
            Stmt::If(condition, body, otherwise) | Stmt::While(condition, body, otherwise) => {
                self.condition(condition);
                self.statement(body, file);
                if let Some(otherwise) = otherwise {
                    self.statement(otherwise, file);
                }
            }
            Stmt::For(_, source, body, otherwise) => {
                self.expression(source);
                self.statement(body, file);
                if let Some(otherwise) = otherwise {
                    self.statement(otherwise, file);
                }
            }
            Stmt::Let(_, _, Some(Initializer::Shared(value) | Initializer::Copied(value))) => {
                self.expression(value)
            }
            Stmt::Return(_, Some(value)) | Stmt::Expr(value) | Stmt::Match(value) => {
                self.expression(value)
            }
            Stmt::Input(items) => {
                for (_, _, args) in items {
                    for arg in args {
                        self.expression(arg);
                    }
                }
            }
            Stmt::Output(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            Stmt::Let(_, _, None)
            | Stmt::Return(_, None)
            | Stmt::Include(_, _)
            | Stmt::Break(_, _)
            | Stmt::Continue(_, _) => {}
        }
    }
    fn function_body(&mut self, body: &Stmt, file: &str) {
        if let Stmt::Block(statements, tail) = body {
            self.block(statements, tail.as_deref(), file);
        } else {
            self.statement(body, file);
        }
    }
    fn block(&mut self, statements: &[Stmt], tail: Option<&Expr>, file: &str) {
        for statement in statements {
            self.statement(statement, file);
        }
        if let Some(tail) = tail {
            self.expression(tail);
        }
    }
    fn condition(&mut self, condition: &Condition) {
        let (Condition::Expr(expression) | Condition::Let(_, expression)) = condition;
        self.expression(expression);
    }
    fn expression(&mut self, expression: &Expr) {
        let file = &expression.span.file;
        match &expression.kind {
            ExprKind::Literal(_, _) | ExprKind::Var(_) => {}
            ExprKind::Block(statements, tail) => self.within(file, "block", true, |this| {
                this.block(statements, tail.as_deref(), file)
            }),
            ExprKind::Lambda(_, body) => self.within(file, "lambda", true, |this| {
                if let ExprKind::Block(statements, tail) = &body.kind {
                    this.block(statements, tail.as_deref(), file);
                } else {
                    this.expression(body);
                }
            }),
            ExprKind::Match(value, arms) => {
                self.expression(value);
                self.within(file, "match", true, |this| {
                    for (i, arm) in arms.iter().enumerate() {
                        this.within(file, &format!("arm {}", i + 1), false, |this| {
                            if let Some(guard) = &arm.guard {
                                this.expression(guard);
                            }
                            this.expression(&arm.body);
                        });
                    }
                });
            }
            ExprKind::If(condition, yes, no) => {
                self.condition(condition);
                self.expression(yes);
                self.expression(no);
            }
            ExprKind::Array(values) | ExprKind::Tuple(values) | ExprKind::Call(_, values) => {
                for value in values {
                    self.expression(value);
                }
            }
            ExprKind::Record(_, fields) => {
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            ExprKind::Invoke(callee, args) => {
                self.expression(callee);
                for arg in args {
                    self.expression(arg);
                }
            }
            ExprKind::Field(owner, _) | ExprKind::Unwrap(owner) | ExprKind::Unary(_, owner) => {
                self.expression(owner)
            }
            ExprKind::CompareChain(first, rest) => {
                self.expression(first);
                for (_, next, _) in rest {
                    self.expression(next);
                }
            }
            ExprKind::Repeat(a, b)
            | ExprKind::Range(a, b)
            | ExprKind::Coalesce(a, b)
            | ExprKind::Index(a, b)
            | ExprKind::Logical(_, a, b)
            | ExprKind::Binary(_, a, b)
            | ExprKind::Assign(_, a, b) => {
                self.expression(a);
                self.expression(b);
            }
        }
    }
}

fn statement_file(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Struct(name, _, _)
        | Stmt::Enum(name, _, _)
        | Stmt::Type(name, _)
        | Stmt::GenericExternType(name, _, _)
        | Stmt::Symbol(name, _)
        | Stmt::Function(name, _, _, _)
        | Stmt::GenericExternFunction(name, _, _, _, _)
        | Stmt::UserFunction(name, _, _, _, _)
        | Stmt::Labeled(name, _) => Some(&name.span.file),
        Stmt::Return(span, _)
        | Stmt::Include(_, span)
        | Stmt::Break(span, _)
        | Stmt::Continue(span, _) => Some(&span.file),
        Stmt::Block(statements, tail) => statements
            .iter()
            .find_map(statement_file)
            .or_else(|| tail.as_ref().map(|e| e.span.file.as_str())),
        Stmt::Let(binding, _, _) | Stmt::For(binding, _, _, _) => Some(&binding.span().file),
        Stmt::If(condition, _, _) | Stmt::While(condition, _, _) => Some(&condition.span().file),
        Stmt::Loop(body) => statement_file(body),
        Stmt::Expr(value) | Stmt::Match(value) => Some(&value.span.file),
        Stmt::Input(items) => items.first().map(|(name, _, _)| name.span.file.as_str()),
        Stmt::Output(values) => values.first().map(|value| value.span.file.as_str()),
    }
}
