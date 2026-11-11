//! CLI file handling, with per-file atomic replacement for --write.
use crate::{
    cli::FormatArgs,
    diagnostics::{Stage, enter},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

pub(crate) fn run(args: FormatArgs) -> Result<(), String> {
    // Parse and format every input before modifying any file.
    let mut prepared = vec![];
    for path in args.files {
        let _stage = enter(Stage::SourceLoading);
        let original = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let formatted = super::source(&original, &path.to_string_lossy())?;
        prepared.push((path, original, formatted));
    }
    if args.check {
        let changed = prepared
            .iter()
            .filter(|(_, a, b)| a != b)
            .map(|(p, _, _)| p.display().to_string())
            .collect::<Vec<_>>();
        if !changed.is_empty() {
            return Err(format!("formatting differs: {}", changed.join(", ")));
        }
    } else if args.write {
        let _stage = enter(Stage::Output);
        let mut written = std::collections::BTreeSet::new();
        for (path, original, formatted) in prepared {
            if original == formatted {
                continue;
            }
            let target = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            if !written.insert(target.clone()) {
                continue;
            }
            replace(&target, &original, &formatted)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    } else {
        let _stage = enter(Stage::Output);
        std::io::stdout()
            .lock()
            .write_all(prepared[0].2.as_bytes())
            .map_err(|e| format!("stdout: {e}"))?;
    }
    Ok(())
}

fn replace(path: &Path, original: &str, formatted: &str) -> Result<(), String> {
    let permissions = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    if permissions.readonly() {
        return Err("cannot format a read-only file".into());
    }
    let parent = path
        .parent()
        .ok_or("source file must have a parent directory")?;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let (temporary, mut file) = loop {
        let temporary = parent.join(format!(
            ".rusk-fmt-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => break (Temporary(temporary), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    file.write_all(formatted.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    file.set_permissions(permissions)
        .map_err(|e| e.to_string())?;
    drop(file);
    if fs::read_to_string(path).map_err(|e| e.to_string())? != original {
        return Err("source changed while formatting; no replacement made".into());
    }
    fs::rename(&temporary.0, path).map_err(|e| e.to_string())?;
    Ok(())
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
