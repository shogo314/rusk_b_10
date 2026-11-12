//! Preserve lexical origins until specialization determines the free variables.
use super::*;

#[derive(Clone)]
pub(in crate::compiler) enum CaptureOrigin {
    Value(Value),
    Frame(usize, String),
}

#[derive(Clone)]
pub(in crate::compiler) enum CaptureReference {
    Value(Value),
    Capture(CaptureKey),
}

pub(super) struct CaptureFrame {
    parent: Option<usize>,
    owner: Option<usize>,
    function: Option<usize>,
    available: BTreeMap<String, Variable>,
    sources: BTreeMap<String, CaptureOrigin>,
    bindings: BTreeMap<String, CaptureReference>,
}

impl Compiler {
    pub(in crate::compiler) fn visible_capture_sources(&self) -> BTreeMap<String, CaptureOrigin> {
        self.visible_variables()
            .into_iter()
            .map(|(name, variable)| {
                let source = if let Some(local) = self.local_variable(&name) {
                    CaptureOrigin::Value(Value {
                        id: local.id,
                        ty: local.ty,
                    })
                } else if let Some(context) = self.capture_contexts.last() {
                    CaptureOrigin::Frame(context.frame, name.clone())
                } else {
                    CaptureOrigin::Value(Value {
                        id: variable.id,
                        ty: variable.ty,
                    })
                };
                (name, source)
            })
            .collect()
    }

    pub(in crate::compiler) fn make_capture_frame(
        &mut self,
        available: BTreeMap<String, Variable>,
        sources: BTreeMap<String, CaptureOrigin>,
        function: Option<usize>,
    ) -> usize {
        let id = self.environments.frames.len();
        self.environments.frames.push(CaptureFrame {
            parent: self.capture_contexts.last().map(|c| c.frame),
            owner: self.context.current_function,
            function,
            available,
            sources,
            bindings: BTreeMap::new(),
        });
        id
    }

    pub(in crate::compiler) fn finish_capture_frame(
        &mut self,
        frame: usize,
        function: usize,
        locals: &BTreeMap<String, Variable>,
    ) {
        let frame = &mut self.environments.frames[frame];
        frame.function = Some(function);
        for (name, local) in locals {
            frame.bindings.insert(
                name.clone(),
                CaptureReference::Value(Value {
                    id: local.id.clone(),
                    ty: local.ty.clone(),
                }),
            );
        }
    }

    fn resolve_capture_origin(
        &mut self,
        origin: CaptureOrigin,
    ) -> Result<CaptureReference, String> {
        match origin {
            CaptureOrigin::Value(value) => Ok(CaptureReference::Value(value)),
            CaptureOrigin::Frame(frame, name) => self.materialize_frame_binding(frame, &name),
        }
    }

    fn materialize_frame_binding(
        &mut self,
        frame: usize,
        name: &str,
    ) -> Result<CaptureReference, String> {
        if let Some(binding) = self.environments.frames[frame].bindings.get(name) {
            return Ok(binding.clone());
        }
        let function = self.environments.frames[frame]
            .function
            .expect("enclosing capture frame has completed");
        let variable = self.environments.frames[frame].available[name].clone();
        let source = self.environments.frames[frame].sources[name].clone();
        let name = Name {
            text: name.into(),
            span: self.functions[function].name.span.clone(),
        };
        if !variable.initialized || variable.ty == crate::ir::hir::DIVERGED {
            return Err(name.span.error(format!(
                "captured variable '{}' used before initialization",
                name.text
            )));
        }
        let source = self.resolve_capture_origin(source)?;
        let closure = self.environments.closures.contains_key(&function);
        let owner = self.environments.frames[frame].owner;
        let owner_frame = self.environments.frames[frame].parent;
        let index = self
            .environments
            .locals
            .entry(function)
            .or_insert_with(|| LocalFunction {
                closure,
                owner,
                owner_frame,
                ..Default::default()
            })
            .captures
            .len();
        let key = CaptureKey { function, index };
        let snapshot = if let Some(template) = self.environments.locals[&function].generic {
            if let Some(snapshot) = self.environments.templates[&template]
                .snapshots
                .get(&name.text)
            {
                snapshot.clone()
            } else {
                let snapshot = (
                    Value {
                        id: self.temp(),
                        ty: variable.ty.clone(),
                    },
                    source.clone(),
                );
                self.environments
                    .templates
                    .get_mut(&template)
                    .unwrap()
                    .snapshots
                    .insert(name.text.clone(), snapshot.clone());
                snapshot
            }
        } else {
            (
                Value {
                    id: self.temp(),
                    ty: variable.ty.clone(),
                },
                source,
            )
        };
        let local = self.environments.locals.get_mut(&function).unwrap();
        local.captures.push((name.clone(), variable.clone()));
        local.snapshots.push(snapshot);
        let binding = if closure {
            CaptureReference::Capture(key)
        } else {
            let value = Value {
                id: self.temp(),
                ty: variable.ty.clone(),
            };
            let mut prologue = Body::default();
            prologue.push(Instruction::Declare {
                result: value.clone(),
                initial: Some(Value {
                    id: ValueId::Parameter(self.functions[function].params.len() + index),
                    ty: variable.ty.clone(),
                }),
            });
            prologue.push(Instruction::DescribeBinding {
                value: value.clone(),
                name: name.clone(),
                captured: true,
            });
            if let Some(info) = &variable.binding {
                prologue.push(Instruction::AnnotateBinding {
                    value: value.clone(),
                    info: info.clone(),
                });
            }
            if let Some(info) = &variable.access {
                prologue.push(Instruction::AnnotateAccess {
                    value: value.clone(),
                    info: info.clone(),
                });
            }
            let body = self.functions[function]
                .checked
                .as_mut()
                .expect("enclosing named body has completed");
            prologue.instructions.append(&mut body.instructions);
            *body = prologue;
            CaptureReference::Value(value)
        };
        self.environments.frames[frame]
            .bindings
            .insert(name.text, binding.clone());
        Ok(binding)
    }

    pub(in crate::compiler) fn register_generic_environment(&mut self, template: usize) {
        self.environments.templates.insert(
            template,
            GenericEnvironment {
                owner: self.context.current_function,
                ..Default::default()
            },
        );
    }

    pub(super) fn declare_generic_environment(&mut self, name: &Name) -> Result<(), String> {
        let Some(template) = self
            .generic_functions
            .iter()
            .enumerate()
            .find(|(id, f)| {
                self.environments.templates.contains_key(id)
                    && f.scope == self.context.declaration_scope
                    && f.name.span.file == name.span.file
                    && f.name.span.line == name.span.line
                    && f.name.span.column == name.span.column
            })
            .map(|(id, _)| id)
        else {
            return Ok(());
        };
        let available = self.visible_variables();
        let sources = self.visible_capture_sources();
        let environment = self.environments.templates.get_mut(&template).unwrap();
        environment.available = Some(available);
        environment.sources = sources;
        environment.owner_frame = self.capture_contexts.last().map(|c| c.frame);
        self.context.ready_generics.insert(template);
        let marker = Value {
            id: self.temp(),
            ty: "Unit".into(),
        };
        self.environments
            .declarations
            .insert(marker.id.clone(), Declaration::Generic(template));
        self.context.body.push(Instruction::Declare {
            result: marker,
            initial: None,
        });
        Ok(())
    }

    pub(in crate::compiler) fn register_generic_instance_environment(
        &mut self,
        function: usize,
        template: usize,
    ) {
        if let Some(environment) = self.environments.templates.get(&template) {
            self.environments.locals.insert(
                function,
                LocalFunction {
                    generic: Some(template),
                    owner: environment.owner,
                    owner_frame: environment.owner_frame,
                    ..Default::default()
                },
            );
        }
    }

    pub(in crate::compiler) fn prepare_generic_capture_source(&mut self, function: usize) {
        if let Some(template) = self
            .environments
            .locals
            .get(&function)
            .and_then(|local| local.generic)
        {
            let environment = &self.environments.templates[&template];
            let local = self.environments.locals.get_mut(&function).unwrap();
            local.available = Some(environment.available.clone().unwrap_or_default());
            local.sources = environment.sources.clone();
            local.owner = environment.owner;
            local.owner_frame = environment.owner_frame;
        }
    }

    pub(in crate::compiler) fn finish_generic_snapshots(
        &mut self,
        function: usize,
    ) -> Result<(), String> {
        let Some(template) = self.environments.locals[&function].generic else {
            return Ok(());
        };
        let captures = self.environments.locals[&function].captures.clone();
        let mut snapshots = vec![];
        for (name, variable) in captures {
            if !self.environments.templates[&template]
                .snapshots
                .contains_key(&name.text)
            {
                let origin = self.environments.templates[&template].sources[&name.text].clone();
                let source = self.resolve_capture_origin(origin)?;
                let snapshot = Value {
                    id: self.temp(),
                    ty: variable.ty,
                };
                self.environments
                    .templates
                    .get_mut(&template)
                    .unwrap()
                    .snapshots
                    .insert(name.text.clone(), (snapshot, source));
            }
            snapshots.push(self.environments.templates[&template].snapshots[&name.text].clone());
        }
        self.environments
            .locals
            .get_mut(&function)
            .unwrap()
            .snapshots = snapshots;
        Ok(())
    }

    pub(super) fn resolve_frame_owners(&mut self) {
        for local in self.environments.locals.values_mut() {
            if let Some(frame) = local.owner_frame {
                local.owner = self.environments.frames[frame].function;
            }
        }
        for environment in self.environments.templates.values_mut() {
            if let Some(frame) = environment.owner_frame {
                environment.owner = self.environments.frames[frame].function;
            }
        }
        for local in self.environments.locals.values_mut() {
            if let Some(template) = local.generic {
                local.owner = self.environments.templates[&template].owner;
            }
        }
    }

    pub(super) fn capture_reference_value(
        &self,
        source: &CaptureReference,
        owner: Option<usize>,
        span: &Span,
    ) -> Result<Value, String> {
        match source {
            CaptureReference::Value(value) => Ok(value.clone()),
            CaptureReference::Capture(key) => self.capture_value(*key, owner, span),
        }
    }

    pub(super) fn capture_declaration_span(&self, marker: &ValueId) -> &Span {
        match self.environments.declarations[marker] {
            Declaration::Function(function) => &self.functions[function].name.span,
            Declaration::Generic(template) => &self.generic_functions[template].name.span,
        }
    }
}
