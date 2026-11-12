//! C++ runtime provider. Rusk HIR/LIR bodies never contain these native sources.
use super::*;
impl Compiler {
    pub(super) fn finalize_native_references(&self, text: &str) -> String {
        let references = self
            .native_requests
            .direct
            .iter()
            .map(|&(index, object)| {
                let name = if self.capture_parameters(index).is_empty() {
                    self.abi_reference_name(index, object)
                } else {
                    format!("rusk_capture_reference_{index}_{object}")
                };
                (format!("rusk_pending_reference_{index}_{object}__"), name)
            })
            .collect::<BTreeMap<_, _>>();
        let mut output = String::with_capacity(text.len());
        let mut remaining = text;
        while let Some(at) = remaining.find("rusk_pending_reference_") {
            output.push_str(&remaining[..at]);
            remaining = &remaining[at..];
            let Some(end) = remaining.find("__") else {
                break;
            };
            let reference = &remaining[..end + 2];
            output.push_str(references.get(reference).map_or(reference, String::as_str));
            remaining = &remaining[end + 2..];
        }
        output.push_str(remaining);
        output
    }

    pub(super) fn cpp_context(&self) -> crate::backend::Context {
        let functions = self
            .functions
            .iter()
            .enumerate()
            .map(|(id, f)| crate::backend::FunctionInfo {
                cpp: if f.checked.is_none() && !self.capture_parameters(id).is_empty() {
                    format!("rusk_native_environment_{id}")
                } else {
                    f.cpp.clone()
                },
                result: f.result.clone(),
                unit_object: self.native_unit_object(id),
                trusted: f.body.is_none()
                    && (f.origin == Origin::StandardLibrary || self.native_lowering(id).is_some()),
                lowering: self
                    .capture_parameters(id)
                    .is_empty()
                    .then(|| self.native_lowering(id).cloned())
                    .flatten(),
                captures: self.native_capture_keys(id),
                native_entry: (f.checked.is_none()
                    && self.used.contains(&id)
                    && !self.capture_parameters(id).is_empty())
                .then(|| f.cpp.clone()),
            })
            .collect();
        let mut prelude = String::new();
        for name in &self.symbol_order {
            prelude += &self.symbols[name].code;
            prelude += "\n";
        }
        prelude += &self.enum_forwards();
        prelude += &self.record_forwards();
        if !self.native_type_prelude.is_empty() {
            prelude += &self.native_type_prelude;
        }
        prelude += &self.enum_cpp();
        prelude += &self.record_cpp();
        prelude += &self.scalar_representation_checks();
        let (concrete_foreign_definitions, foreign_definitions): (Vec<_>, Vec<_>) = self
            .used
            .iter()
            .filter(|&&id| self.functions[id].checked.is_none())
            .partition(|&&id| {
                self.functions[id].origin != Origin::Generated
                    && !self.generic_calls.contains_key(&id)
                    && !self.native_derived_functions.contains(&id)
            });
        let definitions = |ids: Vec<&usize>| {
            ids.into_iter()
                .map(|&id| self.functions[id].code.clone())
                .collect()
        };
        let epilogue = self.enum_copy_cpp() + &self.record_copy_cpp();
        crate::backend::Context {
            types: self.types.clone(),
            functions,
            prelude,
            concrete_foreign_definitions: definitions(concrete_foreign_definitions),
            foreign_definitions: definitions(foreign_definitions),
            native_unit_bridges: self.native_requests.unit_bridges.clone(),
            native_direct_references: self.native_requests.direct.clone(),
            native_value_references: self.native_requests.values.clone(),
            native_owned_references: self.native_requests.owned.clone(),
            native_rooted_references: self.native_requests.rooted.clone(),
            native_effect_references: self.native_requests.effects.clone(),
            native_declarations: self
                .native_requests
                .declarations
                .iter()
                .copied()
                .chain(
                    self.generic_instances
                        .values()
                        .copied()
                        .filter(|&id| self.functions[id].body.is_none() && self.used.contains(&id)),
                )
                .collect(),
            epilogue,
        }
    }
}
