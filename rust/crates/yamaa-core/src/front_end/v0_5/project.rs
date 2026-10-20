//! Lower today's project, environment, function and codelist files into the
//! project roots of [`crate::ast::project`].
//!
//! A spec lowers totally and its analyses refuse what they cannot read. These
//! files are read as before instead: every written scalar of a definition or
//! a codelist converts to the value the engine runs with, and lowering
//! reports each one that does not, and each node whose shape the schema
//! should have refused, with the file's path to it. Those findings decide
//! preparation; nothing after lowering reads the file.
//!
//! A 0.5 project links an environment file for each host language. The one
//! its host reads lowers into the project root ([`environment`]): its lock
//! into that language's environment and its definitions into
//! [`Project::functions`]. Each of its paths keeps the environment file's
//! own spelling, `lock` and `functions.<name>`.

use super::sections;
use super::Positions;
use crate::ast::{
    Codelist, CodelistDeclaration, CodelistItem, CodelistSource, CodelistType, Codelists,
    ExternalDictionary, Function, FunctionDeclaration, FunctionSource, FunctionTest, Header,
    HostEnvironment, Implementation, Meta, Named, Parameter, Paths, Project, Segment, Span,
    Surface, TerminologyStandard, Text,
};
use crate::project_function::Language;
use crate::project_function_document::{Finding, Kind, Reader};
use crate::schema::{Document, DocumentNode as N};
use crate::value::{ColumnType, Value, ValueType};

/// The metas of one document's nodes, in the arena of the root they lower
/// into.
pub(super) struct Metas<'a> {
    positions: Option<&'a Positions>,
    paths: &'a mut Paths,
}

impl<'a> Metas<'a> {
    pub(super) fn new(positions: Option<&'a Positions>, paths: &'a mut Paths) -> Self {
        Self { positions, paths }
    }
    fn span(&self, node: Option<usize>, inherited: Span) -> Span {
        node.and_then(|node| self.positions?.nodes.get(node).copied().flatten())
            .unwrap_or(inherited)
    }
    /// The arena's root, the document's root node.
    pub(super) fn root(&self, node: usize) -> Meta {
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
    pub(super) fn field(&mut self, parent: Meta, name: &'static str, node: Option<usize>) -> Meta {
        self.child(parent, Segment::Field(name), node)
    }
    pub(super) fn index(&mut self, parent: Meta, index: usize, node: usize) -> Meta {
        self.child(parent, Segment::Index(index as u32), Some(node))
    }
    /// A list entry with an identity, such as a parameter by its name.
    pub(super) fn member(&mut self, parent: Meta, index: usize, key: &str, node: usize) -> Meta {
        self.child(
            parent,
            Segment::Member {
                index: index as u32,
                key: key.into(),
            },
            Some(node),
        )
    }
    pub(super) fn key(&mut self, parent: Meta, key: &str, node: Option<usize>) -> Meta {
        self.child(parent, Segment::Key(key.into()), node)
    }
    /// The same path written at `node`, such as an entry's key beside its
    /// value.
    pub(super) fn at(&self, meta: Meta, node: usize) -> Meta {
        Meta {
            path: meta.path,
            span: self.span(Some(node), meta.span),
        }
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

fn text_of(document: &Document, node: usize) -> Option<&str> {
    match document.nodes().get(node) {
        Some(N::Text(text)) => Some(text),
        _ => None,
    }
}

/// `schema_version`, which the schema requires and nothing else reads.
fn header(document: &Document, metas: &mut Metas<'_>, root: usize, meta: Meta) -> Header {
    let node = document.field(root, "schema_version");
    let version = Text {
        text: node
            .and_then(|node| text_of(document, node))
            .unwrap_or_default()
            .into(),
        meta: match node {
            Some(node) => metas.field(meta, "schema_version", Some(node)),
            None => meta,
        },
    };
    Header {
        surface: Surface::V0_5,
        meta: version.meta,
        version,
    }
}

/// Lower a project file. Its study and sections lower beside it: a project
/// whose declarations Define-XML composition could not read still prepares,
/// and a specification still builds with it, so their refusal is kept in
/// [`Project::submission`].
pub fn project(
    document: &Document,
    positions: Option<&Positions>,
) -> Result<Project, Vec<Finding>> {
    let root = document.root();
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let header = header(document, &mut metas, root, meta);
    let mut environments = Vec::new();
    if let Some(node) = document.field(root, "environments") {
        let declared = metas.field(meta, "environments", Some(node));
        for language in [Language::Python, Language::R] {
            let key = language.key();
            let written = reader.optional_text(node, key, &format!("environments.{key}"));
            if let Some(written) = written {
                let meta = metas.field(declared, key, document.field(node, key));
                environments.push(HostEnvironment {
                    language,
                    file: Some(Text {
                        text: written,
                        meta,
                    }),
                    lock: None,
                    meta,
                });
            }
        }
    }
    let mut leaves = Vec::new();
    let mut codelists = Vec::new();
    if let Some(list) = document.field(root, "codelists") {
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
    }
    let submission = sections::lower(document, root, &mut metas, meta)
        .map_err(|_| crate::ast::SubmissionRefused);
    let mut findings = reader.finish();
    findings.extend(leaves);
    if findings.is_empty() {
        Ok(Project {
            header,
            functions: None,
            environments,
            codelists,
            submission,
            paths,
            meta,
        })
    } else {
        Err(findings)
    }
}

/// Lower the environment file a 0.5 project links for `language` into the
/// project root: its lock into that language's environment and its
/// definitions into [`Project::functions`]. On a finding the root is left
/// as it was.
pub fn environment(
    document: &Document,
    positions: Option<&Positions>,
    language: Language,
    project: &mut Project,
) -> Result<(), Vec<Finding>> {
    let root = document.root();
    let mut paths = project.paths.clone();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let lock = reader.optional_text(root, "lock", "lock").map(|text| Text {
        text,
        meta: metas.field(meta, "lock", document.field(root, "lock")),
    });
    let mut leaves = Vec::new();
    let functions = if let Some(node) = document.field(root, "functions") {
        let declared = metas.field(meta, "functions", Some(node));
        let mut functions = Vec::new();
        match document.nodes().get(node) {
            Some(N::Mapping(entries)) => {
                for &(key, value) in entries {
                    let name = reader.text(key, "functions");
                    let path = format!("functions.{name}");
                    let meta = metas.key(declared, &name, Some(value));
                    let source = match document.nodes().get(value) {
                        Some(N::Text(written)) => Some(FunctionSource::File(Text {
                            text: written.clone(),
                            meta,
                        })),
                        Some(N::Mapping(_)) => {
                            match function_at(document, positions, value, &name, language) {
                                Ok(definition) => {
                                    Some(FunctionSource::Inline(Box::new(definition)))
                                }
                                Err(findings) => {
                                    leaves.extend(prefix(findings, &path));
                                    None
                                }
                            }
                        }
                        _ => {
                            reader.fault(&path, value, Kind::NormalizedShape);
                            None
                        }
                    };
                    if let Some(source) = source {
                        let key = Named {
                            name,
                            meta: metas.at(meta, key),
                        };
                        functions.push(FunctionDeclaration {
                            key: Some(key),
                            source,
                            meta,
                        });
                    }
                }
            }
            _ => reader.fault("functions", node, Kind::NormalizedShape),
        }
        Some(functions)
    } else {
        None
    };
    let mut findings = reader.finish();
    findings.extend(leaves);
    if !findings.is_empty() {
        return Err(findings);
    }
    project.paths = paths;
    match project
        .environments
        .iter_mut()
        .find(|environment| environment.language == language)
    {
        Some(environment) => environment.lock = lock,
        None => project.environments.push(HostEnvironment {
            language,
            file: None,
            lock,
            meta,
        }),
    }
    project.functions = functions;
    Ok(())
}

/// Lower a function file that a 0.5 environment file for `language` keys by
/// `name`.
pub fn function(
    document: &Document,
    positions: Option<&Positions>,
    name: &str,
    language: Language,
) -> Result<Function, Vec<Finding>> {
    function_at(document, positions, document.root(), name, language)
}

/// Lower a codelist file.
pub fn codelists(
    document: &Document,
    positions: Option<&Positions>,
) -> Result<Codelists, Vec<Finding>> {
    codelists_at(document, positions, document.root())
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

fn column_kind(reader: &mut Reader<'_>, parent: usize, path: &str) -> ColumnType {
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
fn text_field(
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

/// Decode all scalar leaves before returning any definition. Failed values
/// keep their exact node references; no partial definition is returned.
fn function_at(
    document: &Document,
    positions: Option<&Positions>,
    root: usize,
    name: &str,
    language: Language,
) -> Result<Function, Vec<Finding>> {
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    let at = (root, meta);
    let callable = text_field(
        &mut reader,
        &mut metas,
        document,
        at,
        "function",
        "function",
    );
    let description = text_field(
        &mut reader,
        &mut metas,
        document,
        at,
        "description",
        "description",
    );
    let params = parameters(&mut reader, &mut metas, document, at);
    let returns = column_kind(&mut reader, root, "returns");
    let may_return_missing = reader.boolean(root, "may_return_missing", "may_return_missing");
    let comparison_decimals = reader
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
        .unwrap_or_default();
    let tests = cases(&mut reader, &mut metas, document, at);
    let findings = reader.finish();
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Function {
        header: None,
        name: Named {
            name: name.into(),
            meta,
        },
        description,
        params,
        returns,
        may_return_missing,
        comparison_decimals,
        implementations: vec![Implementation { language, callable }],
        tests,
        paths,
        meta,
    })
}

fn parameters(
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
            let written = document
                .field(node, "name")
                .and_then(|name| text_of(document, name))
                .unwrap_or_default();
            let meta = metas.member(list, i, written, node);
            let at = (node, meta);
            let name = text_field(reader, metas, document, at, "name", &format!("{path}.name"));
            let kind = parameter_kind(reader, node, &format!("{path}.type"));
            let required = reader.boolean(node, "required", &format!("{path}.required"));
            let default = document
                .field(node, "default")
                .map(|value| reader.scalar(value, &format!("{path}.default")));
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

fn cases(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
) -> Vec<FunctionTest> {
    let list = metas.field(meta, "tests", document.field(root, "tests"));
    reader
        .sequence(root, "tests", "tests")
        .iter()
        .enumerate()
        .map(|(i, &node)| {
            let path = format!("tests[{i}]");
            let written = document
                .field(node, "id")
                .and_then(|id| text_of(document, id))
                .unwrap_or_default();
            let meta = metas.member(list, i, written, node);
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
                    for &(key, value) in items {
                        let name = reader.text(key, &format!("{path}.args"));
                        let argument = reader.scalar(value, &format!("{path}.args.{name}"));
                        let meta = metas.key(declared, &name, Some(value));
                        args.push((Named { name, meta }, argument));
                    }
                } else {
                    reader.fault(&format!("{path}.args"), args_node, Kind::NormalizedShape);
                }
            }
            let result = reader
                .field(node, "result", &format!("{path}.result"))
                .map(|node| reader.scalar(node, &format!("{path}.result")))
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

/// Decode a normalized codelist source, keeping the authored source and list
/// order.
fn codelists_at(
    document: &Document,
    positions: Option<&Positions>,
    root: usize,
) -> Result<Codelists, Vec<Finding>> {
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    let standard = document.field(root, "standard").map(|node| {
        let meta = metas.field(meta, "standard", Some(node));
        let at = (node, meta);
        TerminologyStandard {
            name: text_field(
                &mut reader,
                &mut metas,
                document,
                at,
                "name",
                "standard.name",
            ),
            publishing_set: text_field(
                &mut reader,
                &mut metas,
                document,
                at,
                "publishing_set",
                "standard.publishing_set",
            ),
            version: text_field(
                &mut reader,
                &mut metas,
                document,
                at,
                "version",
                "standard.version",
            ),
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
        let written = document
            .field(node, "id")
            .and_then(|id| text_of(document, id))
            .unwrap_or_default();
        let meta = metas.member(list, index, written, node);
        let at = (node, meta);
        let id = text_field(
            &mut reader,
            &mut metas,
            document,
            at,
            "id",
            &format!("{path}.id"),
        );
        let name = text_field(
            &mut reader,
            &mut metas,
            document,
            at,
            "name",
            &format!("{path}.name"),
        );
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
            &mut reader,
            &mut metas,
            document,
            at,
            "alias",
            &format!("{path}.alias"),
        );
        let format_name = optional_text(
            &mut reader,
            &mut metas,
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
                        &mut reader,
                        &mut metas,
                        document,
                        at,
                        "decode",
                        &format!("{path}.decode"),
                    );
                    let rank = document
                        .field(node, "rank")
                        .and_then(|node| reader.integer(node, &format!("{path}.rank")));
                    let alias = optional_text(
                        &mut reader,
                        &mut metas,
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
                    &mut reader,
                    &mut metas,
                    document,
                    at,
                    "dictionary",
                    &format!("{path}.external.dictionary"),
                ),
                version: text_field(
                    &mut reader,
                    &mut metas,
                    document,
                    at,
                    "version",
                    &format!("{path}.external.version"),
                ),
                href: optional_text(
                    &mut reader,
                    &mut metas,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CodelistSource, FunctionSource, Notation};
    use crate::test_document::Tree;

    fn definition(result: Tree) -> Tree {
        Tree::map(vec![
            ("function", Tree::text("project.identity")),
            ("description", Tree::text("Identity")),
            (
                "params",
                Tree::list(vec![Tree::map(vec![
                    ("name", Tree::text("x")),
                    ("type", Tree::text("int")),
                    ("required", Tree::boolean(false)),
                    ("default", Tree::integer(1)),
                    ("accepts_missing", Tree::boolean(false)),
                ])]),
            ),
            ("returns", Tree::text("int")),
            ("may_return_missing", Tree::boolean(false)),
            ("comparison_decimals", Tree::integer(4)),
            (
                "tests",
                Tree::list(vec![Tree::map(vec![
                    ("id", Tree::text("ordinary")),
                    (
                        "covers",
                        Tree::list(vec![Tree::text("normal"), Tree::text("boundary")]),
                    ),
                    ("args", Tree::map(vec![("x", Tree::integer(2))])),
                    ("result", result),
                ])]),
            ),
        ])
    }
    fn project_tree() -> Tree {
        Tree::map(vec![
            ("schema_version", Tree::text("1.0")),
            (
                "environments",
                Tree::map(vec![("python", Tree::text("python/environment.yaml"))]),
            ),
            (
                "codelists",
                Tree::list(vec![
                    Tree::text("ct.yaml"),
                    Tree::map(vec![(
                        "codelists",
                        Tree::list(vec![Tree::map(vec![
                            ("id", Tree::text("SEX")),
                            ("name", Tree::text("Sex")),
                            ("data_type", Tree::text("text")),
                            ("extensible", Tree::boolean(false)),
                            (
                                "items",
                                Tree::list(vec![Tree::map(vec![
                                    ("value", Tree::text("F")),
                                    ("extended", Tree::boolean(false)),
                                ])]),
                            ),
                        ])]),
                    )]),
                ]),
            ),
        ])
    }

    #[test]
    fn every_lowered_node_renders_the_path_its_diagnostics_cite() {
        let function = function(
            &definition(Tree::integer(2)).document(),
            None,
            "identity",
            Language::Python,
        )
        .unwrap();
        let path = |meta: Meta| function.paths.render(meta.path, Notation::V0_5);
        assert_eq!(path(function.params[0].meta), "params[0]");
        assert_eq!(path(function.params[0].name.meta), "params[0].name");
        assert_eq!(path(function.tests[0].id.meta), "tests[0].id");
        assert_eq!(path(function.tests[0].covers[1].meta), "tests[0].covers[1]");
        assert_eq!(path(function.tests[0].args[0].0.meta), "tests[0].args.x");
        assert_eq!(path(function.implementations[0].callable.meta), "function");
        assert_eq!(function.name.name, "identity");
        assert_eq!(function.implementation(Language::R), None);
        let project = project(&project_tree().document(), None).unwrap();
        let path = |meta: Meta| project.paths.render(meta.path, Notation::V0_5);
        let python = project.environment(Language::Python).unwrap();
        assert_eq!(path(python.meta), "environments.python");
        assert!(project.environment(Language::R).is_none());
        assert_eq!(path(project.codelists[1].meta), "codelists[1]");
        let CodelistSource::Inline(source) = &project.codelists[1].source else {
            panic!("inline source")
        };
        // An inline source is a root of its own; diagnostics prefix its
        // declaration.
        let item = &source.codelists[0].items.as_ref().unwrap()[0];
        assert_eq!(
            source.paths.render(item.meta.path, Notation::V0_5),
            "codelists[0].items[0]"
        );
        assert_eq!(
            source
                .paths
                .render(source.codelists[0].id.meta.path, Notation::V0_5),
            "codelists[0].id"
        );
    }

    #[test]
    fn an_environment_file_that_fails_leaves_the_project_root_as_it_was() {
        let mut project = project(&project_tree().document(), None).unwrap();
        let before = project.clone();
        let environment = Tree::map(vec![
            ("schema_version", Tree::text("1.0")),
            ("lock", Tree::text("uv.lock")),
            (
                "functions",
                Tree::map(vec![
                    ("first", Tree::text("first.yaml")),
                    ("wide", definition(Tree::integer("9223372036854775808"))),
                ]),
            ),
        ]);
        let findings = super::environment(
            &environment.clone().document(),
            None,
            Language::Python,
            &mut project,
        )
        .unwrap_err();
        assert_eq!(project, before);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, "functions.wide.tests[0].result");
        assert_eq!(findings[0].value.as_deref(), Some("9223372036854775808"));
        // Once it lowers, its lock and definitions keep its own paths.
        let mut fixed = environment;
        fixed.set(&["functions", "wide"], definition(Tree::integer(2)));
        super::environment(&fixed.document(), None, Language::Python, &mut project).unwrap();
        let lock = project
            .environment(Language::Python)
            .unwrap()
            .lock
            .as_ref()
            .unwrap();
        assert_eq!(lock.text, "uv.lock");
        assert_eq!(project.paths.render(lock.meta.path, Notation::V0_5), "lock");
        let functions = project.functions.as_ref().unwrap();
        assert!(
            matches!(&functions[0].source, FunctionSource::File(path) if path.text == "first.yaml")
        );
        let FunctionSource::Inline(definition) = &functions[1].source else {
            panic!("inline definition")
        };
        assert_eq!(definition.name.name, "wide");
        assert_eq!(
            project.paths.render(functions[1].meta.path, Notation::V0_5),
            "functions.wide"
        );
    }
}
