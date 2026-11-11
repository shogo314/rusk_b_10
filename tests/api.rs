mod support;

use serde_json::Value;
use std::{fs, process::Command};
use support::Case;

fn rusk(case: &Case, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn json(case: &Case, args: &[&str]) -> Value {
    serde_json::from_str(&rusk(case, args)).unwrap()
}

#[test]
fn catalog_keeps_overloads_members_comments_and_include_identity() {
    let case = Case::new(
        r#"//! Root file.
include "lib.rusk";
/// Does not transfer to the include.
include "./lib.rusk";
extern symbol Native {code_file:"unavailable.hpp",}
fn len(x:Int)->Int{return x;}
"#,
    );
    fs::write(
        case.0.join("lib.rusk"),
        r#"//! Library file.
/// A generic record.
struct Box<T>{
/// A stored member.
value:T;
}
enum Choice{Empty,
/// Holds a count.
Count(Int)}
/// Distinct description.
extern fn len<T>(readonly noescape data:Box<T>)->Int{code:"not compiled",}
"#,
    )
    .unwrap();
    let data = json(&case, &["doc", "--file", "main.rusk", "--json"]);
    assert_eq!(data["version"], 1);
    let files = data["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["description"], "Library file.");
    assert_eq!(files[1]["description"], "Root file.");
    let entries = data["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 8);
    assert!(entries.iter().all(|entry| entry["origin"] == "user"));
    let lookup = |name: &str| entries.iter().find(|entry| entry["name"] == name).unwrap();
    assert_eq!(lookup("value")["owner"], "Box");
    assert_eq!(lookup("value")["description"], "A stored member.");
    assert_eq!(lookup("Count")["owner"], "Choice");
    assert_eq!(lookup("Count")["signature"], "Count(Int)");
    assert_eq!(lookup("Count")["description"], "Holds a count.");
    assert_eq!(lookup("Native")["kind"], "extern symbol");
    assert!(lookup("Native")["description"].is_null());
    let native_len = lookup("len");
    assert_eq!(native_len["kind"], "extern function");
    assert_eq!(native_len["description"], "Distinct description.");
    assert_eq!(native_len["type_parameters"][0], "T");
    assert_eq!(native_len["parameters"][0]["access"], "readonly");
    assert_eq!(native_len["parameters"][0]["escape"], "noescape");
    assert_eq!(native_len["return_type"], "Int");
    assert_eq!(native_len["line"], 11);

    let overloads = json(&case, &["search", "LEN", "--file", "main.rusk", "--json"]);
    assert_eq!(overloads["entries"].as_array().unwrap().len(), 2);
    assert_eq!(overloads["entries"][0], *native_len);
    assert_ne!(overloads["entries"][0]["id"], overloads["entries"][1]["id"]);
    let description = json(
        &case,
        &[
            "search",
            "distinct description",
            "--file",
            "main.rusk",
            "--json",
        ],
    );
    assert_eq!(description["entries"][0], *native_len);
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn formatting_keeps_catalog_identity_and_attachments() {
    let case = Case::new(
        r#"//! File summary.

/// A value.
struct A<T>{
/// A field.
x:T;}
/// A read.
fn get<T>(readonly x:A<T>)->T{return x.x;}
"#,
    );
    let args = ["doc", "--file", "main.rusk", "--json"];
    let mut before = json(&case, &args);
    rusk(&case, &["fmt", "--write", "main.rusk"]);
    let mut after = json(&case, &args);
    assert_ne!(before, after, "source positions must reflect formatting");
    for catalog in [&mut before, &mut after] {
        for entry in catalog["entries"].as_array_mut().unwrap() {
            let fields = entry.as_object_mut().unwrap();
            fields.remove("line");
            fields.remove("column");
        }
    }
    assert_eq!(before, after);
}

#[test]
fn tuple_externs_and_include_errors_are_handled_from_source() {
    let case =
        Case::new("/// A tuple representation.\nextern type (Int, Str) {cpp_name:\"Unused\",}");
    let catalog = json(&case, &["doc", "--file", "main.rusk", "--json"]);
    assert_eq!(catalog["entries"][0]["name"], "Tuple");
    assert_eq!(
        catalog["entries"][0]["description"],
        "A tuple representation."
    );

    let broken = Case::new("include \"missing.rusk\";");
    let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&broken.0)
        .args(["search", "copy", "--file", "main.rusk", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("main.rusk:1:") && error.contains("missing.rusk"),
        "{error}"
    );
}

#[test]
fn library_reference_has_unique_stable_anchors_and_located_signatures() {
    let case = Case::new("fn len(x:Int)->Int{return x;}");
    let source_url = "https://example.test/source";
    let markdown = rusk(&case, &["doc", "--source-url", source_url]);
    let catalog = json(&case, &["doc", "--json"]);
    let mut ids = std::collections::BTreeSet::new();
    for entry in catalog["entries"].as_array().unwrap() {
        let id = entry["id"].as_str().unwrap();
        let file = entry["file"].as_str().unwrap();
        let file = file
            .strip_prefix("<stdlib/")
            .unwrap()
            .strip_suffix('>')
            .unwrap();
        assert!(ids.insert(id));
        assert!(markdown.contains(&format!("{{ #{id} }}")));
        assert!(markdown.contains(&format!("{source_url}/{file}#L{}", entry["line"])));
        assert!(markdown.contains(entry["signature"].as_str().unwrap()));
        assert!(
            entry["description"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
            "{} has no API description",
            entry["signature"]
        );
    }
    assert!(ids.len() > 100);

    let query = json(&case, &["search", "__range__", "--all", "--json"]);
    assert_eq!(query["entries"][0]["internal"], true);
    let query = json(&case, &["search", "__range__", "--json"]);
    assert!(query["entries"].as_array().unwrap().is_empty());
    let query = json(
        &case,
        &["search", "len", "--file", "main.rusk", "--stdlib", "--json"],
    );
    let origins = query["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["origin"].as_str().unwrap());
    assert_eq!(
        origins.collect::<std::collections::BTreeSet<_>>(),
        ["stdlib", "user"].into()
    );
}

#[test]
fn documented_standard_api_examples_run() {
    let root = Case::new("");
    let catalog = json(&root, &["doc", "--json"]);
    let mut tested = 0;
    for entry in catalog["entries"].as_array().unwrap() {
        let Some(description) = entry["description"].as_str() else {
            continue;
        };
        for block in description.split("```rusk\n").skip(1) {
            let (source, rest) = block.split_once("\n```").unwrap();
            let expected = rest
                .strip_prefix("\n\n出力\n\n```text\n")
                .and_then(|text| text.split_once("\n```"))
                .map(|(text, _)| format!("{text}\n"))
                .unwrap_or_else(|| panic!("missing output block in {}", entry["signature"]));
            let output = Case::new(source).run("");
            assert!(
                output.status.success(),
                "{}: {}",
                entry["signature"],
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected.as_bytes(), "{}", entry["signature"]);
            tested += 1;
        }
    }
    assert!(tested >= 2);
}
