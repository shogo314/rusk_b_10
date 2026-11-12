//! Closure conversion for named local functions. Source signatures stay intact;
//! environment arguments are attached only after all bodies have been checked.
use super::*;
mod frames;
pub(super) use frames::{CaptureOrigin, CaptureReference};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct CaptureKey {
    function: usize,
    index: usize,
}

#[derive(Clone, Default)]
pub(super) struct LocalFunction {
    pub available: Option<BTreeMap<String, Variable>>,
    pub captures: Vec<(Name, Variable)>,
    pub snapshots: Vec<(Value, CaptureReference)>,
    pub sources: BTreeMap<String, CaptureOrigin>,
    pub generic: Option<usize>,
    pub closure: bool,
    pub owner_frame: Option<usize>,
    pub owner: Option<usize>,
}

#[derive(Clone)]
pub(super) struct CaptureSite {
    ready: BTreeSet<usize>,
    ready_generics: BTreeSet<usize>,
    span: Span,
}

pub(super) struct ClosureLayout {
    pub ty: String,
    pub base_fields: usize,
    values: BTreeMap<CaptureKey, Value>,
}
impl ClosureLayout {
    pub(super) fn new(ty: String, base_fields: usize) -> Self {
        Self {
            ty,
            base_fields,
            values: BTreeMap::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct Environments {
    pub locals: BTreeMap<usize, LocalFunction>,
    sites: BTreeMap<ValueId, CaptureSite>,
    declarations: BTreeMap<ValueId, Declaration>,
    pub templates: BTreeMap<usize, GenericEnvironment>,
    frames: Vec<frames::CaptureFrame>,
    creations: BTreeMap<ValueId, usize>,
    pub closures: BTreeMap<usize, ClosureLayout>,
    required: BTreeMap<usize, Vec<CaptureKey>>,
    aliases: BTreeMap<CaptureKey, CaptureKey>,
    reachable: BTreeMap<usize, BTreeSet<usize>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Declaration {
    Function(usize),
    Generic(usize),
}

#[derive(Default)]
pub(super) struct GenericEnvironment {
    available: Option<BTreeMap<String, Variable>>,
    sources: BTreeMap<String, CaptureOrigin>,
    snapshots: BTreeMap<String, (Value, CaptureReference)>,
    pub owner: Option<usize>,
    owner_frame: Option<usize>,
}

fn walk(body: &Body, visit: &mut impl FnMut(&Instruction)) {
    for op in &body.instructions {
        visit(op);
        match op {
            Instruction::Scope(body) | Instruction::Loop { body, .. } => walk(body, visit),
            Instruction::If { yes, no, .. } => {
                walk(yes, visit);
                walk(no, visit);
            }
            _ => {}
        }
    }
}

impl Compiler {
    pub(super) fn visible_variables(&self) -> BTreeMap<String, Variable> {
        let mut available = self
            .capture_contexts
            .last()
            .map(|c| c.available.clone())
            .unwrap_or_default();
        for scope in &self.context.scopes {
            available.extend(scope.clone());
        }
        available
    }

    pub(super) fn record_environment_site(&mut self, id: ValueId, span: &Span) {
        self.environments.sites.insert(
            id,
            CaptureSite {
                ready: self.context.ready_functions.clone(),
                ready_generics: self.context.ready_generics.clone(),
                span: span.clone(),
            },
        );
    }

    pub(super) fn construct_callable(
        &mut self,
        ty: &str,
        fields: Vec<Value>,
        span: &Span,
    ) -> Value {
        let value = self.emit(ty, Rvalue::Aggregate(fields));
        let function = self.callables[ty].function;
        self.environments
            .creations
            .insert(value.id.clone(), function);
        self.record_environment_site(value.id.clone(), span);
        value
    }

    pub(super) fn declare_local_function(
        &mut self,
        name: &Name,
        generic: bool,
    ) -> Result<(), String> {
        if generic {
            return self.declare_generic_environment(name);
        }
        let index = self
            .functions
            .iter()
            .enumerate()
            .find(|(id, f)| {
                self.environments.locals.contains_key(id)
                    && f.scope == self.context.declaration_scope
                    && f.name.span.file == name.span.file
                    && f.name.span.line == name.span.line
                    && f.name.span.column == name.span.column
            })
            .map(|(id, _)| id);
        let Some(index) = index else {
            return Ok(());
        };
        self.environments.locals.get_mut(&index).unwrap().available =
            Some(self.visible_variables());
        self.environments.locals.get_mut(&index).unwrap().sources = self.visible_capture_sources();
        self.environments
            .locals
            .get_mut(&index)
            .unwrap()
            .owner_frame = self.capture_contexts.last().map(|c| c.frame);
        self.context.ready_functions.insert(index);
        self.compile_function(index)?;
        // Materialize captures in the enclosing checker as well. This matters
        // when a nested function alone uses a variable from a grandparent.
        let captures = self.environments.locals[&index].captures.clone();
        let mut snapshots = vec![];
        for (name, _) in captures {
            let variable = self
                .lookup_variable(&name, false)?
                .ok_or_else(|| name.span.error("missing captured variable"))?;
            if !variable.initialized {
                return Err(name
                    .span
                    .error("captured variable used before initialization"));
            }
            let source = Value {
                id: variable.id,
                ty: variable.ty,
            };
            let snapshot = Value {
                id: self.temp(),
                ty: source.ty.clone(),
            };
            snapshots.push((snapshot, CaptureReference::Value(source)));
        }
        let marker = Value {
            id: self.temp(),
            ty: "Unit".into(),
        };
        self.environments.locals.get_mut(&index).unwrap().snapshots = snapshots;
        self.environments
            .declarations
            .insert(marker.id.clone(), Declaration::Function(index));
        self.context.body.push(Instruction::Declare {
            result: marker,
            initial: None,
        });
        Ok(())
    }

    pub(super) fn prepare_capture_layouts(&mut self) -> Result<bool, String> {
        self.resolve_frame_owners();
        let mut edges = BTreeMap::<usize, BTreeSet<usize>>::new();
        for family in self.families.values() {
            let targets = edges.entry(family.anchor).or_default();
            targets.extend(&family.concrete);
            targets.extend(
                self.generic_instances
                    .iter()
                    .filter(|((template, arguments), _)| {
                        family.accepts_instance(*template, arguments)
                    })
                    .map(|(_, &id)| id),
            );
        }
        let mut owners = BTreeMap::new();
        for (id, f) in self.functions.iter().enumerate() {
            if let Some(body) = &f.checked {
                walk(body, &mut |op| {
                    self.capture_edges(Some(id), op, &mut edges, &mut owners)
                });
            } else {
                for dependency in self.native_capture_dependencies(id) {
                    if !self.environments.closures.contains_key(&dependency)
                        || self.family_capture_mode(dependency).is_some()
                    {
                        edges.entry(id).or_default().insert(dependency);
                    }
                }
            }
        }
        walk(&self.context.body, &mut |op| {
            self.capture_edges(None, op, &mut edges, &mut owners)
        });
        for (declaration, owner) in owners {
            match declaration {
                Declaration::Function(function) => {
                    self.environments.locals.get_mut(&function).unwrap().owner = owner
                }
                Declaration::Generic(template) => {
                    self.environments
                        .templates
                        .get_mut(&template)
                        .unwrap()
                        .owner = owner;
                    for local in self.environments.locals.values_mut() {
                        if local.generic == Some(template) {
                            local.owner = owner;
                        }
                    }
                }
            }
        }
        let mut reachable = edges.clone();
        loop {
            let before = reachable.clone();
            for targets in reachable.values_mut() {
                let mut added = BTreeSet::new();
                for target in targets.iter() {
                    added.extend(before.get(target).into_iter().flatten().copied());
                }
                targets.extend(added);
            }
            if reachable == before {
                break;
            }
        }
        // Concrete instances of one generic declaration share its snapshots.
        // Keep one environment slot for each snapshot so copying a callable
        // does not split that declaration's state between its instances.
        let mut snapshots = BTreeMap::new();
        self.environments.aliases.clear();
        for (&id, local) in &self.environments.locals {
            for (index, (value, _)) in local.snapshots.iter().enumerate() {
                let key = CaptureKey {
                    function: id,
                    index,
                };
                let canonical = *snapshots.entry(value.id.clone()).or_insert(key);
                self.environments.aliases.insert(key, canonical);
            }
        }
        let mut required = BTreeMap::<usize, BTreeSet<CaptureKey>>::new();
        for (&id, local) in &self.environments.locals {
            required.insert(
                id,
                (0..local.captures.len())
                    .map(|index| {
                        self.environments.aliases[&CaptureKey {
                            function: id,
                            index,
                        }]
                    })
                    .collect(),
            );
        }
        loop {
            let before = required.clone();
            for (&id, targets) in &edges {
                let mut added = BTreeSet::new();
                for target in targets {
                    for key in before.get(target).into_iter().flatten() {
                        if let Some((origin, creation)) = self.family_capture_mode(*target) {
                            let held = self.capture_is_lexical(*key, origin);
                            if held != creation {
                                continue;
                            }
                        }
                        if self.environments.locals[&key.function].owner != Some(id) {
                            added.insert(*key);
                        }
                    }
                }
                required.entry(id).or_default().extend(added);
            }
            if required == before {
                break;
            }
        }
        self.environments.reachable = reachable;
        self.environments.required = required
            .into_iter()
            .map(|(id, keys)| {
                let direct = self
                    .environments
                    .locals
                    .get(&id)
                    .map_or(0, |f| f.captures.len());
                let mut ordered = (0..direct)
                    .map(|index| {
                        self.environments.aliases[&CaptureKey {
                            function: id,
                            index,
                        }]
                    })
                    .collect::<Vec<_>>();
                for key in keys {
                    if !ordered.contains(&key) {
                        ordered.push(key);
                    }
                }
                (id, ordered)
            })
            .collect();
        let mut changed = false;
        let mut shared = BTreeMap::<String, BTreeSet<CaptureKey>>::new();
        for (&id, closure) in &self.environments.closures {
            let family = self.families.get(&closure.ty);
            if family.is_some_and(|family| family.anchor != id) {
                continue;
            }
            let keys = self
                .environments
                .required
                .get(&id)
                .into_iter()
                .flatten()
                .copied()
                .filter(|key| {
                    family.is_none_or(|family| self.capture_is_lexical(*key, family.origin))
                });
            shared.entry(closure.ty.clone()).or_default().extend(keys);
        }
        for (&id, closure) in &self.environments.closures {
            if !self.families.contains_key(&closure.ty) {
                self.environments
                    .required
                    .insert(id, shared[&closure.ty].iter().copied().collect());
            }
        }
        let closures = self
            .environments
            .closures
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in closures {
            let ty = self.environments.closures[&id].ty.clone();
            let required = shared[&ty].iter().copied().collect::<Vec<_>>();
            self.environments
                .closures
                .get_mut(&id)
                .unwrap()
                .values
                .retain(|key, _| shared[&ty].contains(key));
            let base = self.environments.closures[&id].base_fields;
            let mut fields = self.records[&ty].fields[..base].to_vec();
            for key in required {
                let (name, variable) =
                    self.environments.locals[&key.function].captures[key.index].clone();
                fields.push((
                    Name {
                        text: format!("$capture{}_{}", key.function, key.index),
                        span: name.span.clone(),
                    },
                    Name {
                        text: variable.ty.clone(),
                        span: name.span,
                    },
                ));
                if !self.environments.closures[&id].values.contains_key(&key) {
                    let value = Value {
                        id: self.temp(),
                        ty: variable.ty,
                    };
                    self.environments
                        .closures
                        .get_mut(&id)
                        .unwrap()
                        .values
                        .insert(key, value);
                }
            }
            if self.records[&ty]
                .fields
                .iter()
                .map(|(name, ty)| (&name.text, &ty.text))
                .ne(fields.iter().map(|(name, ty)| (&name.text, &ty.text)))
            {
                self.records.get_mut(&ty).unwrap().fields = fields;
                self.record_copies.remove(&ty);
                for (&function, (_, target)) in &self.native_derivation_tasks {
                    if target == &ty {
                        self.native_derived_functions.remove(&function);
                    }
                }
                changed = true;
            }
        }
        if changed {
            for closure in self.environments.closures.values() {
                self.check_record_cycle(&self.records[&closure.ty].name, &mut vec![])?;
            }
        }
        // Validate readiness before entering HIR validation or the C++ backend.
        for (id, f) in self.functions.iter().enumerate() {
            if let Some(body) = &f.checked {
                self.convert_capture_body(body, Some(id))?;
            }
        }
        self.convert_capture_body(&self.context.body, None)?;
        Ok(changed)
    }

    fn capture_edges(
        &self,
        owner: Option<usize>,
        op: &Instruction,
        edges: &mut BTreeMap<usize, BTreeSet<usize>>,
        owners: &mut BTreeMap<Declaration, Option<usize>>,
    ) {
        let result = match op {
            Instruction::Call(call) => {
                if (!self.environments.closures.contains_key(&call.function)
                    || self.family_capture_mode(call.function).is_some())
                    && let Some(owner) = owner
                {
                    edges.entry(owner).or_default().insert(call.function);
                }
                None
            }
            Instruction::Object { result, .. } | Instruction::Declare { result, .. } => {
                Some(result)
            }
            _ => None,
        };
        if let Some(result) = result {
            if let Some(&function) = self.environments.creations.get(&result.id)
                && let Some(owner) = owner
            {
                edges.entry(owner).or_default().insert(function);
            }
            if let Some(&function) = self.environments.creations.get(&result.id)
                && self
                    .environments
                    .locals
                    .get(&function)
                    .is_some_and(|local| local.closure)
            {
                owners.insert(Declaration::Function(function), owner);
            }
            if let Some(&function) = self.environments.declarations.get(&result.id) {
                owners.insert(function, owner);
            }
        }
    }

    fn capture_value(
        &self,
        key: CaptureKey,
        owner: Option<usize>,
        span: &Span,
    ) -> Result<Value, String> {
        let key = self.environments.aliases.get(&key).copied().unwrap_or(key);
        if self.environments.locals[&key.function].owner == owner {
            if self.environments.locals[&key.function].closure {
                return self.capture_reference_value(
                    &self.environments.locals[&key.function].snapshots[key.index].1,
                    owner,
                    span,
                );
            }
            return Ok(self.environments.locals[&key.function].snapshots[key.index]
                .0
                .clone());
        }
        let Some(owner) = owner else {
            return Err(span.error(format!(
                "capture environment for '{}' is not available in this scope",
                self.functions[key.function].name.text
            )));
        };
        if let Some(closure) = self.environments.closures.get(&owner)
            && let Some(value) = closure.values.get(&key)
        {
            return Ok(value.clone());
        }
        let index = self
            .capture_abi_keys(owner)
            .iter()
            .position(|k| *k == key)
            .expect("capture dependency was propagated");
        Ok(Value {
            id: ValueId::Parameter(self.functions[owner].params.len() + index),
            ty: self.environments.locals[&key.function].captures[key.index]
                .1
                .ty
                .clone(),
        })
    }

    fn check_environment_site(
        &self,
        target: usize,
        owner: Option<usize>,
        site: Option<&CaptureSite>,
        wanted: &BTreeSet<CaptureKey>,
    ) -> Result<(), String> {
        let Some(site) = site else {
            return Ok(());
        };
        for function in std::iter::once(&target).chain(
            self.environments
                .reachable
                .get(&target)
                .into_iter()
                .flatten(),
        ) {
            if let Some(local) = self.environments.locals.get(function)
                && !local.closure
                && local.owner == owner
                && self.environments.required.get(function).is_some_and(|r| {
                    r.iter().any(|key| {
                        wanted.contains(key)
                            && self.capture_is_lexical(*key, self.functions[*function].scope)
                    })
                })
                && !local.generic.map_or_else(
                    || site.ready.contains(function),
                    |template| site.ready_generics.contains(&template),
                )
            {
                let name = &self.functions[*function].name;
                return Err(site.span.error(format!(
                    "capture environment for '{}' is not ready; declaration at {}:{}:{}",
                    name.text, name.span.file, name.span.line, name.span.column
                )));
            }
        }
        Ok(())
    }

    fn capture_abi_keys(&self, id: usize) -> Vec<CaptureKey> {
        self.environments
            .required
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|key| {
                self.environments
                    .closures
                    .get(&id)
                    .is_none_or(|closure| !closure.values.contains_key(key))
            })
            .copied()
            .collect()
    }

    fn family_capture_mode(&self, id: usize) -> Option<(scopes::ScopeId, bool)> {
        let closure = self.environments.closures.get(&id)?;
        let family = self.families.get(&closure.ty)?;
        Some((family.origin, id == family.anchor))
    }

    fn capture_is_lexical(&self, key: CaptureKey, origin: scopes::ScopeId) -> bool {
        let scope = self.functions[key.function].scope;
        scope == origin || self.declaration_scopes.ancestor(scope, origin)
    }

    pub(super) fn capture_parameters(&self, id: usize) -> Vec<String> {
        self.capture_abi_keys(id)
            .iter()
            .map(|key| {
                self.environments.locals[&key.function].captures[key.index]
                    .1
                    .ty
                    .clone()
            })
            .collect()
    }

    pub(super) fn native_capture_keys(&self, id: usize) -> Vec<(usize, usize)> {
        self.capture_abi_keys(id)
            .iter()
            .map(|key| (key.function, key.index))
            .collect()
    }

    pub(super) fn native_capture_dependencies(&self, id: usize) -> BTreeSet<usize> {
        let mut references = self.native_dependencies(id).cloned().unwrap_or_default();
        references.extend(self.native_mappings(id).iter().map(|m| m.function));
        let function = &self.functions[id];
        let mut types = function
            .params
            .iter()
            .chain(std::iter::once(&function.result))
            .cloned()
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(ty) = types.pop() {
            if !seen.insert(ty.clone()) {
                continue;
            }
            if let Some(binding) = self.native_type_bindings.get(&ty) {
                references.extend(&binding.references);
            }
            types.extend(
                self.native_type_dependencies
                    .get(&ty)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        references
    }

    pub(super) fn convert_capture_body(
        &self,
        body: &Body,
        owner: Option<usize>,
    ) -> Result<Body, String> {
        let mut result = Body::default();
        if let Some(id) = owner
            && let Some(closure) = self.environments.closures.get(&id)
        {
            for (index, (key, value)) in closure.values.iter().enumerate() {
                if !self
                    .environments
                    .required
                    .get(&id)
                    .is_some_and(|keys| keys.contains(key))
                {
                    continue;
                }
                result.push(Instruction::Object {
                    result: value.clone(),
                    expression: Rvalue::Field {
                        owner: Value {
                            id: ValueId::Parameter(0),
                            ty: closure.ty.clone(),
                        },
                        index: closure.base_fields + index,
                    },
                });
            }
        }
        self.convert_capture_instructions(body, owner, &mut result)?;
        Ok(result)
    }

    fn convert_capture_instructions(
        &self,
        body: &Body,
        owner: Option<usize>,
        result: &mut Body,
    ) -> Result<(), String> {
        for instruction in &body.instructions {
            let mut op = instruction.clone();
            match &mut op {
                Instruction::Declare { result: marker, .. }
                    if self.environments.declarations.contains_key(&marker.id) =>
                {
                    let snapshots = match self.environments.declarations[&marker.id] {
                        Declaration::Function(function) => self.environments.locals[&function]
                            .snapshots
                            .iter()
                            .collect::<Vec<_>>(),
                        Declaration::Generic(template) => self.environments.templates[&template]
                            .snapshots
                            .values()
                            .collect(),
                    };
                    for (snapshot, source) in snapshots {
                        result.push(Instruction::Declare {
                            result: snapshot.clone(),
                            initial: Some(self.capture_reference_value(
                                source,
                                owner,
                                self.capture_declaration_span(&marker.id),
                            )?),
                        });
                    }
                    continue;
                }
                Instruction::Call(call) => {
                    let keys = self.capture_abi_keys(call.function);
                    self.check_environment_site(
                        call.function,
                        owner,
                        self.environments.sites.get(&call.result.id),
                        &keys.iter().copied().collect(),
                    )?;
                    for key in keys {
                        call.args.push(self.capture_value(key, owner, &call.span)?);
                    }
                }
                Instruction::Object {
                    result: value,
                    expression: Rvalue::Aggregate(fields),
                } => {
                    if let Some(&function) = self.environments.creations.get(&value.id) {
                        let keys = self.environments.closures[&function]
                            .values
                            .keys()
                            .copied()
                            .collect::<BTreeSet<_>>();
                        self.check_environment_site(
                            function,
                            owner,
                            self.environments.sites.get(&value.id),
                            &keys,
                        )?;
                        for key in &keys {
                            fields.push(self.capture_value(
                                *key,
                                owner,
                                &self.environments.sites[&value.id].span,
                            )?);
                        }
                    }
                }
                Instruction::Scope(body) | Instruction::Loop { body, .. } => {
                    let mut converted = Body::default();
                    self.convert_capture_instructions(body, owner, &mut converted)?;
                    *body = converted;
                }
                Instruction::If { yes, no, .. } => {
                    let mut a = Body::default();
                    let mut b = Body::default();
                    self.convert_capture_instructions(yes, owner, &mut a)?;
                    self.convert_capture_instructions(no, owner, &mut b)?;
                    *yes = a;
                    *no = b;
                }
                _ => {}
            }
            result.push(op);
        }
        Ok(())
    }
}
