use super::*;

pub(super) fn fallback(name: &str) -> Option<&'static str> {
    Some(match name {
        "__lt__" => "__ordering_lt__",
        "__le__" => "__ordering_le__",
        "__gt__" => "__ordering_gt__",
        "__ge__" => "__ordering_ge__",
        _ => return None,
    })
}

impl Compiler {
    pub(super) fn resolve_ordering_fallback(
        &mut self,
        name: &str,
        args: &[Value],
        span: &Span,
    ) -> Result<Option<usize>, String> {
        let (Some(helper), [left, right]) = (fallback(name), args) else {
            return Ok(None);
        };
        // Use an explicit comparison before considering an independently
        // derived tuple relation. Derived tuple relations can also operate on
        // fields with only the ordinary comparisons, such as Float.
        let Some(compare) = self.resolve("__cmp__", args, Some("Ordering"), span)? else {
            return Ok(None);
        };
        if self.native_derivation_tasks.contains_key(&compare) {
            return Ok(None);
        }
        self.same("Ordering", &self.functions[compare].result, span)?;
        let name = format!("{helper}<{},{}>", left.ty, right.ty);
        self.resolve(&name, args, Some("Bool"), span)?.map_or_else(
            || Err(span.error(format!("missing standard ordering helper '{helper}'"))),
            |id| {
                self.same("Bool", &self.functions[id].result, span)?;
                Ok(Some(id))
            },
        )
    }
}
