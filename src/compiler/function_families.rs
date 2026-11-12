//! A function name denotes a static family, with invocation entries generated
//! for its actual argument types. Aliases keep the original lookup origin.
use super::*;
use callables::Callable;

#[derive(Clone)]
pub(super) struct Family {
    pub name: Name,
    pub origin: scopes::ScopeId,
    pub anchor: usize,
    pub concrete: BTreeSet<usize>,
    pub generic: BTreeSet<usize>,
    pub entries: BTreeMap<usize, Callable>,
}
impl Family {
    pub(super) fn accepts_instance(&self, template: usize, arguments: &[String]) -> bool {
        self.generic.contains(&template)
            && tuples::type_parts(&self.name.text).is_none_or(|(_, explicit)| {
                explicit
                    .iter()
                    .copied()
                    .eq(arguments.iter().map(String::as_str))
            })
    }
}

impl Compiler {
    pub(super) fn callable_argument_hint(
        &mut self,
        ty: &str,
        count: usize,
        known: &[Value],
        expected: Option<&str>,
    ) -> Option<String> {
        let Some(family) = self.families.get(ty).cloned() else {
            return self.callables.get(ty)?.parameters.get(known.len()).cloned();
        };
        let previous = self.context.declaration_scope;
        self.context.declaration_scope = family.origin;
        let hints = self.argument_hints(&family.name.text, count, known, expected);
        self.context.declaration_scope = previous;
        (hints.len() == 1).then(|| hints.into_iter().next().unwrap())
    }

    pub(super) fn family_value(&mut self, name: &Name) -> Result<Value, String> {
        let explicit = tuples::type_parts(&name.text)
            .map(|(_, args)| args.into_iter().map(String::from).collect::<Vec<_>>());
        if let Some(arguments) = &explicit {
            for argument in arguments {
                self.check_type(&Name {
                    text: argument.clone(),
                    span: name.span.clone(),
                })?;
            }
        }
        let scopes = self.function_scopes(&name.text, &[]);
        let lookup = self.source_function_name(&name.text);
        let base = tuples::type_parts(&lookup).map_or(lookup.as_ref(), |(base, _)| base);
        let concrete = self
            .functions
            .iter()
            .enumerate()
            .filter(|(id, f)| {
                scopes.contains(&f.scope)
                    && f.name.text == lookup
                    && !self.generic_calls.contains_key(id)
            })
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        let generic = self
            .generic_functions
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                scopes.contains(&f.scope)
                    && f.name.text == base
                    && explicit
                        .as_ref()
                        .is_none_or(|arguments| arguments.len() == f.parameters.len())
            })
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        if concrete.is_empty() && generic.is_empty() {
            return Err(name
                .span
                .error(format!("unknown variable or function '{}'", name.text)));
        }
        let key = (name.text.clone(), scopes.iter().copied().collect());
        if let Some(ty) = self.family_types.get(&key).cloned() {
            return Ok(self.construct_callable(&ty, vec![], &name.span));
        }
        if self.callables.len() >= 256 {
            return Err(name.span.error("callable specialization limit exceeded"));
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
                    span: name.span.clone(),
                },
                fields: vec![],
            },
        );
        self.prepare_record(&ty)?;
        let anchor = self.functions.len();
        self.functions.push(Function {
            scope: scopes::ScopeId::ROOT,
            name: Name {
                text: format!("$family{anchor}"),
                span: name.span.clone(),
            },
            params: vec![ty.clone()],
            result: "Unit".into(),
            cpp: format!("rusk_family_{anchor}"),
            code: String::new(),
            body: Some((vec![], Box::new(Stmt::Block(vec![], None)))),
            origin: Origin::Generated,
            checked: Some(Body {
                instructions: vec![Instruction::Return {
                    value: None,
                    span: Some(name.span.clone()),
                }],
            }),
        });
        self.environments
            .closures
            .insert(anchor, local_captures::ClosureLayout::new(ty.clone(), 0));
        self.callables.insert(
            ty.clone(),
            Callable {
                parameters: vec![],
                result: "Unit".into(),
                function: anchor,
            },
        );
        self.families.insert(
            ty.clone(),
            Family {
                name: name.clone(),
                origin: *scopes.first().unwrap(),
                anchor,
                concrete,
                generic,
                entries: BTreeMap::new(),
            },
        );
        self.family_types.insert(key, ty.clone());
        Ok(self.construct_callable(&ty, vec![], &name.span))
    }

    pub(super) fn specialize_callable(
        &mut self,
        ty: &str,
        arguments: &[String],
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Callable, String> {
        let Some(family) = self.families.get(ty).cloned() else {
            return self
                .callables
                .get(ty)
                .cloned()
                .ok_or_else(|| span.error(format!("type '{ty}' is not statically callable")));
        };
        let values = arguments
            .iter()
            .enumerate()
            .map(|(i, ty)| Value {
                id: ValueId::Parameter(i + 1),
                ty: ty.clone(),
            })
            .collect::<Vec<_>>();
        let previous = self.context.declaration_scope;
        self.context.declaration_scope = family.origin;
        let target = self.resolve_callable(&family.name.text, &values, expected, span);
        self.context.declaration_scope = previous;
        let target = target?.ok_or_else(|| {
            span.error(format!(
                "no matching function '{}' for ({})",
                family.name.text,
                arguments.join(", ")
            ))
        })?;
        if let Some(entry) = family.entries.get(&target) {
            return Ok(entry.clone());
        }
        if family.entries.len() >= 256 {
            return Err(span.error("callable specialization limit exceeded"));
        }
        let result = self.functions[target].result.clone();
        let index = self.functions.len();
        let value = Value {
            id: self.temp(),
            ty: result.clone(),
        };
        self.record_environment_site(value.id.clone(), span);
        let body = Body {
            instructions: vec![
                Instruction::Call(Call {
                    function: target,
                    args: values,
                    span: span.clone(),
                    inherit_location: true,
                    result: value.clone(),
                }),
                Instruction::Return {
                    value: (result != "Unit").then_some(value),
                    span: Some(span.clone()),
                },
            ],
        };
        self.used.insert(target);
        self.functions.push(Function {
            scope: scopes::ScopeId::ROOT,
            name: Name {
                text: format!("$invoke{index}"),
                span: span.clone(),
            },
            params: std::iter::once(ty.to_string())
                .chain(arguments.iter().cloned())
                .collect(),
            result: result.clone(),
            cpp: format!("rusk_callable_{index}"),
            code: String::new(),
            body: Some((vec![], Box::new(Stmt::Block(vec![], None)))),
            origin: Origin::Generated,
            checked: Some(body),
        });
        self.environments
            .closures
            .insert(index, local_captures::ClosureLayout::new(ty.into(), 0));
        let entry = Callable {
            parameters: arguments.to_vec(),
            result,
            function: index,
        };
        self.families
            .get_mut(ty)
            .unwrap()
            .entries
            .insert(target, entry.clone());
        Ok(entry)
    }
}
