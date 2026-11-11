use crate::frontend::{
    ast::{AccessMode, BindingMode, DeclaredName, EscapeMode, Name, Span, Stmt},
    source::Documented,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Origin {
    Standard,
    User,
}
impl Origin {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Standard => "stdlib",
            Self::User => "user",
        }
    }
}

pub(super) struct File {
    pub origin: Origin,
    pub path: String,
    pub description: String,
}

pub(super) struct Entry {
    pub origin: Origin,
    pub name: String,
    pub kind: &'static str,
    pub signature: String,
    pub owner: Option<String>,
    pub description: Option<String>,
    pub position: Span,
    pub id: String,
    pub type_parameters: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub internal: bool,
}
impl Entry {
    pub fn qualified_name(&self) -> String {
        match &self.owner {
            Some(owner) if self.kind == "variant" => format!("{owner}::{}", self.name),
            Some(owner) => format!("{owner}.{}", self.name),
            None => self.name.clone(),
        }
    }
    pub fn json(&self) -> Value {
        json!({
            "id": self.id,
            "origin": self.origin.label(),
            "name": self.name,
            "kind": self.kind,
            "signature": self.signature,
            "owner": self.owner,
            "description": self.description,
            "file": self.position.file,
            "line": self.position.line,
            "column": self.position.column,
            "type_parameters": self.type_parameters,
            "parameters": self.parameters.iter().map(Parameter::json).collect::<Vec<_>>(),
            "return_type": self.return_type,
            "internal": self.internal,
        })
    }
}

pub(super) struct Parameter {
    name: String,
    ty: String,
    binding: Option<&'static str>,
    access: Option<&'static str>,
    escape: Option<&'static str>,
}
impl Parameter {
    fn new((name, ty): (DeclaredName, Name)) -> Self {
        Self {
            name: name.name.text,
            ty: ty.text,
            binding: name.binding.map(|a| match a.mode {
                BindingMode::Fixed => "fixed",
                BindingMode::Rebindable => "rebindable",
            }),
            access: name.access.map(|a| match a.mode {
                AccessMode::Readonly => "readonly",
                AccessMode::Mutable => "mutable",
            }),
            escape: name.escape.map(|a| match a.mode {
                EscapeMode::Noescape => "noescape",
                EscapeMode::Escape => "escape",
            }),
        }
    }
    fn signature(&self) -> String {
        let mut text = String::new();
        for modifier in [self.binding, self.access, self.escape]
            .into_iter()
            .flatten()
        {
            text.push_str(modifier);
            text.push(' ');
        }
        text.push_str(&self.name);
        text.push_str(": ");
        text.push_str(&type_text(&self.ty));
        text
    }
    fn json(&self) -> Value {
        json!({
            "name": self.name,
            "type": self.ty,
            "binding": self.binding,
            "access": self.access,
            "escape": self.escape,
        })
    }
}

#[derive(Default)]
pub(super) struct Catalog {
    pub files: Vec<File>,
    pub entries: Vec<Entry>,
}
impl Catalog {
    pub fn extend(&mut self, origin: Origin, mut source: Documented) {
        let mut identifiers = BTreeMap::<String, usize>::new();
        let mut add = |mut entry: Entry| {
            if let Some(file) = source.files.get_mut(&entry.position.file) {
                entry.description = file
                    .declarations
                    .remove(&(entry.position.line, entry.position.column));
            }
            let id = identifier(&entry);
            let count = identifiers.entry(id.clone()).or_default();
            *count += 1;
            entry.id = if *count == 1 {
                id
            } else {
                format!("{id}-{count}")
            };
            self.entries.push(entry);
        };
        for statement in source.statements {
            match statement {
                Stmt::Struct(name, generics, fields) => {
                    let ty = name.text.clone();
                    add(type_entry(origin, name, "struct", generics));
                    for (name, field) in fields {
                        let signature = format!("{}: {}", name.text, type_text(&field.text));
                        add(entry(origin, name, "field", signature, Some(ty.clone())));
                    }
                }
                Stmt::Enum(name, generics, variants) => {
                    let ty = name.text.clone();
                    add(type_entry(origin, name, "enum", generics));
                    for (name, payload) in variants {
                        let signature = if payload.is_empty() {
                            name.text.clone()
                        } else {
                            format!(
                                "{}({})",
                                name.text,
                                payload
                                    .iter()
                                    .map(|ty| type_text(&ty.text))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        };
                        add(entry(origin, name, "variant", signature, Some(ty.clone())));
                    }
                }
                Stmt::Type(name, _) => add(type_entry(origin, name, "extern type", vec![])),
                Stmt::GenericExternType(name, generics, _) => {
                    add(type_entry(origin, name, "extern type", generics));
                }
                Stmt::Symbol(name, _) => {
                    let signature = format!("extern symbol {}", name.text);
                    add(entry(origin, name, "extern symbol", signature, None));
                }
                Stmt::UserFunction(name, generics, params, result, _) => {
                    add(function(origin, name, "function", generics, params, result));
                }
                Stmt::Function(name, params, result, _) => {
                    add(function(
                        origin,
                        name,
                        "extern function",
                        vec![],
                        params,
                        result,
                    ));
                }
                Stmt::GenericExternFunction(name, generics, params, result, properties) => {
                    let mut entry =
                        function(origin, name, "extern function", generics, params, result);
                    entry.internal |=
                        origin == Origin::Standard && properties.contains_key("derive");
                    add(entry);
                }
                _ => {}
            }
        }
        self.files
            .extend(source.files.into_iter().map(|(path, docs)| File {
                origin,
                path,
                description: docs.file,
            }));
        self.entries.sort_by(|a, b| {
            (
                &a.origin,
                &a.position.file,
                a.position.line,
                a.position.column,
            )
                .cmp(&(
                    &b.origin,
                    &b.position.file,
                    b.position.line,
                    b.position.column,
                ))
        });
    }
}

fn entry(
    origin: Origin,
    name: Name,
    kind: &'static str,
    signature: String,
    owner: Option<String>,
) -> Entry {
    let internal =
        origin == Origin::Standard && (kind == "extern symbol" || name.text.starts_with("__"));
    Entry {
        origin,
        name: name.text.split('<').next().unwrap_or_default().into(),
        position: name.span,
        kind,
        signature,
        owner,
        description: None,
        id: String::new(),
        type_parameters: vec![],
        parameters: vec![],
        return_type: None,
        internal,
    }
}

fn type_entry(origin: Origin, name: Name, kind: &'static str, generics: Vec<Name>) -> Entry {
    let type_parameters = generics.into_iter().map(|p| p.text).collect::<Vec<_>>();
    let signature = if kind == "extern type" && !type_parameters.is_empty() {
        format!(
            "{kind}<{}> {}",
            type_parameters.join(", "),
            type_text(&name.text)
        )
    } else {
        format!(
            "{kind} {}{}",
            type_text(&name.text),
            generic_suffix(&type_parameters)
        )
    };
    let mut entry = entry(origin, name, kind, signature, None);
    entry.type_parameters = type_parameters;
    entry
}

fn function(
    origin: Origin,
    name: Name,
    kind: &'static str,
    generics: Vec<Name>,
    arguments: Vec<(DeclaredName, Name)>,
    result: Option<Name>,
) -> Entry {
    let type_parameters = generics.into_iter().map(|p| p.text).collect::<Vec<_>>();
    let parameters = arguments
        .into_iter()
        .map(Parameter::new)
        .collect::<Vec<_>>();
    let result = result.map_or_else(|| "Unit".into(), |result| result.text);
    let keyword = if kind == "function" {
        "fn"
    } else {
        "extern fn"
    };
    let signature = format!(
        "{keyword} {}{}({}) -> {}",
        name.text,
        generic_suffix(&type_parameters),
        parameters
            .iter()
            .map(Parameter::signature)
            .collect::<Vec<_>>()
            .join(", "),
        type_text(&result),
    );
    let mut entry = entry(origin, name, kind, signature, None);
    entry.type_parameters = type_parameters;
    entry.parameters = parameters;
    entry.return_type = Some(result);
    entry
}

fn type_text(ty: &str) -> String {
    ty.replace(',', ", ")
}

fn generic_suffix(parameters: &[String]) -> String {
    if parameters.is_empty() {
        String::new()
    } else {
        format!("<{}>", parameters.join(", "))
    }
}

fn identifier(entry: &Entry) -> String {
    // FNV-1a over source identity and the declaration, independent of spans.
    let mut hash = 0xcbf29ce484222325u64;
    for part in [
        entry.origin.label(),
        entry.position.file.as_str(),
        entry.kind,
        entry.owner.as_deref().unwrap_or_default(),
        entry.signature.as_str(),
    ] {
        for byte in part.bytes().chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    format!("api-{}-{hash:016x}", entry.origin.label())
}
