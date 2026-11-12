//! Give block-local nominal types stable identities without changing source ASTs.
use super::*;

#[derive(Default)]
pub(super) struct LocalTypes {
    bindings: BTreeMap<(scopes::ScopeId, String), String>,
    pub names: BTreeMap<String, Name>,
}

impl Compiler {
    pub(super) fn diagnostic_type(&self, text: &str) -> String {
        let (base, args) =
            tuples::type_parts(text).map_or((text, None), |(base, args)| (base, Some(args)));
        let name = self.local_types.names.get(base);
        let mut result = name.map_or(base, |name| name.text.as_str()).to_string();
        if let Some(args) = args {
            result.push('<');
            result.push_str(
                &args
                    .iter()
                    .map(|arg| self.diagnostic_type(arg))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            result.push('>');
        }
        if let Some(name) = name {
            result.push_str(&format!(
                " (declared at {}:{}:{})",
                name.span.file, name.span.line, name.span.column
            ));
        }
        result
    }
    pub(super) fn declare_local_type_names(&mut self, statements: &[Stmt]) -> Result<(), String> {
        for statement in statements {
            let (Stmt::Struct(name, parameters, _) | Stmt::Enum(name, parameters, _)) = statement
            else {
                continue;
            };
            generics::parameters_valid(parameters)?;
            if name.text.starts_with("__Rusk") {
                return Err(name.span.error("reserved internal type name"));
            }
            if name.text == "Tuple" {
                return Err(name.span.error("reserved tuple type constructor"));
            }
            let key = (self.context.declaration_scope, name.text.clone());
            if self.local_types.bindings.contains_key(&key) {
                return Err(name.span.error(format!("duplicate type '{}'", name.text)));
            }
            let identity = format!("__RuskLocalType{}", self.local_types.names.len());
            self.local_types.bindings.insert(key, identity.clone());
            self.local_types
                .names
                .insert(identity.clone(), name.clone());
            self.type_declaration_scopes
                .insert(identity, self.context.declaration_scope);
        }
        Ok(())
    }

    fn scoped_type_name(&self, name: &str) -> String {
        let mut scope = Some(self.context.declaration_scope);
        while let Some(current) = scope {
            if let Some(identity) = self.local_types.bindings.get(&(current, name.into())) {
                return identity.clone();
            }
            scope = self.declaration_scopes.parent(current);
        }
        name.into()
    }

    fn resolve_type_text(&self, text: &str, bound: &BTreeSet<String>) -> String {
        if let Some((base, args)) = tuples::type_parts(text) {
            let base = if bound.contains(base) {
                base.into()
            } else {
                self.scoped_type_name(base)
            };
            format!(
                "{base}<{}>",
                args.iter()
                    .map(|arg| self.resolve_type_text(arg, bound))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        } else if bound.contains(text) {
            text.into()
        } else {
            self.scoped_type_name(text)
        }
    }

    fn resolve_type_reference(&self, name: &mut Name, bound: &BTreeSet<String>) {
        name.text = self.resolve_type_text(&name.text, bound);
    }

    fn resolve_value_reference(&self, name: &mut Name, bound: &BTreeSet<String>) {
        if let Some((owner, variant)) = name.text.rsplit_once("::") {
            name.text = format!("{}::{variant}", self.resolve_type_text(owner, bound));
        } else if let Some((base, args)) = tuples::type_parts(&name.text) {
            name.text = format!(
                "{base}<{}>",
                args.iter()
                    .map(|arg| self.resolve_type_text(arg, bound))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }

    fn resolve_pattern_types(&self, pattern: &mut Pattern, bound: &BTreeSet<String>) {
        match &mut pattern.kind {
            PatternKind::Named(name, children) => {
                self.resolve_value_reference(name, bound);
                if let Some(children) = children {
                    for child in children {
                        self.resolve_pattern_types(child, bound);
                    }
                }
            }
            PatternKind::Record(name, fields, _) => {
                self.resolve_type_reference(name, bound);
                for (_, pattern) in fields {
                    self.resolve_pattern_types(pattern, bound);
                }
            }
            PatternKind::Tuple(items) | PatternKind::Or(items) => {
                for item in items {
                    self.resolve_pattern_types(item, bound);
                }
            }
            PatternKind::Literal(..) => {}
        }
    }

    fn resolve_condition_types(&self, condition: &mut Condition, bound: &BTreeSet<String>) {
        match condition {
            Condition::Expr(value) => self.resolve_expression_types(value, bound),
            Condition::Let(pattern, value) => {
                self.resolve_expression_types(value, bound);
                self.resolve_pattern_types(pattern, bound);
            }
        }
    }

    pub(super) fn scoped_expression(&self, expression: &Expr) -> Expr {
        let mut expression = expression.clone();
        self.resolve_expression_types(&mut expression, &BTreeSet::new());
        expression
    }

    fn resolve_expression_types(&self, expression: &mut Expr, bound: &BTreeSet<String>) {
        match &mut expression.kind {
            // Each block registers its own type names before resolving its contents.
            ExprKind::Block(..) | ExprKind::Literal(..) => {}
            ExprKind::Var(name) => self.resolve_value_reference(name, bound),
            ExprKind::Call(name, args) => {
                self.resolve_value_reference(name, bound);
                for arg in args {
                    self.resolve_expression_types(arg, bound);
                }
            }
            ExprKind::Array(items) | ExprKind::Tuple(items) => {
                for item in items {
                    self.resolve_expression_types(item, bound);
                }
            }
            ExprKind::Record(name, fields) => {
                self.resolve_type_reference(name, bound);
                for (_, value) in fields {
                    self.resolve_expression_types(value, bound);
                }
            }
            ExprKind::Lambda(parameters, body) => {
                for (_, ty) in parameters {
                    self.resolve_type_reference(ty, bound);
                }
                self.resolve_expression_types(body, bound);
            }
            ExprKind::Field(value, name) => {
                self.resolve_expression_types(value, bound);
                self.resolve_value_reference(name, bound);
            }
            ExprKind::Unary(_, value) | ExprKind::Unwrap(value) => {
                self.resolve_expression_types(value, bound)
            }
            ExprKind::Repeat(a, b)
            | ExprKind::Range(a, b)
            | ExprKind::Logical(_, a, b)
            | ExprKind::Coalesce(a, b)
            | ExprKind::Index(a, b)
            | ExprKind::Binary(_, a, b)
            | ExprKind::Assign(_, a, b) => {
                self.resolve_expression_types(a, bound);
                self.resolve_expression_types(b, bound);
            }
            ExprKind::Invoke(value, args) => {
                self.resolve_expression_types(value, bound);
                for arg in args {
                    self.resolve_expression_types(arg, bound);
                }
            }
            ExprKind::CompareChain(value, rest) => {
                self.resolve_expression_types(value, bound);
                for (_, value, _) in rest {
                    self.resolve_expression_types(value, bound);
                }
            }
            ExprKind::If(condition, yes, no) => {
                self.resolve_condition_types(condition, bound);
                self.resolve_expression_types(yes, bound);
                self.resolve_expression_types(no, bound);
            }
            ExprKind::Match(value, arms) => {
                self.resolve_expression_types(value, bound);
                for arm in arms {
                    self.resolve_pattern_types(&mut arm.pattern, bound);
                    if let Some(guard) = &mut arm.guard {
                        self.resolve_expression_types(guard, bound);
                    }
                    self.resolve_expression_types(&mut arm.body, bound);
                }
            }
        }
    }

    pub(super) fn scoped_statements(&self, statements: &[Stmt]) -> Vec<Stmt> {
        let mut statements = statements.to_vec();
        for statement in &mut statements {
            self.resolve_statement_types(statement, &BTreeSet::new());
        }
        statements
    }

    fn resolve_statement_types(&self, statement: &mut Stmt, bound: &BTreeSet<String>) {
        match statement {
            Stmt::Struct(name, parameters, fields) => {
                name.text = self.scoped_type_name(&name.text);
                let bound = bound
                    .iter()
                    .cloned()
                    .chain(parameters.iter().map(|p| p.text.clone()))
                    .collect();
                for (_, ty) in fields {
                    self.resolve_type_reference(ty, &bound);
                }
            }
            Stmt::Enum(name, parameters, variants) => {
                name.text = self.scoped_type_name(&name.text);
                let bound = bound
                    .iter()
                    .cloned()
                    .chain(parameters.iter().map(|p| p.text.clone()))
                    .collect();
                for (_, fields) in variants {
                    for ty in fields {
                        self.resolve_type_reference(ty, &bound);
                    }
                }
            }
            Stmt::UserFunction(_, parameters, arguments, result, _) => {
                let bound = bound
                    .iter()
                    .cloned()
                    .chain(parameters.iter().map(|p| p.text.clone()))
                    .collect();
                for (_, ty) in arguments {
                    self.resolve_type_reference(ty, &bound);
                }
                if let Some(result) = result {
                    self.resolve_type_reference(result, &bound);
                }
            }
            Stmt::Let(_, ty, initializer) => {
                if let Some(ty) = ty {
                    self.resolve_type_reference(ty, bound);
                }
                if let Some(Initializer::Shared(value) | Initializer::Copied(value)) = initializer {
                    self.resolve_expression_types(value, bound);
                }
            }
            Stmt::Input(items) => {
                for (_, ty, args) in items {
                    self.resolve_type_reference(ty, bound);
                    for arg in args {
                        self.resolve_expression_types(arg, bound);
                    }
                }
            }
            Stmt::Output(values) => {
                for value in values {
                    self.resolve_expression_types(value, bound);
                }
            }
            Stmt::Expr(value) | Stmt::Match(value) | Stmt::Return(_, Some(value)) => {
                self.resolve_expression_types(value, bound)
            }
            Stmt::If(condition, yes, no) | Stmt::While(condition, yes, no) => {
                self.resolve_condition_types(condition, bound);
                self.resolve_statement_types(yes, bound);
                if let Some(no) = no {
                    self.resolve_statement_types(no, bound);
                }
            }
            Stmt::For(_, value, body, otherwise) => {
                self.resolve_expression_types(value, bound);
                self.resolve_statement_types(body, bound);
                if let Some(otherwise) = otherwise {
                    self.resolve_statement_types(otherwise, bound);
                }
            }
            Stmt::Labeled(_, body) | Stmt::Loop(body) => self.resolve_statement_types(body, bound),
            _ => {}
        }
    }

    pub(super) fn source_function_name<'a>(&self, name: &'a str) -> std::borrow::Cow<'a, str> {
        let base = tuples::type_parts(name).map_or(name, |(base, _)| base);
        if let Some(original) = self.local_types.names.get(base) {
            std::borrow::Cow::Owned(format!("{}{}", original.text, &name[base.len()..]))
        } else {
            std::borrow::Cow::Borrowed(name)
        }
    }
}
