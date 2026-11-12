use super::*;

#[derive(Clone)]
pub(super) struct Callable {
    pub parameters: Vec<String>,
    pub result: String,
    pub function: usize,
}

#[derive(Clone)]
pub(super) struct CaptureContext {
    pub(super) frame: usize,
    pub(super) available: BTreeMap<String, Variable>,
    function: Option<usize>,
    captures: Vec<(Name, Variable)>,
    pub(super) locals: BTreeMap<String, Variable>,
    prologue: Body,
    ty: String,
    span: Span,
}

impl Compiler {
    pub(super) fn begin_named_capture(
        &mut self,
        index: usize,
        available: BTreeMap<String, Variable>,
    ) {
        let sources = self.environments.locals[&index].sources.clone();
        let frame = self.make_capture_frame(available.clone(), sources, Some(index));
        self.capture_contexts.push(CaptureContext {
            frame,
            available,
            function: Some(index),
            captures: vec![],
            locals: BTreeMap::new(),
            prologue: Body::default(),
            ty: String::new(),
            span: self.functions[index].name.span.clone(),
        });
    }

    pub(super) fn finish_named_capture(&mut self, index: usize) -> Vec<(Name, Variable)> {
        let mut capture = self
            .capture_contexts
            .pop()
            .expect("named capture context exists");
        assert_eq!(capture.function, Some(index));
        self.finish_capture_frame(capture.frame, index, &capture.locals);
        if let Some(body) = &mut self.functions[index].checked {
            capture.prologue.instructions.append(&mut body.instructions);
            *body = capture.prologue;
        }
        capture.captures
    }
    pub(super) fn local_variable(&self, name: &str) -> Option<Variable> {
        self.context
            .scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name))
            .filter(|variable| !self.function_shadows_variable(name, variable.scope))
            .cloned()
    }

    pub(super) fn lookup_variable(
        &mut self,
        name: &Name,
        callable_only: bool,
    ) -> Result<Option<Variable>, String> {
        if let Some(variable) = self.local_variable(&name.text) {
            return Ok(Some(variable));
        }
        let Some(context) = self.capture_contexts.last() else {
            return Ok(None);
        };
        if let Some(local) = context.locals.get(&name.text).cloned() {
            if self.function_shadows_variable(&name.text, local.scope) {
                return Ok(None);
            }
            if callable_only && !self.callables.contains_key(&local.ty) {
                return Ok(None);
            }
            self.context.scopes[0].insert(name.text.clone(), local.clone());
            return Ok(Some(local));
        }
        let Some(variable) = context.available.get(&name.text).cloned() else {
            return Ok(None);
        };
        if self.function_shadows_variable(&name.text, variable.scope) {
            return Ok(None);
        }
        if callable_only && !self.callables.contains_key(&variable.ty) {
            return Ok(None);
        }
        if !variable.initialized || variable.ty == crate::ir::hir::DIVERGED {
            return Err(context.span.error(format!(
                "captured variable '{}' used before initialization",
                name.text
            )));
        }
        let env = context.ty.clone();
        let index = context.captures.len();
        let value = Value {
            id: self.temp(),
            ty: variable.ty.clone(),
        };
        let context = self.capture_contexts.last_mut().unwrap();
        context.captures.push((name.clone(), variable.clone()));
        if let Some(function) = context.function {
            context.prologue.push(Instruction::Declare {
                result: value.clone(),
                initial: Some(Value {
                    id: ValueId::Parameter(self.functions[function].params.len() + index),
                    ty: variable.ty.clone(),
                }),
            });
        } else {
            context.prologue.push(Instruction::Object {
                result: value.clone(),
                expression: Rvalue::Field {
                    owner: Value {
                        id: ValueId::Parameter(0),
                        ty: env,
                    },
                    index,
                },
            });
        }
        context.prologue.push(Instruction::DescribeBinding {
            value: value.clone(),
            name: name.clone(),
            captured: true,
        });
        if let Some(info) = &variable.binding {
            context.prologue.push(Instruction::AnnotateBinding {
                value: value.clone(),
                info: info.clone(),
            });
        }
        if let Some(info) = &variable.access {
            context.prologue.push(Instruction::AnnotateAccess {
                value: value.clone(),
                info: info.clone(),
            });
        }
        let local = Variable {
            scope: self.context.declaration_scope,
            id: value.id,
            ty: value.ty,
            initialized: true,
            binding: variable.binding.clone(),
            access: variable.access.clone(),
        };
        self.capture_contexts
            .last_mut()
            .unwrap()
            .locals
            .insert(name.text.clone(), local.clone());
        self.context.scopes[0].insert(name.text.clone(), local.clone());
        Ok(Some(local))
    }

    pub(super) fn invoke(
        &mut self,
        callee: Value,
        args: Vec<Value>,
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Value, String> {
        if self.context.diverged {
            return Ok(Value::never());
        }
        if !self.callables.contains_key(&callee.ty) {
            return Err(span.error(format!("type '{}' is not callable", callee.ty)));
        }

        let callable = self.specialize_callable(
            &callee.ty,
            &args
                .iter()
                .map(|value| value.ty.clone())
                .collect::<Vec<_>>(),
            expected,
            span,
        )?;
        if args.len() != callable.parameters.len() {
            return Err(span.error("callable argument count mismatch"));
        }
        for (value, ty) in args.iter().zip(&callable.parameters) {
            self.same(ty, &value.ty, span)?;
        }
        let name = self.functions[callable.function].name.text.clone();
        let mut values = vec![callee];
        values.extend(args);
        self.call(&name, values, None, span)
    }

    pub(super) fn lambda(
        &mut self,
        parameters: &[(DeclaredName, Name)],
        body: &Expr,
        span: &Span,
    ) -> Result<Value, String> {
        let mut bound = BTreeSet::new();
        for (name, ty) in parameters {
            self.check_type(ty)?;
            if !bound.insert(name.text.clone()) {
                return Err(name.span.error("duplicate lambda parameter"));
            }
        }
        let mut available = self
            .capture_contexts
            .last()
            .map(|c| c.available.clone())
            .unwrap_or_default();
        for scope in &self.context.scopes {
            available.extend(scope.clone());
        }
        let key = format!(
            "{}:{}:{}:{:?}:{:?}:{:?}:{:?}",
            span.file,
            span.line,
            span.column,
            self.context.current_function,
            self.context.declaration_scope,
            parameters.iter().map(|(_, t)| &t.text).collect::<Vec<_>>(),
            available
                .iter()
                .map(|(n, v)| (n, &v.ty, v.initialized))
                .collect::<Vec<_>>()
        );
        let ty = if let Some(ty) = self.lambda_types.get(&key) {
            ty.clone()
        } else {
            if self.callables.len() >= 256 {
                return Err(span.error("callable specialization limit exceeded"));
            }
            let ty = format!("__RuskCallable{}", self.lambda_counter);
            self.lambda_counter += 1;
            let native = format!("RuskRecord{}", self.records.len());
            self.types.insert(ty.clone(), native);
            self.records.insert(
                ty.clone(),
                records::Record {
                    name: Name {
                        text: ty.clone(),
                        span: span.clone(),
                    },
                    fields: vec![],
                },
            );
            let sources = self.visible_capture_sources();
            let frame = self.make_capture_frame(available.clone(), sources, None);
            self.capture_contexts.push(CaptureContext {
                frame,
                function: None,
                available,
                captures: vec![],
                locals: BTreeMap::new(),
                prologue: Body::default(),
                ty: ty.clone(),
                span: span.clone(),
            });
            let mut lambda_context = FunctionContext::lambda(self.context.current_function);
            lambda_context.declaration_scope = self
                .declaration_scopes
                .child(self.context.declaration_scope);
            let (checked, completed) = self.with_function_context(lambda_context, |compiler| {
                for (index, (name, ty)) in parameters.iter().enumerate() {
                    compiler.declare_annotated(
                        name,
                        ty.text.clone(),
                        Some(Value {
                            ty: ty.text.clone(),
                            id: ValueId::Parameter(index + 1),
                        }),
                    )?;
                }
                compiler.expr(body, None)
            });
            let mut context = self.capture_contexts.pop().unwrap();
            // Keep the established capture/copy order independent of first use.
            let original = context
                .captures
                .iter()
                .map(|(n, _)| n.text.clone())
                .collect::<Vec<_>>();
            context
                .captures
                .sort_by(|(a, _), (b, _)| a.text.cmp(&b.text));
            let positions = context
                .captures
                .iter()
                .enumerate()
                .map(|(i, (n, _))| (n.text.clone(), i))
                .collect::<BTreeMap<_, _>>();
            for instruction in &mut context.prologue.instructions {
                if let Instruction::Object {
                    expression: Rvalue::Field { index, .. },
                    ..
                } = instruction
                {
                    *index = positions[&original[*index]];
                }
            }
            let mut checked_body = context.prologue;
            checked_body
                .instructions
                .extend(completed.body.instructions);
            let exits = completed.diverged;
            let inferred = completed.inferred_return;
            let value = checked?;
            self.records.get_mut(&ty).unwrap().fields = context
                .captures
                .iter()
                .map(|(name, var)| {
                    (
                        name.clone(),
                        Name {
                            text: var.ty.clone(),
                            span: name.span.clone(),
                        },
                    )
                })
                .collect();
            self.prepare_record(&ty)?;
            let result = if value.is_never() {
                inferred.clone().unwrap_or("Unit".into())
            } else {
                if let Some(inferred) = &inferred {
                    self.same(inferred, &value.ty, span)?;
                }
                value.ty.clone()
            };
            let index = self.functions.len();
            let f = Function {
                // The checked body keeps lexical resolution, while the hidden
                // invocation entry remains callable after its scope has ended.
                scope: scopes::ScopeId::ROOT,
                name: Name {
                    text: format!("$invoke{index}"),
                    span: span.clone(),
                },
                params: std::iter::once(ty.clone())
                    .chain(parameters.iter().map(|(_, t)| t.text.clone()))
                    .collect(),
                result: result.clone(),
                cpp: format!("rusk_callable_{index}"),
                code: String::new(),
                body: Some((vec![], Box::new(Stmt::Block(vec![], None)))),
                origin: Origin::Generated,
                checked: Some({
                    if !exits {
                        checked_body.push(Instruction::Return {
                            value: if result == "Unit" {
                                None
                            } else {
                                Some(value.clone())
                            },
                            span: Some(body.span.clone()),
                        });
                    }
                    checked_body
                }),
            };
            self.functions.push(f);
            self.finish_capture_frame(context.frame, index, &context.locals);
            self.environments.closures.insert(
                index,
                local_captures::ClosureLayout::new(ty.clone(), self.records[&ty].fields.len()),
            );
            self.callables.insert(
                ty.clone(),
                Callable {
                    parameters: parameters.iter().map(|(_, t)| t.text.clone()).collect(),
                    result,
                    function: index,
                },
            );
            self.lambda_types.insert(key, ty.clone());
            ty
        };
        let function = self.callables[&ty].function;
        let fields =
            self.records[&ty].fields[..self.environments.closures[&function].base_fields].to_vec();
        let mut values = vec![];
        for (name, _) in fields {
            let variable = self
                .lookup_variable(&name, false)?
                .ok_or_else(|| name.span.error("missing captured variable"))?;
            if !variable.initialized || variable.ty == crate::ir::hir::DIVERGED {
                return Err(name.span.error(format!(
                    "captured variable '{}' used before initialization",
                    name.text
                )));
            }
            values.push(Value {
                id: variable.id,
                ty: variable.ty,
            });
        }
        Ok(self.construct_callable(&ty, values, span))
    }

    pub(super) fn function_value(&mut self, name: &Name) -> Result<Value, String> {
        self.family_value(name)
    }
}

impl Compiler {
    pub(super) fn invoke_arguments(
        &mut self,
        callee: Value,
        args: &[Expr],
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Value, String> {
        if self.context.diverged {
            return Ok(Value::never());
        }

        let mut values = vec![];
        for arg in args {
            let hint = self.callable_argument_hint(&callee.ty, args.len(), &values, expected);
            values.push(self.expr(arg, hint.as_deref())?);
        }
        self.invoke(callee, values, expected, span)
    }
    pub(super) fn invoke_expression(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Value, String> {
        if let ExprKind::Field(owner, name) = &callee.kind {
            let owner = self.expr(owner, None)?;
            if !self.callables.contains_key(&owner.ty)
                && let Ok((index, ty)) = self.field(&owner.ty, name)
                && self.callables.contains_key(&ty)
            {
                let field = self.emit(&ty, Rvalue::Field { owner, index });
                return self.invoke_arguments(field, args, expected, span);
            }
            if self
                .lookup_variable(name, true)?
                .is_some_and(|v| self.callables.contains_key(&v.ty))
            {
                let function = self.expr(
                    &Expr {
                        kind: ExprKind::Var(name.clone()),
                        span: name.span.clone(),
                    },
                    None,
                )?;
                let mut values = vec![owner];
                for arg in args {
                    let hint = self.callable_argument_hint(
                        &function.ty,
                        args.len() + 1,
                        &values,
                        expected,
                    );
                    values.push(self.expr(arg, hint.as_deref())?);
                }
                return self.invoke(function, values, expected, span);
            }
            let mut values = vec![owner];
            for arg in args {
                let hints = self.argument_hints(&name.text, args.len() + 1, &values, expected);
                let hint = if hints.len() == 1 {
                    hints.first().map(String::as_str)
                } else {
                    None
                };
                values.push(self.expr(arg, hint)?);
            }
            return self.call(&name.text, values, expected, span);
        }
        let callee = self.expr(callee, None)?;
        self.invoke_arguments(callee, args, expected, span)
    }
}
