//! Lower what a project's own files share into the project roots of
//! [`crate::ast::project`]: codelist sources, and a definition's parameters
//! and tests. The study and sections lower in [`super::sections`], and each
//! file type's header and own fields in [`crate::front_end::v1::project`].
//!
//! A spec lowers totally and its analyses refuse what they cannot read. These
//! files are read as before instead: every written scalar of a definition or
//! a codelist converts to the value the engine runs with, and lowering
//! reports each one that does not, and each node whose shape the schema
//! should have refused, with the file's path to it. Those findings decide
//! preparation; nothing after lowering reads the file.

use super::Positions;
use crate::ast::{
    Codelist, CodelistDeclaration, CodelistItem, CodelistSource, CodelistType, Codelists,
    ExternalDictionary, FunctionTest, Meta, Named, Parameter, Paths, Segment, Span,
    TerminologyStandard, Text,
};
use crate::project_function_document::{Finding, Kind, Reader};
use crate::schema::{Document, DocumentNode as N};
use crate::value::{ColumnType, Value, ValueType};

/// The metas of one document's nodes, in the arena of the root they lower
/// into.
pub(in crate::front_end) struct Metas<'a> {
    positions: Option<&'a Positions>,
    paths: &'a mut Paths,
}

impl<'a> Metas<'a> {
    pub(in crate::front_end) fn new(
        positions: Option<&'a Positions>,
        paths: &'a mut Paths,
    ) -> Self {
        Self { positions, paths }
    }
    fn span(&self, node: Option<usize>, inherited: Span) -> Span {
        node.and_then(|node| self.positions?.nodes.get(node).copied().flatten())
            .unwrap_or(inherited)
    }
    /// The arena's root, the document's root node.
    pub(in crate::front_end) fn root(&self, node: usize) -> Meta {
        Meta {
            path: self.paths.root(),
            span: self.span(Some(node), Span::UNKNOWN),
        }
    }
    /// The node `segment` names inside `parent`; an absent node takes the
    /// parent's position.
    pub(super) fn child(&mut self, parent: Meta, segment: Segment, node: Option<usize>) -> Meta {
        Meta {
            path: self.paths.child(parent.path, segment),
            span: self.span(node, parent.span),
        }
    }
    pub(in crate::front_end) fn field(
        &mut self,
        parent: Meta,
        name: &'static str,
        node: Option<usize>,
    ) -> Meta {
        self.child(parent, Segment::Field(name), node)
    }
    pub(in crate::front_end) fn index(&mut self, parent: Meta, index: usize, node: usize) -> Meta {
        self.child(parent, Segment::Index(index as u32), Some(node))
    }
    fn key(&mut self, parent: Meta, key: &str, node: Option<usize>) -> Meta {
        self.child(parent, Segment::Key(key.into()), node)
    }
}

/// Prefix each finding of a nested source with the path that declares it.
fn prefix(findings: Vec<Finding>, path: &str) -> Vec<Finding> {
    findings
        .into_iter()
        .map(|mut finding| {
            finding.path = if finding.path.is_empty() {
                path.into()
            } else {
                format!("{path}.{}", finding.path)
            };
            finding
        })
        .collect()
}

pub(in crate::front_end) fn text_of(document: &Document, node: usize) -> Option<&str> {
    match document.nodes().get(node) {
        Some(N::Text(text)) => Some(text),
        _ => None,
    }
}

/// The codelist sources a project file declares in `codelists`, each written
/// inline or as the path of a codelist file (CT-1, P-TYPE-1). An inline
/// source is a root of its own; each of its findings, prefixed with its
/// declaration, goes to `leaves`.
pub(in crate::front_end) fn codelist_declarations(
    document: &Document,
    positions: Option<&Positions>,
    (reader, metas): (&mut Reader<'_>, &mut Metas<'_>),
    (root, meta): (usize, Meta),
    leaves: &mut Vec<Finding>,
) -> Vec<CodelistDeclaration> {
    let mut codelists = Vec::new();
    let Some(list) = document.field(root, "codelists") else {
        return codelists;
    };
    let declared = metas.field(meta, "codelists", Some(list));
    for (index, node) in reader
        .sequence(root, "codelists", "codelists")
        .into_iter()
        .enumerate()
    {
        let path = format!("codelists[{index}]");
        let meta = metas.index(declared, index, node);
        let source = match document.nodes().get(node) {
            Some(N::Text(written)) => Some(CodelistSource::File(Text {
                text: written.clone(),
                meta,
            })),
            Some(N::Mapping(_)) => match codelists_at(document, positions, node) {
                Ok(source) => Some(CodelistSource::Inline(Box::new(source))),
                Err(findings) => {
                    leaves.extend(prefix(findings, &path));
                    None
                }
            },
            _ => {
                reader.fault(&path, node, Kind::NormalizedShape);
                None
            }
        };
        if let Some(source) = source {
            codelists.push(CodelistDeclaration { source, meta });
        }
    }
    codelists
}

fn parameter_kind(reader: &mut Reader<'_>, parent: usize, path: &str) -> ValueType {
    let text = reader.text_field(parent, "type", path);
    match text.as_str() {
        "str" => ValueType::Str,
        "int" => ValueType::Int,
        "float" => ValueType::Float,
        "bool" => ValueType::Bool,
        "date" => ValueType::Date,
        "datetime" => ValueType::DateTime,
        _ => {
            reader.fault(path, parent, Kind::NormalizedShape);
            ValueType::Str
        }
    }
}

pub(in crate::front_end) fn column_kind(
    reader: &mut Reader<'_>,
    parent: usize,
    path: &str,
) -> ColumnType {
    let text = reader.text_field(parent, "returns", path);
    match text.as_str() {
        "str" => ColumnType::Str,
        "int" => ColumnType::Int,
        "float" => ColumnType::Float,
        "date" => ColumnType::Date,
        "datetime" => ColumnType::DateTime,
        _ => {
            reader.fault(path, parent, Kind::NormalizedShape);
            ColumnType::Str
        }
    }
}

/// A text field's value and meta, read through `reader` as the field `name`.
pub(in crate::front_end) fn text_field(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (parent, meta): (usize, Meta),
    name: &'static str,
    path: &str,
) -> Text {
    let text = reader.text_field(parent, name, path);
    Text {
        text,
        meta: metas.field(meta, name, document.field(parent, name)),
    }
}

/// An optional text field, read through `reader` as the field `name`.
fn optional_text(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (parent, meta): (usize, Meta),
    name: &'static str,
    path: &str,
) -> Option<Text> {
    let text = reader.optional_text(parent, name, path)?;
    Some(Text {
        text,
        meta: metas.field(meta, name, document.field(parent, name)),
    })
}

/// `comparison_decimals`, which the schema defaults.
pub(in crate::front_end) fn comparison_decimals(reader: &mut Reader<'_>, root: usize) -> i64 {
    reader
        .field(root, "comparison_decimals", "comparison_decimals")
        .map(|node| match reader.scalar(node, "comparison_decimals") {
            Value::Int(value) => value,
            _ => {
                if !reader.has_finding_at("comparison_decimals") {
                    reader.fault("comparison_decimals", node, Kind::NormalizedShape);
                }
                0
            }
        })
        .unwrap_or_default()
}

/// The value type a column type's values have.
fn value_type(kind: ColumnType) -> ValueType {
    match kind {
        ColumnType::Str => ValueType::Str,
        ColumnType::Int => ValueType::Int,
        ColumnType::Float => ValueType::Float,
        ColumnType::Date => ValueType::Date,
        ColumnType::DateTime => ValueType::DateTime,
    }
}

pub(in crate::front_end) fn parameters(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
) -> Vec<Parameter> {
    let list = metas.field(meta, "params", document.field(root, "params"));
    reader
        .sequence(root, "params", "params")
        .iter()
        .enumerate()
        .map(|(i, &node)| {
            let path = format!("params[{i}]");
            let meta = metas.index(list, i, node);
            let at = (node, meta);
            let name = text_field(reader, metas, document, at, "name", &format!("{path}.name"));
            let kind = parameter_kind(reader, node, &format!("{path}.type"));
            let required = reader.boolean(node, "required", &format!("{path}.required"));
            let default = document
                .field(node, "default")
                .map(|written| reader.declared(written, &format!("{path}.default"), Some(kind)));
            let accepts_missing =
                reader.boolean(node, "accepts_missing", &format!("{path}.accepts_missing"));
            Parameter {
                name: Named {
                    name: name.text,
                    meta: name.meta,
                },
                kind,
                required,
                default,
                accepts_missing,
                meta,
            }
        })
        .collect()
}

/// The definition's `tests`. A case's arguments are typed by the parameters
/// they name and its result by `returns`, so a date is written as text
/// (P-FN-1).
pub(in crate::front_end) fn cases(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
    (params, returns): (&[Parameter], ColumnType),
) -> Vec<FunctionTest> {
    let list = metas.field(meta, "tests", document.field(root, "tests"));
    reader
        .sequence(root, "tests", "tests")
        .iter()
        .enumerate()
        .map(|(i, &node)| {
            let path = format!("tests[{i}]");
            let meta = metas.index(list, i, node);
            let at = (node, meta);
            let id = text_field(reader, metas, document, at, "id", &format!("{path}.id"));
            let tags = metas.field(meta, "covers", document.field(node, "covers"));
            let covers = reader
                .sequence(node, "covers", &format!("{path}.covers"))
                .iter()
                .enumerate()
                .map(|(i, &tag)| Text {
                    text: reader.text(tag, &format!("{path}.covers[{i}]")),
                    meta: metas.index(tags, i, tag),
                })
                .collect();
            let mut args = Vec::new();
            if let Some(args_node) = reader.field(node, "args", &format!("{path}.args")) {
                let declared = metas.field(meta, "args", Some(args_node));
                if let Some(N::Mapping(items)) = document.nodes().get(args_node) {
                    for &(key, written) in items {
                        let name = reader.text(key, &format!("{path}.args"));
                        let kind = params
                            .iter()
                            .find(|parameter| parameter.name.name == name)
                            .map(|parameter| parameter.kind);
                        let argument =
                            reader.declared(written, &format!("{path}.args.{name}"), kind);
                        let meta = metas.key(declared, &name, Some(written));
                        args.push((Named { name, meta }, argument));
                    }
                } else {
                    reader.fault(&format!("{path}.args"), args_node, Kind::NormalizedShape);
                }
            }
            let result = reader
                .field(node, "result", &format!("{path}.result"))
                .map(|node| {
                    reader.declared(node, &format!("{path}.result"), Some(value_type(returns)))
                })
                .unwrap_or(Value::Missing);
            FunctionTest {
                id,
                covers,
                args,
                result,
                meta,
            }
        })
        .collect()
}

/// Decode a normalized codelist source written inline at `root`, keeping the
/// authored source and list order.
fn codelists_at(
    document: &Document,
    positions: Option<&Positions>,
    root: usize,
) -> Result<Codelists, Vec<Finding>> {
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    let (standard, codelists) = codelist_source(&mut reader, &mut metas, document, (root, meta));
    let findings = reader.finish();
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Codelists {
        header: None,
        standard,
        codelists,
        paths,
        meta,
    })
}

/// The `standard` and `codelists` of the codelist source whose root is
/// `root`, read through `reader` into `metas`' arena.
pub(in crate::front_end) fn codelist_source(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
) -> (Option<TerminologyStandard>, Vec<Codelist>) {
    let standard = document.field(root, "standard").map(|node| {
        let meta = metas.field(meta, "standard", Some(node));
        let at = (node, meta);
        TerminologyStandard {
            name: text_field(reader, metas, document, at, "name", "standard.name"),
            publishing_set: text_field(
                reader,
                metas,
                document,
                at,
                "publishing_set",
                "standard.publishing_set",
            ),
            version: text_field(reader, metas, document, at, "version", "standard.version"),
            meta,
        }
    });
    let list = metas.field(meta, "codelists", document.field(root, "codelists"));
    let mut codelists = Vec::new();
    for (index, node) in reader
        .sequence(root, "codelists", "codelists")
        .into_iter()
        .enumerate()
    {
        let path = format!("codelists[{index}]");
        let meta = metas.index(list, index, node);
        let at = (node, meta);
        let id = text_field(reader, metas, document, at, "id", &format!("{path}.id"));
        let name = text_field(reader, metas, document, at, "name", &format!("{path}.name"));
        let data_type = match reader
            .text_field(node, "data_type", &format!("{path}.data_type"))
            .as_str()
        {
            "text" => CodelistType::Text,
            "integer" => CodelistType::Integer,
            "float" => CodelistType::Float,
            _ => {
                reader.fault(&format!("{path}.data_type"), node, Kind::NormalizedShape);
                CodelistType::Text
            }
        };
        let extensible = reader.boolean(node, "extensible", &format!("{path}.extensible"));
        let alias = optional_text(
            reader,
            metas,
            document,
            at,
            "alias",
            &format!("{path}.alias"),
        );
        let format_name = optional_text(
            reader,
            metas,
            document,
            at,
            "format_name",
            &format!("{path}.format_name"),
        );
        let items = document.field(node, "items").map(|items| {
            let declared = metas.field(meta, "items", Some(items));
            reader
                .sequence(node, "items", &format!("{path}.items"))
                .into_iter()
                .enumerate()
                .map(|(index, node)| {
                    let path = format!("{path}.items[{index}]");
                    let meta = metas.index(declared, index, node);
                    let at = (node, meta);
                    let value = reader
                        .field(node, "value", &format!("{path}.value"))
                        .map(|node| reader.scalar(node, &format!("{path}.value")))
                        .unwrap_or(Value::Missing);
                    let decode = optional_text(
                        reader,
                        metas,
                        document,
                        at,
                        "decode",
                        &format!("{path}.decode"),
                    );
                    let rank = document
                        .field(node, "rank")
                        .and_then(|node| reader.integer(node, &format!("{path}.rank")));
                    let alias = optional_text(
                        reader,
                        metas,
                        document,
                        at,
                        "alias",
                        &format!("{path}.alias"),
                    );
                    let extended = reader.boolean(node, "extended", &format!("{path}.extended"));
                    CodelistItem {
                        value,
                        decode,
                        rank,
                        alias,
                        extended,
                        meta,
                    }
                })
                .collect()
        });
        let external = document.field(node, "external").map(|external| {
            let meta = metas.field(meta, "external", Some(external));
            let at = (external, meta);
            ExternalDictionary {
                dictionary: text_field(
                    reader,
                    metas,
                    document,
                    at,
                    "dictionary",
                    &format!("{path}.external.dictionary"),
                ),
                version: text_field(
                    reader,
                    metas,
                    document,
                    at,
                    "version",
                    &format!("{path}.external.version"),
                ),
                href: optional_text(
                    reader,
                    metas,
                    document,
                    at,
                    "href",
                    &format!("{path}.external.href"),
                ),
                meta,
            }
        });
        codelists.push(Codelist {
            id: Named {
                name: id.text,
                meta: id.meta,
            },
            name,
            data_type,
            extensible,
            alias,
            format_name,
            items,
            external,
            meta,
        });
    }
    (standard, codelists)
}
