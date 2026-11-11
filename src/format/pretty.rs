//! Small document renderer. Only whitespace is synthesized by the formatter.
#[derive(Clone)]
pub(super) enum Doc<'a> {
    Text(&'a str),
    Space,
    Soft,
    Line,
    Blank,
    Sequence(Vec<Self>),
    Nest(Box<Self>),
    Group(Box<Self>),
}

impl<'a> Doc<'a> {
    pub fn sequence(parts: Vec<Self>) -> Self {
        Self::Sequence(parts)
    }
    pub fn nest(self) -> Self {
        Self::Nest(Box::new(self))
    }
    pub fn group(self) -> Self {
        Self::Group(Box::new(self))
    }

    fn flat_width(&self) -> Option<usize> {
        match self {
            Self::Text(t) => (!t.contains(['\r', '\n'])).then(|| t.chars().count()),
            Self::Space => Some(1),
            Self::Soft => Some(0),
            Self::Line | Self::Blank => None,
            Self::Sequence(parts) => parts
                .iter()
                .try_fold(0_usize, |width, part| width.checked_add(part.flat_width()?)),
            Self::Nest(d) | Self::Group(d) => d.flat_width(),
        }
    }

    pub fn render(&self, width: usize) -> String {
        let mut output = String::new();
        let mut column = 0;
        let mut pending_indent = true;
        let mut stack = vec![(0, false, self)];
        while let Some((indent, flat, doc)) = stack.pop() {
            match doc {
                Self::Text(text) => {
                    if text.is_empty() {
                        continue;
                    }
                    if pending_indent {
                        output.extend(std::iter::repeat_n(' ', indent));
                        column = indent;
                    }
                    output.push_str(text);
                    column = text
                        .rsplit_once('\n')
                        .map_or(column + text.chars().count(), |(_, tail)| {
                            tail.chars().count()
                        });
                    pending_indent = text.ends_with('\n');
                }
                Self::Space if flat => {
                    if !pending_indent {
                        output.push(' ');
                        column += 1;
                    }
                }
                Self::Soft if flat => {}
                Self::Space | Self::Soft | Self::Line | Self::Blank => {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    if matches!(doc, Self::Blank) && !output.is_empty() && !output.ends_with("\n\n")
                    {
                        output.push('\n');
                    }
                    column = 0;
                    pending_indent = true;
                }
                Self::Sequence(parts) => {
                    stack.extend(parts.iter().rev().map(|part| (indent, flat, part)));
                }
                Self::Nest(d) => stack.push((indent + 4, flat, d)),
                Self::Group(d) => {
                    let start = if pending_indent { indent } else { column };
                    let fits = d
                        .flat_width()
                        .is_some_and(|n| n <= width.saturating_sub(start));
                    stack.push((indent, flat || fits, d));
                }
            }
        }
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output
    }
}
