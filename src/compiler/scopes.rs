//! Lexical declaration scopes, independent of runtime variable bindings.
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ScopeId(usize);

impl ScopeId {
    pub(super) const ROOT: Self = Self(0);
}

pub(super) struct DeclarationScopes {
    parents: Vec<Option<ScopeId>>,
}

impl Default for DeclarationScopes {
    fn default() -> Self {
        Self {
            parents: vec![None],
        }
    }
}

impl DeclarationScopes {
    pub(super) fn parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.parents[scope.0]
    }
    pub(super) fn child(&mut self, parent: ScopeId) -> ScopeId {
        assert!(parent.0 < self.parents.len(), "invalid declaration scope");
        let id = ScopeId(self.parents.len());
        self.parents.push(Some(parent));
        id
    }

    pub(super) fn ancestor(&self, outer: ScopeId, mut inner: ScopeId) -> bool {
        while let Some(parent) = self.parents[inner.0] {
            if parent == outer {
                return true;
            }
            inner = parent;
        }
        false
    }

    fn nearest(&self, mut origin: ScopeId, available: &BTreeSet<ScopeId>) -> Option<ScopeId> {
        loop {
            if available.contains(&origin) {
                return Some(origin);
            }
            origin = self.parents[origin.0]?;
        }
    }

    fn visible(
        &self,
        origins: impl IntoIterator<Item = ScopeId>,
        available: &BTreeSet<ScopeId>,
    ) -> BTreeSet<ScopeId> {
        let candidates = origins
            .into_iter()
            .filter_map(|origin| self.nearest(origin, available))
            .collect::<BTreeSet<_>>();
        candidates
            .iter()
            .copied()
            .filter(|outer| !candidates.iter().any(|inner| self.ancestor(*outer, *inner)))
            .collect()
    }
}

impl Compiler {
    pub(super) fn function_shadows_variable(&self, name: &str, variable_scope: ScopeId) -> bool {
        self.function_scopes(name, &[])
            .iter()
            .any(|scope| self.declaration_scopes.ancestor(variable_scope, *scope))
    }
    pub(super) fn with_declaration_scope<T>(&mut self, check: impl FnOnce(&mut Self) -> T) -> T {
        let parent = self.context.declaration_scope;
        let ready = self.context.ready_functions.clone();
        let ready_generics = self.context.ready_generics.clone();
        self.context.declaration_scope = self.declaration_scopes.child(parent);
        let result = check(self);
        self.context.declaration_scope = parent;
        self.context.ready_functions = ready;
        self.context.ready_generics = ready_generics;
        result
    }

    pub(super) fn function_scopes(&self, name: &str, arguments: &[String]) -> BTreeSet<ScopeId> {
        self.function_scopes_at(name, arguments, self.context.declaration_scope)
    }

    pub(super) fn function_scopes_at(
        &self,
        name: &str,
        arguments: &[String],
        origin: ScopeId,
    ) -> BTreeSet<ScopeId> {
        let type_base = tuples::type_parts(name).map_or(name, |(base, _)| base);
        let lookup_name = self.source_function_name(name);
        let base = tuples::type_parts(&lookup_name).map_or(lookup_name.as_ref(), |(base, _)| base);
        // Collect by name before considering arity, types, or generic rank.
        let available = self
            .functions
            .iter()
            .enumerate()
            .filter(|(id, f)| f.name.text == lookup_name && !self.generic_calls.contains_key(id))
            .map(|(_, f)| f.scope)
            .chain(
                self.generic_functions
                    .iter()
                    .filter(|f| f.name.text == base)
                    .map(|f| f.scope),
            )
            .collect();
        let constructor_scope = self
            .local_types
            .names
            .contains_key(type_base)
            .then(|| self.type_declaration_scopes[type_base]);
        let origins = std::iter::once(origin)
            .chain(constructor_scope)
            .chain(arguments.iter().map(|ty| {
                let base = tuples::type_parts(ty).map_or(ty.as_str(), |(base, _)| base);
                self.type_declaration_scopes
                    .get(base)
                    .copied()
                    .unwrap_or(ScopeId::ROOT)
            }));
        self.declaration_scopes.visible(origins, &available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_candidates_hide_ancestors_but_keep_unrelated_branches() {
        let mut scopes = DeclarationScopes::default();
        let a = scopes.child(ScopeId::ROOT);
        let b = scopes.child(ScopeId::ROOT);
        let c = scopes.child(a);
        let available = BTreeSet::from([ScopeId::ROOT, a, b, c]);
        assert_eq!(
            scopes.visible([a, c, b], &available),
            BTreeSet::from([b, c])
        );
        assert_eq!(
            scopes.visible([ScopeId::ROOT, a], &available),
            BTreeSet::from([a])
        );
        assert_eq!(scopes.visible([c, c], &available), BTreeSet::from([c]));
    }

    #[test]
    fn each_origin_stops_at_the_nearest_definition_and_does_not_search_siblings() {
        let mut scopes = DeclarationScopes::default();
        let a = scopes.child(ScopeId::ROOT);
        let b = scopes.child(ScopeId::ROOT);
        let c = scopes.child(a);
        assert_eq!(
            scopes.visible([c], &BTreeSet::from([a, b])),
            BTreeSet::from([a])
        );
        assert!(scopes.visible([c], &BTreeSet::from([b])).is_empty());
        assert_eq!(
            scopes.visible([c], &BTreeSet::from([ScopeId::ROOT, b])),
            BTreeSet::from([ScopeId::ROOT])
        );
    }

    fn function(compiler: &mut Compiler, scope: ScopeId, params: &[&str]) -> usize {
        let id = compiler.functions.len();
        compiler.functions.push(Function {
            scope,
            name: Name {
                text: "choose".into(),
                span: Span {
                    file: "scopes.rusk".into(),
                    line: (id + 1) as u32,
                    column: 1,
                },
            },
            params: params.iter().map(|s| (*s).into()).collect(),
            result: "Int".into(),
            cpp: format!("choose_{id}"),
            code: String::new(),
            body: None,
            checked: None,
            origin: Origin::User,
        });
        id
    }

    #[test]
    fn call_resolution_hides_outer_overloads_before_checking_argument_types() {
        let mut compiler = Compiler::default();
        let inner = compiler.declaration_scopes.child(ScopeId::ROOT);
        function(&mut compiler, ScopeId::ROOT, &["Int"]);
        let local = function(&mut compiler, inner, &["Str"]);
        compiler.context.declaration_scope = inner;
        let span = compiler.functions[local].name.span.clone();
        let argument = |ty: &str| Value {
            id: ValueId::Parameter(0),
            ty: ty.into(),
        };
        assert_eq!(
            compiler
                .select("choose", &[argument("Int")], None, &span)
                .unwrap(),
            None
        );
        assert_eq!(
            compiler
                .select("choose", &[argument("Str")], None, &span)
                .unwrap(),
            Some(local)
        );
    }

    #[test]
    fn both_argument_scopes_participate_and_duplicate_paths_do_not_duplicate_functions() {
        let mut compiler = Compiler::default();
        let a = compiler.declaration_scopes.child(ScopeId::ROOT);
        let b = compiler.declaration_scopes.child(ScopeId::ROOT);
        compiler.type_declaration_scopes.insert("A".into(), a);
        compiler.type_declaration_scopes.insert("B".into(), b);
        function(&mut compiler, ScopeId::ROOT, &["A", "B"]);
        let left = function(&mut compiler, a, &["A", "B"]);
        let span = compiler.functions[left].name.span.clone();
        let args = [
            Value {
                id: ValueId::Parameter(0),
                ty: "A".into(),
            },
            Value {
                id: ValueId::Parameter(1),
                ty: "B".into(),
            },
        ];
        assert_eq!(
            compiler.select("choose", &args, None, &span).unwrap(),
            Some(left)
        );
        compiler.context.declaration_scope = a;
        assert_eq!(
            compiler.select("choose", &args, None, &span).unwrap(),
            Some(left)
        );
        function(&mut compiler, b, &["A", "B"]);
        let error = compiler.select("choose", &args, None, &span).unwrap_err();
        assert!(error.contains("ambiguous call"), "{error}");
        assert!(error.contains("scopes.rusk:2:"), "{error}");
        assert!(error.contains("scopes.rusk:3:"), "{error}");
        assert!(
            !error.contains("candidate 'choose' at scopes.rusk:1:"),
            "{error}"
        );
    }

    #[test]
    fn failed_nested_checks_restore_the_lexical_origin() {
        let mut compiler = Compiler::default();
        let result = compiler.with_declaration_scope(|c| {
            assert_ne!(c.context.declaration_scope, ScopeId::ROOT);
            Err::<(), _>("failed child")
        });
        assert_eq!(result, Err("failed child"));
        assert_eq!(compiler.context.declaration_scope, ScopeId::ROOT);
    }
}
