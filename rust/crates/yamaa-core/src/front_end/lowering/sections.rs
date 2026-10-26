//! Lower a project's study and submission sections, the declarations
//! Define-XML composition reads.
//!
//! Every closed field is checked as it lowers: a declaration of the wrong
//! shape, an unknown field, a missing required field or a value outside its
//! vocabulary refuses the whole set, with every finding naming the exact
//! document nodes. The schema admits only complete declarations, so only a
//! document admitted without it is refused.
//!
//! A project's sections lower here (`lower`), from
//! [`crate::front_end::v1::project`]. A section writes `standards`, a list
//! whose first entry is the default (D16), and `transport: dataset_json`,
//! `dataset_ndjson` or `xpt` (P-PRJ-1, #3821); a guide status may be
//! `Provisional` (P-STD-1).

use super::project::Metas;
use crate::ast::{
    DocumentKind, ImplementationGuide, Meta, MetadataVersion, Named, Section, Study,
    SubmissionDeclarations, SupportingDocument, Text, Transport,
};
use crate::cdisc_vocabulary::{DefineContext, GuideStatus, StandardName};
use crate::diagnostic::ScalarSubmissionRule;
use crate::project_environment::Submission;
use crate::schema::{Document, DocumentNode as N};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldName {
    Study,
    Sdtm,
    Adam,
    Send,
    Id,
    Name,
    Description,
    ProtocolName,
    Standards,
    Version,
    Status,
    Specs,
    Define,
    DefineVersion,
    Documents,
    Transport,
    Context,
    XmlLang,
    FileOid,
    MetadataVersion,
    Originator,
    Stylesheet,
    SourceSystemVersion,
    Kind,
    Link,
    Title,
}
impl FieldName {
    pub fn name(self) -> &'static str {
        match self {
            Self::Study => "study",
            Self::Sdtm => "sdtm",
            Self::Adam => "adam",
            Self::Send => "send",
            Self::Id => "id",
            Self::Name => "name",
            Self::Description => "description",
            Self::ProtocolName => "protocol_name",
            Self::Standards => "standards",
            Self::Version => "version",
            Self::Status => "status",
            Self::Specs => "specs",
            Self::Define => "define",
            Self::DefineVersion => "define_version",
            Self::Documents => "documents",
            Self::Transport => "transport",
            Self::Context => "context",
            Self::XmlLang => "xml_lang",
            Self::FileOid => "file_oid",
            Self::MetadataVersion => "metadata_version",
            Self::Originator => "originator",
            Self::Stylesheet => "stylesheet",
            Self::SourceSystemVersion => "source_system_version",
            Self::Kind => "kind",
            Self::Link => "link",
            Self::Title => "title",
        }
    }
}
/// The container/key/value occurrences of one field. A missing key and
/// value identify an absent field, not an authored occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub container: usize,
    pub key: Option<usize>,
    pub node: Option<usize>,
    pub field: FieldName,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Field(FieldName),
    Index(usize),
}
/// A closed, fixed-capacity field path needs no unbounded diagnostic strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    steps: [Option<Step>; 6],
    len: usize,
}
impl Path {
    pub fn steps(&self) -> impl Iterator<Item = Step> + '_ {
        self.steps[..self.len].iter().flatten().copied()
    }
    fn root() -> Self {
        Self {
            steps: [None; 6],
            len: 0,
        }
    }
    fn child(mut self, step: Step) -> Self {
        // All paths have at most four segments, even on malformed input.
        self.steps[self.len] = Some(step);
        self.len += 1;
        self
    }
    fn field(self, field: FieldName) -> Self {
        self.child(Step::Field(field))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Missing,
    ExpectedMapping,
    ExpectedSequence,
    ExpectedText,
    NonTextKey,
    UnknownField,
    EmptyText,
    InvalidIdentifier,
    InvalidDefineId,
    InvalidHref,
    InvalidLanguage,
    InvalidChoice,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub node: usize,
    /// Even unknown or non-text keys retain their exact field occurrences.
    pub container: Option<usize>,
    pub key: Option<usize>,
    pub origin: Option<Origin>,
    pub path: Path,
    pub kind: Kind,
}
impl Finding {
    /// Only scalar rules whose current normative causes are known are
    /// portable here. Shapes and unsupported choices keep their facts.
    pub fn scalar_rule(&self) -> Option<ScalarSubmissionRule> {
        match self.kind {
            Kind::InvalidIdentifier => Some(ScalarSubmissionRule::Identifier),
            Kind::InvalidDefineId => Some(ScalarSubmissionRule::DefineId),
            Kind::InvalidHref => Some(ScalarSubmissionRule::Href),
            Kind::InvalidLanguage => Some(ScalarSubmissionRule::Language),
            Kind::InvalidChoice
                if self.origin.map(|v| v.field) == Some(FieldName::Name)
                    && self
                        .path
                        .steps()
                        .any(|v| matches!(v, Step::Field(FieldName::Standards))) =>
            {
                Some(ScalarSubmissionRule::StandardName)
            }
            _ => None,
        }
    }
    pub fn rule(&self) -> Option<&'static str> {
        self.scalar_rule().map(ScalarSubmissionRule::rule)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Shape {
        node: usize,
    },
    /// The complete set of findings, never a fitting prefix.
    Findings(Vec<Finding>),
}
#[derive(Debug, thiserror::Error)]
#[error("project sections: cause={cause:?}")]
pub struct Error<'a> {
    pub input: &'a Document,
    pub cause: Cause,
}

const STUDY: &[FieldName] = &[
    FieldName::Id,
    FieldName::Name,
    FieldName::Description,
    FieldName::ProtocolName,
];
const GUIDE: &[FieldName] = &[FieldName::Name, FieldName::Version, FieldName::Status];
const METADATA: &[FieldName] = &[FieldName::Id, FieldName::Name, FieldName::Description];
const DOCUMENT: &[FieldName] = &[
    FieldName::Id,
    FieldName::Kind,
    FieldName::Link,
    FieldName::Title,
];
/// A section's fields: `standards` and `transport` (D16, P-PRJ-1).
const SECTION: &[FieldName] = &[
    FieldName::Specs,
    FieldName::Standards,
    FieldName::Define,
    FieldName::DefineVersion,
    FieldName::Documents,
    FieldName::Transport,
    FieldName::Context,
    FieldName::XmlLang,
    FieldName::FileOid,
    FieldName::MetadataVersion,
    FieldName::Originator,
    FieldName::Stylesheet,
    FieldName::SourceSystemVersion,
];
fn family(field: &str) -> Option<(Submission, FieldName)> {
    match field {
        "sdtm" => Some((Submission::Sdtm, FieldName::Sdtm)),
        "adam" => Some((Submission::Adam, FieldName::Adam)),
        "send" => Some((Submission::Send, FieldName::Send)),
        _ => None,
    }
}

/// Lower the study and sections of the project whose root is `root`, with
/// every meta in the project's arena under `meta`.
pub(in crate::front_end) fn lower<'d>(
    input: &'d Document,
    root: usize,
    metas: &mut Metas<'_>,
    meta: Meta,
) -> Result<SubmissionDeclarations, Error<'d>> {
    let refuse = |cause| Error { input, cause };
    let Some(N::Mapping(_)) = input.nodes().get(root) else {
        return Err(refuse(Cause::Shape { node: root }));
    };
    let mut parser = Parser {
        input,
        metas,
        findings: Vec::new(),
    };
    let declarations = parser.run(root, meta);
    if parser.findings.is_empty() {
        Ok(declarations)
    } else {
        Err(refuse(Cause::Findings(parser.findings)))
    }
}

struct Parser<'d, 'm, 'p> {
    input: &'d Document,
    metas: &'m mut Metas<'p>,
    findings: Vec<Finding>,
}
#[derive(Clone, Copy)]
enum TextRule {
    NonEmpty,
    Identifier,
    DefineId,
    Href,
    Language,
}
impl<'d> Parser<'d, '_, '_> {
    fn fault(&mut self, node: usize, origin: Option<Origin>, path: Path, kind: Kind) {
        self.fault_at(
            node,
            origin,
            origin.map(|v| (v.container, v.key)),
            path,
            kind,
        )
    }
    fn fault_at(
        &mut self,
        node: usize,
        origin: Option<Origin>,
        field: Option<(usize, Option<usize>)>,
        path: Path,
        kind: Kind,
    ) {
        self.findings.push(Finding {
            node,
            container: field.map(|v| v.0),
            key: field.and_then(|v| v.1),
            origin,
            path,
            kind,
        });
    }
    fn get(&self, container: usize, field: FieldName) -> Origin {
        let found = match self.input.nodes().get(container) {
            Some(N::Mapping(fields)) => fields.iter().find(|&&(key, _)| {
                matches!(&self.input.nodes()[key], N::Text(text) if text == field.name())
            }),
            _ => None,
        };
        Origin {
            container,
            field,
            key: found.map(|v| v.0),
            node: found.map(|v| v.1),
        }
    }
    /// The meta of `origin`'s field inside `parent`.
    fn meta(&mut self, parent: Meta, origin: Origin) -> Meta {
        self.metas.field(parent, origin.field.name(), origin.node)
    }
    fn mapping(&mut self, origin: Origin, path: Path, allowed: &[FieldName]) -> bool {
        let Some(node) = origin.node else {
            self.fault(origin.container, Some(origin), path, Kind::Missing);
            return false;
        };
        let N::Mapping(fields) = &self.input.nodes()[node] else {
            self.fault(node, Some(origin), path, Kind::ExpectedMapping);
            return false;
        };
        for &(key, value) in fields {
            match &self.input.nodes()[key] {
                N::Text(text) if allowed.iter().any(|v| v.name() == text) => {}
                N::Text(_) => self.fault_at(
                    value,
                    None,
                    Some((node, Some(key))),
                    path,
                    Kind::UnknownField,
                ),
                _ => self.fault_at(key, None, Some((node, Some(key))), path, Kind::NonTextKey),
            }
        }
        true
    }
    fn text(
        &mut self,
        origin: Origin,
        path: Path,
        rule: TextRule,
        default: Option<&str>,
    ) -> Option<String> {
        let value = match origin.node {
            Some(node) => match &self.input.nodes()[node] {
                N::Text(text) => text.as_str(),
                _ => {
                    self.fault(node, Some(origin), path, Kind::ExpectedText);
                    return None;
                }
            },
            None => match default {
                Some(value) => value,
                None => {
                    self.fault(origin.container, Some(origin), path, Kind::Missing);
                    return None;
                }
            },
        };
        let failure = if value.is_empty() {
            Some(match rule {
                TextRule::NonEmpty => Kind::EmptyText,
                TextRule::Identifier => Kind::InvalidIdentifier,
                TextRule::DefineId => Kind::InvalidDefineId,
                TextRule::Href => Kind::InvalidHref,
                TextRule::Language => Kind::InvalidLanguage,
            })
        } else {
            match rule {
                TextRule::NonEmpty => None,
                TextRule::Identifier if !identifier(value, false) => Some(Kind::InvalidIdentifier),
                TextRule::DefineId if !identifier(value, true) => Some(Kind::InvalidDefineId),
                TextRule::Href
                    if value.starts_with('/')
                        || value.contains('\\')
                        || value.split('/').any(|v| v == "..") =>
                {
                    Some(Kind::InvalidHref)
                }
                TextRule::Language if !language(value) => Some(Kind::InvalidLanguage),
                _ => None,
            }
        };
        if let Some(kind) = failure {
            self.fault(
                origin.node.unwrap_or(origin.container),
                Some(origin),
                path,
                kind,
            );
            return None;
        }
        Some(value.into())
    }
    /// A text field and its meta inside `parent`.
    fn text_at(
        &mut self,
        origin: Origin,
        path: Path,
        rule: TextRule,
        default: Option<&str>,
        parent: Meta,
    ) -> Option<Text> {
        let text = self.text(origin, path, rule, default)?;
        Some(Text {
            text,
            meta: self.meta(parent, origin),
        })
    }
    fn required_text(
        &mut self,
        (node, parent): (usize, Meta),
        field: FieldName,
        path: Path,
        rule: TextRule,
    ) -> Option<Text> {
        self.text_at(self.get(node, field), path.field(field), rule, None, parent)
    }
    fn optional_text(
        &mut self,
        (node, parent): (usize, Meta),
        field: FieldName,
        path: Path,
        rule: TextRule,
    ) -> Option<Text> {
        let origin = self.get(node, field);
        origin.node?;
        self.text_at(origin, path.field(field), rule, None, parent)
    }
    fn choice<T: Copy>(
        &mut self,
        origin: Origin,
        path: Path,
        choices: &[(&str, T)],
        default: Option<T>,
    ) -> Option<T> {
        let Some(node) = origin.node else {
            if default.is_some() {
                return default;
            }
            self.fault(origin.container, Some(origin), path, Kind::Missing);
            return None;
        };
        let N::Text(text) = &self.input.nodes()[node] else {
            self.fault(node, Some(origin), path, Kind::ExpectedText);
            return None;
        };
        match choices.iter().find(|v| v.0 == text) {
            Some(&(_, value)) => Some(value),
            None => {
                self.fault(node, Some(origin), path, Kind::InvalidChoice);
                None
            }
        }
    }
    fn sequence(&mut self, origin: Origin, path: Path, optional: bool) -> Option<&'d [usize]> {
        let Some(node) = origin.node else {
            if optional {
                return Some(&[]);
            }
            self.fault(origin.container, Some(origin), path, Kind::Missing);
            return None;
        };
        match &self.input.nodes()[node] {
            N::Sequence(items) => Some(items),
            _ => {
                self.fault(node, Some(origin), path, Kind::ExpectedSequence);
                None
            }
        }
    }
    fn study(&mut self, origin: Origin, path: Path, parent: Meta) -> Option<Study> {
        if !self.mapping(origin, path, STUDY) {
            return None;
        }
        let node = origin.node.unwrap();
        let meta = self.meta(parent, origin);
        let at = (node, meta);
        let id = self.required_text(at, FieldName::Id, path, TextRule::DefineId);
        let name = self.required_text(at, FieldName::Name, path, TextRule::NonEmpty);
        let description = self.required_text(at, FieldName::Description, path, TextRule::NonEmpty);
        let protocol_name =
            self.required_text(at, FieldName::ProtocolName, path, TextRule::NonEmpty);
        Some(Study {
            id: named(id?),
            name: name?,
            description: description?,
            protocol_name: protocol_name?,
            meta,
        })
    }
    fn metadata(&mut self, origin: Origin, path: Path, parent: Meta) -> Option<MetadataVersion> {
        if !self.mapping(origin, path, METADATA) {
            return None;
        }
        let node = origin.node.unwrap();
        let meta = self.meta(parent, origin);
        let at = (node, meta);
        let id = self.required_text(at, FieldName::Id, path, TextRule::DefineId);
        let name = self.required_text(at, FieldName::Name, path, TextRule::NonEmpty);
        let description = self.optional_text(at, FieldName::Description, path, TextRule::NonEmpty);
        Some(MetadataVersion {
            id: named(id?),
            name: name?,
            description,
            meta,
        })
    }
    /// An implementation guide at `meta`, a `standards` entry.
    fn guide(&mut self, origin: Origin, path: Path, meta: Meta) -> Option<ImplementationGuide> {
        if !self.mapping(origin, path, GUIDE) {
            return None;
        }
        let node = origin.node.unwrap();
        let name_origin = self.get(node, FieldName::Name);
        let name = self
            .choice(
                name_origin,
                path.field(FieldName::Name),
                StandardName::VALUES,
                None,
            )
            .map(|name| Text {
                text: name.name().into(),
                meta: self.meta(meta, name_origin),
            });
        let version =
            self.required_text((node, meta), FieldName::Version, path, TextRule::NonEmpty);
        let status = self.choice(
            self.get(node, FieldName::Status),
            path.field(FieldName::Status),
            GuideStatus::VALUES,
            None,
        );
        Some(ImplementationGuide {
            name: name?,
            version: version?,
            status: status?,
            meta,
        })
    }
    fn document(
        &mut self,
        node: usize,
        parent: Origin,
        path: Path,
        meta: Meta,
    ) -> Option<SupportingDocument> {
        let origin = Origin {
            container: parent.node.expect("document list occurrence"),
            key: None,
            node: Some(node),
            field: FieldName::Documents,
        };
        if !self.mapping(origin, path, DOCUMENT) {
            return None;
        }
        let at = (node, meta);
        let id = self.required_text(at, FieldName::Id, path, TextRule::Identifier);
        let kind = self.choice(
            self.get(node, FieldName::Kind),
            path.field(FieldName::Kind),
            &[
                ("annotated_crf", DocumentKind::AnnotatedCrf),
                ("supplemental", DocumentKind::Supplemental),
                ("other", DocumentKind::Other),
            ],
            None,
        );
        let link = self.required_text(at, FieldName::Link, path, TextRule::Href);
        let title = self.required_text(at, FieldName::Title, path, TextRule::NonEmpty);
        Some(SupportingDocument {
            id: named(id?),
            kind: kind?,
            link: link?,
            title: title?,
            meta,
        })
    }
    /// A section's `standards`, each entry at `standards[i]`. An omitted list
    /// declares none.
    fn standards(&mut self, node: usize, path: Path, meta: Meta) -> Vec<ImplementationGuide> {
        let origin = self.get(node, FieldName::Standards);
        let list_path = path.field(FieldName::Standards);
        let items = self.sequence(origin, list_path, true).unwrap_or(&[]);
        let listed = self.meta(meta, origin);
        let mut guides = Vec::with_capacity(items.len());
        for (index, &item) in items.iter().enumerate() {
            let entry = Origin {
                container: origin.node.expect("standards list occurrence"),
                key: None,
                node: Some(item),
                field: FieldName::Standards,
            };
            let at = self.metas.index(listed, index, item);
            if let Some(guide) = self.guide(entry, list_path.child(Step::Index(index)), at) {
                guides.push(guide);
            }
        }
        guides
    }
    fn section(
        &mut self,
        family: Submission,
        origin: Origin,
        path: Path,
        parent: Meta,
    ) -> Option<Section> {
        if !self.mapping(origin, path, SECTION) {
            return None;
        }
        let node = origin.node.unwrap();
        let meta = self.meta(parent, origin);
        let at = (node, meta);
        // Only Define-XML composition requires the guides and target; a
        // section that lists its specifications already admits for runs.
        let standards = self.standards(node, path, meta);
        let specs_origin = self.get(node, FieldName::Specs);
        let specs_path = path.field(FieldName::Specs);
        let spec_nodes = self
            .sequence(specs_origin, specs_path, false)
            .unwrap_or(&[]);
        let listed = self.meta(meta, specs_origin);
        let mut specs = Vec::with_capacity(spec_nodes.len());
        for (index, &item) in spec_nodes.iter().enumerate() {
            let item_origin = Origin {
                container: specs_origin.node.unwrap(),
                key: None,
                node: Some(item),
                field: FieldName::Specs,
            };
            if let Some(text) = self.text(
                item_origin,
                specs_path.child(Step::Index(index)),
                TextRule::NonEmpty,
                None,
            ) {
                specs.push(Text {
                    text,
                    meta: self.metas.index(listed, index, item),
                });
            }
        }
        let define = self.optional_text(at, FieldName::Define, path, TextRule::NonEmpty);
        let define_version = self.choice(
            self.get(node, FieldName::DefineVersion),
            path.field(FieldName::DefineVersion),
            crate::ast::project::DefineVersion::VALUES,
            Some(crate::ast::project::DefineVersion::default()),
        );
        let docs_origin = self.get(node, FieldName::Documents);
        let docs_path = path.field(FieldName::Documents);
        let doc_nodes = self.sequence(docs_origin, docs_path, true).unwrap_or(&[]);
        let listed = self.meta(meta, docs_origin);
        let mut documents = Vec::with_capacity(doc_nodes.len());
        for (index, &item) in doc_nodes.iter().enumerate() {
            let meta = self.metas.index(listed, index, item);
            if let Some(value) =
                self.document(item, docs_origin, docs_path.child(Step::Index(index)), meta)
            {
                documents.push(value);
            }
        }
        let transport_origin = self.get(node, FieldName::Transport);
        let transport = if transport_origin.node.is_some() {
            self.choice(
                transport_origin,
                path.field(FieldName::Transport),
                &[
                    ("dataset_json", Transport::DatasetJson),
                    ("dataset_ndjson", Transport::DatasetNdjson),
                    ("xpt", Transport::Xpt),
                ],
                None,
            )
        } else {
            None
        };
        let context = self.choice(
            self.get(node, FieldName::Context),
            path.field(FieldName::Context),
            DefineContext::VALUES,
            Some(DefineContext::Submission),
        );
        let xml_lang = self.text_at(
            self.get(node, FieldName::XmlLang),
            path.field(FieldName::XmlLang),
            TextRule::Language,
            Some("en"),
            meta,
        );
        let file_oid = self.optional_text(at, FieldName::FileOid, path, TextRule::NonEmpty);
        let metadata_origin = self.get(node, FieldName::MetadataVersion);
        let metadata_version = if metadata_origin.node.is_some() {
            self.metadata(
                metadata_origin,
                path.field(FieldName::MetadataVersion),
                meta,
            )
        } else {
            None
        };
        let originator = self.optional_text(at, FieldName::Originator, path, TextRule::NonEmpty);
        let stylesheet = self.optional_text(at, FieldName::Stylesheet, path, TextRule::Href);
        let source_system_version =
            self.optional_text(at, FieldName::SourceSystemVersion, path, TextRule::NonEmpty);
        Some(Section {
            family,
            specs,
            standards,
            define,
            define_version: define_version?,
            documents,
            transport,
            context: context?,
            xml_lang: xml_lang?,
            file_oid,
            metadata_version,
            originator,
            stylesheet,
            source_system_version,
            meta,
        })
    }
    fn run(&mut self, root: usize, meta: Meta) -> SubmissionDeclarations {
        let N::Mapping(fields) = &self.input.nodes()[root] else {
            unreachable!("preflight root")
        };
        let mut study = None;
        let count = fields
            .iter()
            .filter(|&&(key, _)| matches!(&self.input.nodes()[key], N::Text(text) if family(text).is_some()))
            .count();
        let mut sections = Vec::with_capacity(count);
        for &(key, node) in fields {
            let N::Text(text) = &self.input.nodes()[key] else {
                self.fault_at(
                    key,
                    None,
                    Some((root, Some(key))),
                    Path::root(),
                    Kind::NonTextKey,
                );
                continue;
            };
            if text == "study" {
                let origin = Origin {
                    container: root,
                    key: Some(key),
                    node: Some(node),
                    field: FieldName::Study,
                };
                study = self.study(origin, Path::root().field(FieldName::Study), meta);
            } else if let Some((family, field)) = family(text) {
                let origin = Origin {
                    container: root,
                    key: Some(key),
                    node: Some(node),
                    field,
                };
                if let Some(section) = self.section(family, origin, Path::root().field(field), meta)
                {
                    sections.push(section);
                }
            }
        }
        SubmissionDeclarations { study, sections }
    }
}

fn named(text: Text) -> Named {
    Named {
        name: text.text,
        meta: text.meta,
    }
}
fn identifier(text: &str, define: bool) -> bool {
    let mut bytes = text.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && bytes
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || (define && (b == b'.' || b == b'-')))
}
fn language(text: &str) -> bool {
    let mut parts = text.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    (2..=3).contains(&first.len())
        && first.bytes().all(|v| v.is_ascii_alphabetic())
        && parts.all(|v| (1..=8).contains(&v.len()) && v.bytes().all(|v| v.is_ascii_alphanumeric()))
}
