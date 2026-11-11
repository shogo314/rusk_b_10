//! Lossless tokens shared by formatting and source documentation tools.
use super::rusk_parser::{TERMINAL_NAMES, rusk_grammar_scanner::RuskGrammarScanner};
use std::{collections::BTreeSet, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Ordinary,
    Declaration,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Terminal(&'static str),
    Whitespace,
    Comment(CommentKind),
}

#[derive(Clone, Debug)]
pub(crate) struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub range: Range<usize>,
}

impl Token<'_> {
    pub fn is_comment(&self) -> bool {
        matches!(self.kind, Kind::Comment(_))
    }
}

#[derive(Default)]
pub(crate) struct LayoutHints {
    // First type token, including tuple-type prefixes skipped by the CST.
    pub type_starts: BTreeSet<usize>,
    pub unary: BTreeSet<usize>,
    pub joint_operators: BTreeSet<usize>,
    pub negative_patterns: BTreeSet<usize>,
    pub blocks: BTreeSet<usize>,
    pub indexes: BTreeSet<usize>,
    pub declarations: Vec<Declaration>,
}

pub(crate) struct Declaration {
    pub kind: &'static str,
    pub name: String,
    pub name_offset: usize,
    pub line: u32,
    pub column: u32,
}

// Parol counts the recursive rules used by flat lists as well. Check actual
// delimiter nesting separately so long lists can reach their own limits.
pub(super) fn check_delimiter_depth(text: &str, file: &str) -> Result<(), String> {
    const MAX_DEPTH: usize = 128;
    if text.len() <= MAX_DEPTH {
        return Ok(());
    }
    let scanner = RuskGrammarScanner::new();
    let mut depth = 0usize;
    for found in scanner.find_matches(text, 0) {
        match &text[found.span.clone()] {
            "(" | "[" | "{" => {
                depth += 1;
                if depth > MAX_DEPTH {
                    let before = &text[..found.span.start];
                    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
                    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                    return Err(format!(
                        "{file}:{line}:{column}: delimiter nesting limit ({MAX_DEPTH}) exceeded"
                    ));
                }
            }
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = vec![];
    scan(text, 0, &mut tokens);
    tokens
}

fn scan<'a>(text: &'a str, base: usize, output: &mut Vec<Token<'a>>) {
    let scanner = RuskGrammarScanner::new();
    let mut end = 0;
    for found in scanner.find_matches(text, 0) {
        assert_eq!(
            found.span.start, end,
            "scanner must retain every source byte"
        );
        end = found.span.end;
        let name = TERMINAL_NAMES[found.token_type];
        let raw = &text[found.span.clone()];
        if name == "LoopLabel" {
            // The parser token can contain whitespace and comments before ':'.
            // Re-scan without the following loop keyword to expose that trivia.
            scan(raw, base + found.span.start, output);
            continue;
        }
        let mut finish = found.span.end;
        let kind = match name {
            "Newline" | "Whitespace" => Kind::Whitespace,
            "LineComment" => {
                finish -= raw.len() - raw.trim_end_matches(['\r', '\n']).len();
                let kind = if raw.starts_with("///") && !raw.starts_with("////") {
                    CommentKind::Declaration
                } else if raw.starts_with("//!") {
                    CommentKind::File
                } else {
                    CommentKind::Ordinary
                };
                Kind::Comment(kind)
            }
            _ => Kind::Terminal(name),
        };
        output.push(Token {
            kind,
            text: &text[found.span.start..finish],
            range: base + found.span.start..base + finish,
        });
        if finish < found.span.end {
            output.push(Token {
                kind: Kind::Whitespace,
                text: &text[finish..found.span.end],
                range: base + finish..base + found.span.end,
            });
        }
    }
    assert_eq!(end, text.len(), "scanner must retain the source suffix");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_reconstruct_literals_comments_labels_and_line_endings_exactly() {
        let source = "//! file\r\n/// 説明\r\nfn f(){outer // label\n :loop{wt \"// \\\"\", '\\n';break outer;}}\r\n//// ordinary\n";
        let tokens = tokenize(source);
        assert_eq!(tokens.iter().map(|t| t.text).collect::<String>(), source);
        let mut offset = 0;
        for token in &tokens {
            assert_eq!(token.range.start, offset);
            assert_eq!(&source[token.range.clone()], token.text);
            offset = token.range.end;
        }
        assert_eq!(offset, source.len());
        let comments = tokens
            .iter()
            .filter(|t| t.is_comment())
            .map(|t| t.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            comments,
            [
                Kind::Comment(CommentKind::File),
                Kind::Comment(CommentKind::Declaration),
                Kind::Comment(CommentKind::Ordinary),
                Kind::Comment(CommentKind::Ordinary)
            ]
        );
    }

    #[test]
    fn arbitrary_text_is_not_lost_even_before_syntax_validation() {
        for source in [
            "",
            "\0あ@",
            "\"unfinished",
            "//no newline",
            "\r\n\t ",
            "let \"a\nb\";",
        ] {
            assert_eq!(
                tokenize(source).iter().map(|t| t.text).collect::<String>(),
                source
            );
        }
    }
}
