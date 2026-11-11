//! Syntax-aware formatting backed by lossless tokens from the grammar scanner.
mod files;
mod pretty;
use crate::frontend::{
    self,
    text::{Kind, LayoutHints, Token},
};
pub(crate) use files::run;
use pretty::Doc;
use std::collections::{BTreeMap, BTreeSet};

const WIDTH: usize = 100;

pub(crate) fn source(text: &str, file: &str) -> Result<String, String> {
    let hints = frontend::parse_layout(text, file)?;
    let _stage = crate::diagnostics::enter(crate::diagnostics::Stage::Formatting);
    let tokens = frontend::text::tokenize(text);
    let significant = tokens
        .iter()
        .filter(|t| t.kind != Kind::Whitespace)
        .collect::<Vec<_>>();
    let layout = Layout::new(text, significant, hints);
    let result = layout
        .sequence(0, layout.tokens.len(), Context::Top)
        .render(WIDTH);
    // Whitespace must never combine/split lexical tokens, move a comment into a
    // literal, or otherwise change the token stream. Catch formatter bugs here.
    let actual = frontend::text::tokenize(&result);
    let expected = tokens
        .iter()
        .filter(|t| t.kind != Kind::Whitespace)
        .map(|t| (t.kind, t.text))
        .collect::<Vec<_>>();
    let actual = actual
        .iter()
        .filter(|t| t.kind != Kind::Whitespace)
        .map(|t| (t.kind, t.text))
        .collect::<Vec<_>>();
    assert_eq!(
        actual, expected,
        "formatting must preserve every non-whitespace token"
    );
    let formatted_hints = frontend::parse_layout(&result, file)
        .unwrap_or_else(|error| panic!("formatter produced invalid syntax: {error}"));
    assert_eq!(
        frontend::documentation::collect(text, &tokens, &layout.hints),
        frontend::documentation::collect(
            &result,
            &frontend::text::tokenize(&result),
            &formatted_hints
        ),
        "formatting must preserve documentation attachments"
    );
    Ok(result)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Top,
    Block,
    Fields,
    List,
    Array,
}

struct Layout<'a> {
    source: &'a str,
    tokens: Vec<&'a Token<'a>>,
    hints: LayoutHints,
    angles: BTreeSet<usize>,
    pairs: BTreeMap<usize, usize>,
}

impl<'a> Layout<'a> {
    fn new(source: &'a str, tokens: Vec<&'a Token<'a>>, mut hints: LayoutHints) -> Self {
        for offset in &hints.negative_patterns {
            let index = tokens
                .iter()
                .position(|t| t.range.start == *offset)
                .expect("negative pattern literal must be a token");
            let minus = (0..index)
                .rev()
                .find(|&i| !tokens[i].is_comment())
                .expect("negative pattern must have a minus");
            assert_eq!(tokens[minus].text, "-");
            hints.unary.insert(tokens[minus].range.start);
        }
        let mut angles = BTreeSet::new();
        for start in &hints.type_starts {
            let index = tokens
                .iter()
                .position(|t| t.range.start == *start)
                .expect("type start must be a token");
            let open = (0..index)
                .rev()
                .find(|&i| !tokens[i].is_comment() && tokens[i].text == "<")
                .expect("type arguments must have an opening angle");
            angles.insert(open);
        }
        let mut pairs = BTreeMap::new();
        let mut stack = vec![];
        for (i, token) in tokens.iter().enumerate() {
            if token.is_comment() {
                continue;
            }
            match token.text {
                "(" | "[" | "{" => stack.push((i, token.text)),
                "<" if angles.contains(&i) => stack.push((i, "<")),
                ")" | "]" | "}" => {
                    let (open, kind) = stack.pop().expect("parsed delimiters must balance");
                    assert!(matches!(
                        (kind, token.text),
                        ("(", ")") | ("[", "]") | ("{", "}")
                    ));
                    pairs.insert(open, i);
                }
                ">" if stack.last().is_some_and(|(_, kind)| *kind == "<") => {
                    let (open, _) = stack.pop().unwrap();
                    pairs.insert(open, i);
                }
                _ => {}
            }
        }
        assert!(stack.is_empty(), "parsed delimiters must close");
        Self {
            source,
            tokens,
            hints,
            angles,
            pairs,
        }
    }

    fn sequence(&self, start: usize, end: usize, context: Context) -> Doc<'a> {
        let mut docs = vec![];
        let mut previous = None;
        let mut index = start;
        while index < end {
            if let Some(last) = previous
                && let Some(separator) = self.separator(last, index, context)
            {
                docs.push(separator);
            }
            let token = self.tokens[index];
            if let Some(&close) = self.pairs.get(&index) {
                let inner_context = match token.text {
                    "{" if self.hints.blocks.contains(&token.range.start) => Context::Block,
                    "{" => Context::Fields,
                    "[" => Context::Array,
                    _ => Context::List,
                };
                let inner = self.sequence(index + 1, close, inner_context);
                let doc = if close == index + 1 {
                    Doc::sequence(vec![
                        Doc::Text(token.text),
                        Doc::Text(self.tokens[close].text),
                    ])
                } else if token.text == "{" {
                    let prefix = if self.inline_opening_comment(index, close) {
                        Doc::Text(" ")
                    } else {
                        Doc::Line
                    };
                    Doc::sequence(vec![
                        Doc::Text("{"),
                        Doc::sequence(vec![prefix, inner]).nest(),
                        Doc::Line,
                        Doc::Text("}"),
                    ])
                } else {
                    let prefix = if self.inline_opening_comment(index, close) {
                        Doc::Text(" ")
                    } else {
                        Doc::Soft
                    };
                    Doc::sequence(vec![
                        Doc::Text(token.text),
                        Doc::sequence(vec![prefix, inner]).nest(),
                        Doc::Soft,
                        Doc::Text(self.tokens[close].text),
                    ])
                    .group()
                };
                docs.push(doc);
                previous = Some(close);
                index = close + 1;
            } else {
                docs.push(Doc::Text(token.text));
                previous = Some(index);
                index += 1;
            }
        }
        if previous.is_some_and(|i| self.tokens[i].is_comment()) {
            docs.push(Doc::Line);
        }
        Doc::sequence(docs)
    }

    fn inline_opening_comment(&self, open: usize, close: usize) -> bool {
        open + 1 < close
            && self.tokens[open + 1].is_comment()
            && !self.source[self.tokens[open].range.end..self.tokens[open + 1].range.start]
                .contains(['\r', '\n'])
    }

    fn separator(&self, previous: usize, next: usize, context: Context) -> Option<Doc<'a>> {
        let a = self.tokens[previous];
        let b = self.tokens[next];
        let gap = &self.source[a.range.end..b.range.start];
        let newlines = frontend::documentation::line_breaks(gap);
        if a.is_comment() {
            return Some(if newlines > 1 { Doc::Blank } else { Doc::Line });
        }
        if b.is_comment() {
            return Some(if newlines > 1 {
                Doc::Blank
            } else if newlines > 0 {
                Doc::Line
            } else {
                Doc::Text(" ")
            });
        }
        if a.text == ";" {
            return Some(if matches!(context, Context::Array | Context::List) {
                Doc::Text(" ")
            } else if newlines > 1 {
                Doc::Blank
            } else {
                Doc::Line
            });
        }
        if a.text == "," {
            return Some(match context {
                Context::Fields => {
                    if newlines > 1 {
                        Doc::Blank
                    } else {
                        Doc::Line
                    }
                }
                Context::List | Context::Array => Doc::Space,
                _ => Doc::Text(" "),
            });
        }
        if matches!(b.text, ";" | "," | ":" | "." | "::" | "?!")
            || self.angles.contains(&next)
            || self.hints.joint_operators.contains(&b.range.start)
        {
            return None;
        }
        if matches!(a.text, "." | "::") {
            return None;
        }
        if self.hints.unary.contains(&a.range.start) {
            return if self.hints.unary.contains(&b.range.start) {
                Some(Doc::Text(" "))
            } else {
                None
            };
        }
        if b.text == "[" && self.hints.indexes.contains(&b.range.start) {
            return None;
        }
        if b.text == "("
            && !matches!(
                a.text,
                "if" | "while" | "for" | "return" | "wt" | "in" | "let" | "=" | ":=" | "=>" | ":"
            )
        {
            // Operators still need a space before a parenthesized operand.
            let type_close = a.text == ">"
                && self
                    .angles
                    .iter()
                    .any(|open| self.pairs.get(open) == Some(&previous));
            if matches!(a.kind, Kind::Terminal("Identifier"))
                || matches!(a.text, ")" | "]" | "}" | "?!")
                || type_close
            {
                return None;
            }
        }
        if a.text == "}"
            && !matches!(
                b.text,
                "else"
                    | "("
                    | "["
                    | "{"
                    | "+"
                    | "-"
                    | "*"
                    | "**"
                    | "/"
                    | "%"
                    | "=="
                    | "!="
                    | "<"
                    | ">"
                    | "<="
                    | ">="
                    | "<=>"
                    | "<?"
                    | ">?"
                    | "&&"
                    | "||"
                    | "&"
                    | "|"
                    | "^"
                    | "??"
                    | ".."
                    | "="
                    | ":="
                    | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "<?="
                    | ">?="
                    | "=>"
            )
        {
            return Some(if newlines > 1 { Doc::Blank } else { Doc::Line });
        }
        Some(Doc::Text(" "))
    }
}

#[cfg(test)]
mod tests;
