//! Documentation comments are attached to syntactic declarations, not guesses
//! based on line contents. Parsing, formatting and future API tools share this.
use super::text::{CommentKind, Declaration, Kind, LayoutHints, Token};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Documentation {
    pub file: String,
    pub declarations: Vec<(&'static str, String, Option<String>)>,
}

pub(crate) fn collect(source: &str, tokens: &[Token<'_>], hints: &LayoutHints) -> Documentation {
    let (file, declarations) = collect_declarations(source, tokens, hints);
    Documentation {
        file,
        declarations: declarations
            .into_iter()
            .map(|(decl, comment)| (decl.kind, decl.name.clone(), comment))
            .collect(),
    }
}

pub(crate) struct SourceDocumentation {
    pub file: String,
    pub declarations: BTreeMap<(u32, u32), String>,
}

pub(crate) fn collect_at_positions(source: &str, hints: &LayoutHints) -> SourceDocumentation {
    let tokens = super::text::tokenize(source);
    let (file, declarations) = collect_declarations(source, &tokens, hints);
    SourceDocumentation {
        file,
        declarations: declarations
            .into_iter()
            .filter_map(|(decl, comment)| Some(((decl.line, decl.column), comment?)))
            .collect(),
    }
}

fn collect_declarations<'a>(
    source: &str,
    tokens: &[Token<'_>],
    hints: &'a LayoutHints,
) -> (String, Vec<(&'a Declaration, Option<String>)>) {
    let first_code = tokens
        .iter()
        .position(|t| matches!(t.kind, Kind::Terminal(_)))
        .unwrap_or(tokens.len());
    let mut file = vec![];
    let mut previous_file_end = None;
    for token in &tokens[..first_code] {
        if token.kind == Kind::Comment(CommentKind::File) && own_line(source, token) {
            if previous_file_end.is_some_and(|end| line_breaks(&source[end..token.range.start]) > 1)
            {
                file.push(String::new());
            }
            file.push(body(token.text, "//!"));
            previous_file_end = Some(token.range.end);
        }
    }
    let mut declarations = hints.declarations.iter().collect::<Vec<_>>();
    declarations.sort_by_key(|d| d.name_offset);
    let declarations = declarations
        .into_iter()
        .map(|decl| {
            let name = tokens
                .binary_search_by_key(&decl.name_offset, |token| token.range.start)
                .expect("declaration name must be a token");
            let keyword = match decl.kind {
                "function" => Some("fn"),
                "struct" => Some("struct"),
                "enum" => Some("enum"),
                "extern function" | "extern type" | "extern symbol" => Some("extern"),
                _ => None,
            };
            let start = keyword.map_or(name, |keyword| {
                (0..name)
                    .rev()
                    .find(|&i| {
                        matches!(tokens[i].kind, Kind::Terminal(_)) && tokens[i].text == keyword
                    })
                    .expect("declaration must have its keyword")
            });
            (decl, leading(source, tokens, start))
        })
        .collect();
    (file.join("\n"), declarations)
}

fn leading(source: &str, tokens: &[Token<'_>], start: usize) -> Option<String> {
    let mut index = start;
    let mut lines = vec![];
    while index > 0 {
        let mut breaks = 0;
        while index > 0 && tokens[index - 1].kind == Kind::Whitespace {
            index -= 1;
            breaks += line_breaks(tokens[index].text);
        }
        if index == 0 || breaks > 1 {
            break;
        }
        let comment = &tokens[index - 1];
        if comment.kind != Kind::Comment(CommentKind::Declaration) || !own_line(source, comment) {
            break;
        }
        lines.push(body(comment.text, "///"));
        index -= 1;
    }
    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

fn body(text: &str, prefix: &str) -> String {
    let text = text
        .strip_prefix(prefix)
        .expect("classified documentation prefix");
    text.strip_prefix(' ').unwrap_or(text).to_string()
}

fn own_line(source: &str, token: &Token<'_>) -> bool {
    let prefix = &source[..token.range.start];
    let start = prefix.rfind(['\r', '\n']).map_or(0, |i| i + 1);
    source[start..token.range.start].trim().is_empty()
}

pub(crate) fn line_breaks(text: &str) -> usize {
    text.bytes()
        .enumerate()
        .filter(|&(i, byte)| {
            byte == b'\n' || (byte == b'\r' && text.as_bytes().get(i + 1) != Some(&b'\n'))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn docs(source: &str) -> Documentation {
        collect(
            source,
            &super::super::text::tokenize(source),
            &super::super::parse_layout(source, "docs.rusk").unwrap(),
        )
    }

    #[test]
    fn attaches_markdown_to_declarations_fields_variants_and_externs() {
        let source = "//! file\n//!\n//! # Examples\n/// a type\nstruct S{\n/// field\nx:Int;}\n/// choice\nenum E{\n/// variant\nA(Int)}\n/// function\n///\n///     wt f(1);\nfn f(x:Int)->Int{return x;}\n/// native\nextern fn n<T>(x:T)->T{code:\"unused\",}\n/// native type\nextern type<T> Box{cpp_name:\"{T}\",}";
        let docs = docs(source);
        assert_eq!(docs.file, "file\n\n# Examples");
        assert_eq!(
            docs.declarations
                .iter()
                .map(|(kind, name, doc)| (*kind, name.as_str(), doc.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("struct", "S", Some("a type")),
                ("field", "x", Some("field")),
                ("enum", "E", Some("choice")),
                ("variant", "A", Some("variant")),
                ("function", "f", Some("function\n\n    wt f(1);")),
                ("extern function", "n", Some("native")),
                ("extern type", "Box", Some("native type")),
            ]
        );
    }

    #[test]
    fn blank_lines_ordinary_comments_and_inline_comments_do_not_attach() {
        let source = "/// separated\n\nfn a(){}\n/// interrupted\n// ordinary\nfn b(){}\nfn c(){} /// inline\nfn d(){}\n//! late file comment\nfn e(){}\n/// before include\ninclude \"missing.rusk\";fn f(){}";
        let docs = docs(source);
        assert!(docs.file.is_empty());
        assert!(
            docs.declarations.iter().all(|(_, _, doc)| doc.is_none()),
            "{docs:?}"
        );
    }
}
