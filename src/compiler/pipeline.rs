//! Body completion, intermediate representations, and analysis phases.
use super::*;

impl Compiler {
    pub(super) fn prepare_bodies(&mut self, sources: &[Source]) -> Result<(), String> {
        // Check every body, including unused functions, in an isolated variable scope.
        for i in 0..self.functions.len() {
            if self.functions[i].body.is_some() {
                self.compile_function(i)?;
            }
        }
        // Executable top-level bindings live in main, inside the declaration
        // root. They may shadow library functions without becoming captures of
        // top-level named functions.
        self.context.declaration_scope = self.declaration_scopes.child(scopes::ScopeId::ROOT);
        self.context.scopes.push(BTreeMap::new());
        for source in sources {
            for s in source {
                self.statement(s)?;
            }
        }
        let main_body = std::mem::take(&mut self.context.body);
        self.context.diverged = false;
        self.complete_instantiations()?;
        self.context.body = main_body;
        while self.prepare_capture_layouts()? {
            let main_body = std::mem::take(&mut self.context.body);
            let result = self.complete_instantiations();
            self.context.body = main_body;
            result?;
        }
        self.context.body.push(Instruction::Return {
            value: None,
            span: None,
        });
        Ok(())
    }
    fn complete_instantiations(&mut self) -> Result<(), String> {
        let mut index = 0;
        loop {
            while index < self.functions.len() {
                if self.functions[index].body.is_some()
                    && self.functions[index].checked.is_none()
                    && let Err(error) = self.compile_function(index)
                {
                    if let Some(span) = self.generic_calls.get(&index) {
                        return Err(format!(
                            "{error}\nwhile instantiating '{}' at {}:{}:{}",
                            self.generic_instances
                                .iter()
                                .find(|(_, value)| **value == index)
                                .map(|((template, arguments), _)| format!(
                                    "{}<{}>",
                                    self.generic_functions[*template].name.text,
                                    arguments
                                        .iter()
                                        .map(|argument| self.diagnostic_type(argument))
                                        .collect::<Vec<_>>()
                                        .join(",")
                                ))
                                .unwrap_or_else(|| self.functions[index].name.text.clone()),
                            span.file,
                            span.line,
                            span.column
                        ));
                    }
                    return Err(error);
                }
                index += 1;
            }
            self.prepare_record_copies()?;
            self.prepare_enum_copies()?;
            self.prepare_derived_functions()?;
            self.prepare_native_types()?;
            if index == self.functions.len()
                && self
                    .native_type_bindings
                    .values()
                    .all(|binding| binding.native.is_some())
                && self.record_copy_functions.iter().all(|(ty, id)| {
                    !self.native_operation_needed(*id) || self.record_copies.contains_key(ty)
                })
                && self.enum_copy_functions.iter().all(|(ty, id)| {
                    !self.native_operation_needed(*id) || self.enum_copies.contains_key(ty)
                })
                && self.native_derivation_tasks.keys().all(|id| {
                    !self.native_operation_needed(*id) || self.native_derived_functions.contains(id)
                })
            {
                break;
            }
        }
        Ok(())
    }
    pub(super) fn build_hir(&mut self) -> Result<crate::ir::hir::Program, String> {
        let mut hir = crate::ir::hir::Program {
            types: self.ir_types(),
            functions: self
                .functions
                .iter()
                .enumerate()
                .map(|(id, f)| {
                    Ok(crate::ir::hir::Function {
                        id,
                        name: f.name.text.clone(),
                        params: f
                            .params
                            .iter()
                            .cloned()
                            .chain(self.capture_parameters(id))
                            .collect(),
                        result: f.result.clone(),
                        body: f
                            .checked
                            .as_ref()
                            .map(|body| self.convert_capture_body(body, Some(id)))
                            .transpose()?,
                        access: crate::analysis::readonly::Summary::default(),
                        effects: self.native_environment_contract(id).unwrap_or_else(|| {
                            crate::analysis::effects::Summary::unanalyzed(
                                f.params.len() + self.capture_parameters(id).len(),
                            )
                        }),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            main: self.convert_capture_body(&self.context.body, None)?,
        };
        hir.prune_unreachable();
        Ok(hir)
    }
    pub(super) fn analyze_program(
        &self,
        hir: &mut crate::ir::hir::Program,
    ) -> Result<crate::ir::lir::Program, String> {
        use crate::diagnostics::{Stage, enter};
        let conditional = self
            .native_info
            .iter()
            .filter_map(|(&id, info)| {
                if info.dependencies.is_none() && self.capture_parameters(id).is_empty() {
                    return None;
                }
                let contract = info.contract.as_ref()?;
                (contract.source == crate::analysis::effects::SummarySource::NativeContract)
                    .then_some(id)
            })
            .collect::<BTreeSet<_>>();
        for &id in &conditional {
            hir.functions[id].effects =
                crate::analysis::effects::Summary::unanalyzed(hir.functions[id].params.len());
        }
        hir.assert_valid();
        let mut lir = {
            let _stage = enter(Stage::Lir);
            crate::ir::lir::lower_program(hir)
        };
        lir.assert_valid();
        lir.validate_bindings()?;
        {
            let _stage = enter(Stage::Effects);
            self.resolve_effects(hir, &mut lir, conditional)?;
        }
        hir.assert_valid();
        lir.assert_valid();
        {
            let _stage = enter(Stage::Effects);
            crate::analysis::effects::validate_noescape(&lir)?;
        }
        let access = {
            let _stage = enter(Stage::Readonly);
            crate::analysis::readonly::analyze(&lir)?
        };
        for (id, summary) in access.into_iter().enumerate() {
            hir.functions[id].access = summary.clone();
            lir.functions[id].access = summary;
        }
        hir.assert_valid();
        lir.assert_valid();
        Ok(lir)
    }
    fn resolve_effects(
        &self,
        hir: &mut crate::ir::hir::Program,
        lir: &mut crate::ir::lir::Program,
        mut conditional: BTreeSet<usize>,
    ) -> Result<(), String> {
        let mut summaries = crate::analysis::effects::analyze(lir);
        loop {
            let proven = conditional
                .iter()
                .copied()
                .filter_map(|id| {
                    let composed = self.compose_native_environment_contract(id, &summaries)?;
                    Some((id, composed))
                })
                .collect::<Vec<_>>();
            if proven.is_empty() {
                break;
            }
            for (id, composed) in proven {
                composed.validate_signature(
                    &lir.functions[id].params,
                    &lir.functions[id].result,
                    &lir.types,
                    false,
                )?;
                conditional.remove(&id);
                lir.functions[id].effects = composed;
            }
            summaries = crate::analysis::effects::analyze(lir);
        }
        for (id, summary) in summaries.into_iter().enumerate() {
            hir.functions[id].effects = summary.clone();
            lir.functions[id].effects = summary;
        }
        Ok(())
    }
}
