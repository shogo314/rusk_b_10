//! Source-based API inspection shared by search and documentation generation.
mod catalog;
use crate::{
    cli::{DocArgs, LibraryArgs, SearchArgs},
    diagnostics::{Stage, enter},
};
use catalog::{Catalog, Entry, Origin};
use serde_json::{Value, json};
use std::{fmt::Write as _, io::Write as _};

pub(crate) fn search(args: SearchArgs) -> Result<(), String> {
    let query = args.query.trim().to_lowercase();
    if query.is_empty() {
        return Err("search query is empty".into());
    }
    let catalog = load(&args.library)?;
    let mut results = entries(&catalog, &args.library)
        .filter_map(|entry| rank(entry, &query).map(|rank| (rank, entry)))
        .collect::<Vec<_>>();
    results.sort_by(|(ra, a), (rb, b)| {
        (ra, a.qualified_name(), &a.signature, &a.position).cmp(&(
            rb,
            b.qualified_name(),
            &b.signature,
            &b.position,
        ))
    });
    let mut output = String::new();
    if args.library.json {
        output = pretty(json!({
            "version": 1,
            "entries": results.iter().map(|(_, entry)| entry.json()).collect::<Vec<_>>()
        }))?;
    } else {
        for (_, entry) in results {
            writeln!(output, "{}  [{}]", entry.signature, entry.qualified_name()).unwrap();
            if let Some(summary) = entry.description.as_deref().and_then(summary) {
                writeln!(output, "  {summary}").unwrap();
            }
            writeln!(output, "  {}", location(entry)).unwrap();
        }
    }
    write_output(&output)
}

pub(crate) fn doc(args: DocArgs) -> Result<(), String> {
    let catalog = load(&args.library)?;
    let entries = entries(&catalog, &args.library).collect::<Vec<_>>();
    let output = if args.library.json {
        pretty(json!({
            "version": 1,
            "files": catalog.files.iter().map(|file| json!({
                "origin": file.origin.label(), "file": file.path, "description": file.description
            })).collect::<Vec<_>>(),
            "entries": entries.iter().map(|entry| entry.json()).collect::<Vec<_>>()
        }))?
    } else {
        markdown(&catalog, &entries, args.source_url.as_deref())
    };
    write_output(&output)
}

fn load(args: &LibraryArgs) -> Result<Catalog, String> {
    let _stage = enter(Stage::SourceLoading);
    let mut catalog = Catalog::default();
    if args.file.is_none() || args.stdlib {
        catalog.extend(Origin::Standard, crate::stdlib::documented()?);
    }
    if let Some(file) = &args.file {
        catalog.extend(Origin::User, crate::frontend::source::documented(file)?);
    }
    Ok(catalog)
}

fn entries<'a>(catalog: &'a Catalog, args: &'a LibraryArgs) -> impl Iterator<Item = &'a Entry> {
    catalog
        .entries
        .iter()
        .filter(|entry| args.all || !entry.internal)
}

fn rank(entry: &Entry, query: &str) -> Option<u8> {
    let name = entry.name.to_lowercase();
    let qualified = entry.qualified_name().to_lowercase();
    if name == query || qualified == query {
        Some(0)
    } else if name.starts_with(query) || qualified.starts_with(query) {
        Some(1)
    } else if qualified.contains(query) || entry.signature.to_lowercase().contains(query) {
        Some(2)
    } else if entry
        .description
        .as_ref()
        .is_some_and(|description| description.to_lowercase().contains(query))
    {
        Some(3)
    } else {
        None
    }
}

fn summary(markdown: &str) -> Option<&str> {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

fn markdown(catalog: &Catalog, entries: &[&Entry], source_url: Option<&str>) -> String {
    let mut output = String::from("# APIリファレンス\n\n");
    output.push_str("Ruskソースの宣言と文書コメントから生成した一覧です。\n");
    let mut entries = entries.iter().peekable();
    for file in &catalog.files {
        let belongs_to_file =
            |entry: &Entry| entry.origin == file.origin && entry.position.file == file.path;
        if !entries.peek().is_some_and(|entry| belongs_to_file(entry))
            && file.description.is_empty()
        {
            continue;
        }
        let file_title = standard_path(&file.path).unwrap_or(&file.path);
        writeln!(output, "\n## `{file_title}`\n").unwrap();
        if !file.description.is_empty() {
            writeln!(output, "{}\n", file.description).unwrap();
        }
        while let Some(entry) = entries.next_if(|entry| belongs_to_file(entry)) {
            writeln!(
                output,
                "### `{}` {{ #{id} }}\n\n```text\n{}\n```\n",
                entry.qualified_name(),
                entry.signature,
                id = entry.id,
            )
            .unwrap();
            if let Some(description) = &entry.description {
                writeln!(output, "{description}\n").unwrap();
            }
            if let Some((base, path)) = source_url.zip(standard_path(&entry.position.file)) {
                writeln!(
                    output,
                    "[定義 `{path}:{}`]({}/{}#L{})\n",
                    entry.position.line,
                    base.trim_end_matches('/'),
                    path,
                    entry.position.line,
                )
                .unwrap();
            } else {
                writeln!(output, "定義 `{}`\n", location(entry)).unwrap();
            }
        }
    }
    output
}

fn standard_path(file: &str) -> Option<&str> {
    file.strip_prefix("<stdlib/")?.strip_suffix('>')
}

fn location(entry: &Entry) -> String {
    let span = &entry.position;
    format!("{}:{}:{}", span.file, span.line, span.column)
}

fn pretty(value: Value) -> Result<String, String> {
    serde_json::to_string_pretty(&value)
        .map(|text| text + "\n")
        .map_err(|error| format!("cannot encode API catalog: {error}"))
}

fn write_output(output: &str) -> Result<(), String> {
    let _stage = enter(Stage::Output);
    std::io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| format!("stdout: {error}"))
}
