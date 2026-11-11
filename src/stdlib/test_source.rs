//! Source fixtures for tests that deliberately rename declarations or change
//! their origin. Production loads embedded resources through frontend::source.
use std::{collections::BTreeSet, fs, path::Path};

pub(crate) fn source(entry: &str) -> String {
    fn expand(
        root: &Path,
        entry: &Path,
        seen: &mut BTreeSet<std::path::PathBuf>,
        output: &mut String,
    ) {
        let path = entry
            .canonicalize()
            .unwrap_or_else(|error| panic!("{}: {error}", entry.display()));
        assert!(path.starts_with(root), "fixture must stay inside lib/");
        if !seen.insert(path.clone()) {
            return;
        }
        let text =
            fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        if path.file_name().is_some_and(|name| name == "prelude.rusk") {
            // These manifests contain only ordinary include statements and
            // comments. Keep leaf declarations and their property text intact.
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with("//") {
                    continue;
                }
                let child = line
                    .strip_prefix("include \"")
                    .and_then(|p| p.strip_suffix("\";"))
                    .expect("fixture manifest include");
                expand(root, &path.parent().unwrap().join(child), seen, output);
            }
        } else {
            output.push_str(&text);
            if !text.ends_with('\n') {
                output.push('\n');
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("lib")
        .canonicalize()
        .unwrap();
    let mut output = String::new();
    expand(&root, &root.join(entry), &mut BTreeSet::new(), &mut output);
    output
}
