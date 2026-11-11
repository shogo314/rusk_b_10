//! Embedded resources use the same include/code_file loader as user sources.
include!(concat!(env!("OUT_DIR"), "/stdlib_resources.rs"));

#[cfg(test)]
pub(crate) mod test_source;

pub(crate) fn load() -> Result<crate::compiler::Source, String> {
    let _stage = crate::diagnostics::enter(crate::diagnostics::Stage::StandardLibrary);
    Ok(crate::compiler::Source::standard(
        crate::frontend::source::embedded("prelude.rusk", FILES)?.statements,
    ))
}
pub(crate) fn documented() -> Result<crate::frontend::source::Documented, String> {
    let _stage = crate::diagnostics::enter(crate::diagnostics::Stage::StandardLibrary);
    crate::frontend::source::documented_embedded("prelude.rusk", FILES)
}
#[cfg(test)]
pub(crate) fn runtime() -> Vec<crate::frontend::ast::Stmt> {
    crate::frontend::source::embedded("runtime/prelude.rusk", FILES)
        .unwrap()
        .statements
}

#[cfg(test)]
#[test]
fn prelude_reaches_every_packaged_library_resource() {
    use std::collections::BTreeSet;
    use std::path::Path;

    let loaded = crate::frontend::source::embedded("prelude.rusk", FILES).unwrap();
    let loaded = loaded
        .dependencies
        .iter()
        .map(|p| p.as_path())
        .collect::<BTreeSet<_>>();
    let packaged = FILES
        .iter()
        .map(|(name, _)| Path::new(name))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        loaded, packaged,
        "library files must be reachable through prelude includes or native resources"
    );
}
