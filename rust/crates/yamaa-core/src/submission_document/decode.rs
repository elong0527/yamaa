//! A non-owning admission pass precedes one complete owned decode. Both traverse
//! the same immutable document; refusal keeps that exact borrowed input.
use super::*;
use alloc::{string::String, vec::Vec};

struct Budget(Limits);
impl Budget {
    fn charge(&mut self, resource: Resource, amount: usize) -> Result<(), Cause> {
        let remaining = match resource {
            Resource::Fields => &mut self.0.fields,
            Resource::Declarations => &mut self.0.declarations,
            Resource::Work => &mut self.0.work,
            Resource::Nodes => &mut self.0.nodes,
            Resource::ListItems => &mut self.0.list_items,
            Resource::TextBytes => &mut self.0.text_bytes,
            Resource::EscapedBytes => &mut self.0.escaped_bytes,
            Resource::PathSegments => &mut self.0.path_segments,
            Resource::Findings => &mut self.0.findings,
            Resource::Depth => unreachable!("depth is a maximum"),
        };
        *remaining = remaining
            .checked_sub(amount)
            .ok_or(Cause::Limit(resource))?;
        Ok(())
    }
    fn multiply(&mut self, resource: Resource, amount: usize, factor: usize) -> Result<(), Cause> {
        self.charge(
            resource,
            amount.checked_mul(factor).ok_or(Cause::Limit(resource))?,
        )
    }
    fn text(&mut self, text: &str) -> Result<(), Cause> {
        self.charge(Resource::TextBytes, text.len())?;
        // Every scalar can be quoted with at most ten bytes per UTF-8 byte plus
        // two quotes. This reserves a complete projection, not a fitting prefix.
        self.multiply(Resource::EscapedBytes, text.len(), 10)?;
        self.charge(Resource::EscapedBytes, 2)?;
        self.multiply(Resource::Work, text.len(), 32)
    }
    fn mapping(
        &mut self,
        input: &Document,
        fields: &[(usize, usize)],
        depth: usize,
    ) -> Result<(), Cause> {
        if !fields.is_empty() && depth >= self.0.depth.min(64) {
            return Err(Cause::Limit(Resource::Depth));
        }
        self.charge(Resource::Fields, fields.len())?;
        // Covers the fixed closed-field comparisons in both passes, including
        // every unsuccessful lookup. No lookup runs before this reservation.
        self.multiply(Resource::Work, fields.len(), 128)?;
        for &(key, _) in fields {
            self.charge(Resource::Nodes, 1)?;
            if let N::Text(text) = &input.nodes()[key] {
                self.text(text)?;
            }
        }
        Ok(())
    }
    fn tree(&mut self, input: &Document, node: usize, depth: usize) -> Result<(), Cause> {
        // Malformed nested values also have a fixed native stack bound.
        if depth > self.0.depth.min(64) {
            return Err(Cause::Limit(Resource::Depth));
        }
        self.charge(Resource::Nodes, 1)?;
        self.charge(Resource::Work, 32)?;
        match &input.nodes()[node] {
            N::Text(text) | N::Integer(text) => self.text(text)?,
            N::Sequence(items) => {
                self.charge(Resource::ListItems, items.len())?;
                self.multiply(Resource::Work, items.len(), 32)?;
                for &child in items {
                    self.tree(input, child, depth + 1)?;
                }
            }
            N::Mapping(fields) => {
                self.mapping(input, fields, depth)?;
                for &(_, child) in fields {
                    self.tree(input, child, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
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
const SECTION: &[FieldName] = &[
    FieldName::Standard,
    FieldName::Specs,
    FieldName::Define,
    FieldName::Documents,
    FieldName::DatasetJson,
    FieldName::Context,
    FieldName::XmlLang,
    FieldName::FileOid,
    FieldName::MetadataVersion,
    FieldName::Originator,
    FieldName::Stylesheet,
    FieldName::SourceSystemVersion,
];
impl Path {
    fn root() -> Self {
        Self {
            steps: [None; 6],
            len: 0,
        }
    }
    fn child(mut self, step: Step) -> Self {
        // All decoder paths have at most four segments, even on malformed input.
        self.steps[self.len] = Some(step);
        self.len += 1;
        self
    }
    fn field(self, field: FieldName) -> Self {
        self.child(Step::Field(field))
    }
}
fn family(field: &str) -> Option<(Submission, FieldName)> {
    match field {
        "sdtm" => Some((Submission::Sdtm, FieldName::Sdtm)),
        "adam" => Some((Submission::Adam, FieldName::Adam)),
        "send" => Some((Submission::Send, FieldName::Send)),
        _ => None,
    }
}
/// Decode only submission declaration metadata. Other environment domains are
/// left to their existing admission. This supplies no file or execution grant.
pub fn decode(input: &Document, root: usize, limits: Limits) -> Result<Declarations, Error<'_>> {
    let refuse = |cause| Error { input, cause };
    let Some(N::Mapping(fields)) = input.nodes().get(root) else {
        return Err(refuse(Cause::Shape { node: root }));
    };
    let mut budget = Budget(limits);
    let admission = (|| {
        if budget.0.depth == 0 {
            return Err(Cause::Limit(Resource::Depth));
        }
        budget.charge(Resource::Nodes, 1)?;
        budget.charge(Resource::Work, 32)?;
        budget.mapping(input, fields, 1)?;
        for &(key, node) in fields {
            if let N::Text(text) = &input.nodes()[key] {
                if text == "study" || family(text).is_some() {
                    budget.charge(Resource::Declarations, 1)?;
                    budget.tree(input, node, 2)?;
                }
            }
        }
        // No list, model string, field path or diagnostic buffer has grown.
        let mut pass = Parser {
            input,
            budget: Some(&mut budget),
            model: false,
            findings: Vec::new(),
            count: 0,
        };
        pass.run(root)?;
        Ok(pass.count)
    })()
    .map_err(refuse)?;
    // The exact whole finding capacity is admitted before allocation. If any
    // structural error exists, no model strings or lists are built at all.
    let mut owner = Parser {
        input,
        budget: None,
        model: admission == 0,
        findings: Vec::with_capacity(admission),
        count: 0,
    };
    let declarations = owner.run(root).map_err(refuse)?;
    if owner.findings.is_empty() {
        Ok(declarations)
    } else {
        Err(refuse(Cause::Findings(owner.findings)))
    }
}
struct Parser<'a, 'b> {
    input: &'a Document,
    budget: Option<&'b mut Budget>,
    model: bool,
    findings: Vec<Finding>,
    count: usize,
}
#[derive(Clone, Copy)]
enum TextRule {
    NonEmpty,
    Identifier,
    DefineId,
    Href,
    Language,
}
impl<'a> Parser<'a, '_> {
    fn fault(
        &mut self,
        node: usize,
        origin: Option<Origin>,
        path: Path,
        kind: Kind,
    ) -> Result<(), Cause> {
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
    ) -> Result<(), Cause> {
        if let Some(budget) = &mut self.budget {
            budget.charge(Resource::Findings, 1)?;
            budget.charge(Resource::PathSegments, path.len)?;
            budget.charge(Resource::Work, path.len + 1)?;
        } else {
            self.findings.push(Finding {
                node,
                container: field.map(|v| v.0),
                key: field.and_then(|v| v.1),
                origin,
                path,
                kind,
            });
        }
        self.count = self
            .count
            .checked_add(1)
            .ok_or(Cause::Limit(Resource::Findings))?;
        Ok(())
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
    fn mapping(
        &mut self,
        origin: Origin,
        path: Path,
        allowed: &[FieldName],
    ) -> Result<bool, Cause> {
        let Some(node) = origin.node else {
            self.fault(origin.container, Some(origin), path, Kind::Missing)?;
            return Ok(false);
        };
        let N::Mapping(fields) = &self.input.nodes()[node] else {
            self.fault(node, Some(origin), path, Kind::ExpectedMapping)?;
            return Ok(false);
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
                )?,
                _ => self.fault_at(key, None, Some((node, Some(key))), path, Kind::NonTextKey)?,
            }
        }
        Ok(true)
    }
    fn text(
        &mut self,
        origin: Origin,
        path: Path,
        rule: TextRule,
        default: Option<&str>,
    ) -> Result<Option<Field<String>>, Cause> {
        let value = match origin.node {
            Some(node) => match &self.input.nodes()[node] {
                N::Text(text) => text.as_str(),
                _ => {
                    self.fault(node, Some(origin), path, Kind::ExpectedText)?;
                    return Ok(None);
                }
            },
            None => match default {
                Some(value) => {
                    if let Some(budget) = &mut self.budget {
                        budget.text(value)?;
                    }
                    value
                }
                None => {
                    self.fault(origin.container, Some(origin), path, Kind::Missing)?;
                    return Ok(None);
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
            )?;
            return Ok(None);
        }
        Ok(Some(Field {
            value: if self.model {
                value.into()
            } else {
                String::new()
            },
            origin,
        }))
    }
    fn required_text(
        &mut self,
        node: usize,
        field: FieldName,
        path: Path,
        rule: TextRule,
    ) -> Result<Option<Field<String>>, Cause> {
        self.text(self.get(node, field), path.field(field), rule, None)
    }
    fn optional_text(
        &mut self,
        node: usize,
        field: FieldName,
        path: Path,
        rule: TextRule,
    ) -> Result<Option<Field<String>>, Cause> {
        let origin = self.get(node, field);
        if origin.node.is_none() {
            return Ok(None);
        }
        self.text(origin, path.field(field), rule, None)
    }
    fn choice<T: Copy>(
        &mut self,
        origin: Origin,
        path: Path,
        choices: &[(&str, T)],
        default: Option<T>,
    ) -> Result<Option<Field<T>>, Cause> {
        let Some(node) = origin.node else {
            if let Some(value) = default {
                return Ok(Some(Field { value, origin }));
            }
            self.fault(origin.container, Some(origin), path, Kind::Missing)?;
            return Ok(None);
        };
        let N::Text(text) = &self.input.nodes()[node] else {
            self.fault(node, Some(origin), path, Kind::ExpectedText)?;
            return Ok(None);
        };
        match choices.iter().find(|v| v.0 == text) {
            Some(&(_, value)) => Ok(Some(Field { value, origin })),
            None => {
                self.fault(node, Some(origin), path, Kind::InvalidChoice)?;
                Ok(None)
            }
        }
    }
    fn boolean(&mut self, origin: Origin, path: Path) -> Result<Option<Field<bool>>, Cause> {
        match origin.node {
            None => Ok(Some(Field {
                value: false,
                origin,
            })),
            Some(node) => match self.input.nodes()[node] {
                N::Boolean(value) => Ok(Some(Field { value, origin })),
                _ => {
                    self.fault(node, Some(origin), path, Kind::ExpectedBoolean)?;
                    Ok(None)
                }
            },
        }
    }
    fn sequence(
        &mut self,
        origin: Origin,
        path: Path,
        optional: bool,
    ) -> Result<Option<&'a [usize]>, Cause> {
        let Some(node) = origin.node else {
            if optional {
                return Ok(Some(&[]));
            }
            self.fault(origin.container, Some(origin), path, Kind::Missing)?;
            return Ok(None);
        };
        match &self.input.nodes()[node] {
            N::Sequence(items) => Ok(Some(items)),
            _ => {
                self.fault(node, Some(origin), path, Kind::ExpectedSequence)?;
                Ok(None)
            }
        }
    }
    fn study(&mut self, origin: Origin, path: Path) -> Result<Option<Study>, Cause> {
        if !self.mapping(origin, path, STUDY)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let id = self.required_text(node, FieldName::Id, path, TextRule::DefineId)?;
        let name = self.required_text(node, FieldName::Name, path, TextRule::NonEmpty)?;
        let description =
            self.required_text(node, FieldName::Description, path, TextRule::NonEmpty)?;
        let protocol_name =
            self.required_text(node, FieldName::ProtocolName, path, TextRule::NonEmpty)?;
        Ok(id.zip(name).zip(description).zip(protocol_name).map(
            |(((id, name), description), protocol_name)| Study {
                origin,
                id,
                name,
                description,
                protocol_name,
            },
        ))
    }
    fn metadata(&mut self, origin: Origin, path: Path) -> Result<Option<MetadataVersion>, Cause> {
        if !self.mapping(origin, path, METADATA)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let id = self.required_text(node, FieldName::Id, path, TextRule::DefineId)?;
        let name = self.required_text(node, FieldName::Name, path, TextRule::NonEmpty)?;
        let description =
            self.optional_text(node, FieldName::Description, path, TextRule::NonEmpty)?;
        Ok(id.zip(name).map(|(id, name)| MetadataVersion {
            origin,
            id,
            name,
            description,
        }))
    }
    fn guide(&mut self, origin: Origin, path: Path) -> Result<Option<ImplementationGuide>, Cause> {
        if !self.mapping(origin, path, GUIDE)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let name = self.choice(
            self.get(node, FieldName::Name),
            path.field(FieldName::Name),
            &[
                ("ADaMIG", GuideName::AdamIg),
                ("ADaMIG-MD", GuideName::AdamIgMd),
                ("BIMO", GuideName::Bimo),
                ("CDISC/NCI", GuideName::CdiscNci),
                ("SDTMIG", GuideName::SdtmIg),
                ("SDTMIG-AP", GuideName::SdtmIgAp),
                ("SDTMIG-MD", GuideName::SdtmIgMd),
                ("SENDIG", GuideName::SendIg),
                ("SENDIG-AR", GuideName::SendIgAr),
                ("SENDIG-DART", GuideName::SendIgDart),
                ("SENDIG-GENETOX", GuideName::SendIgGenetox),
            ],
            None,
        )?;
        let version = self.required_text(node, FieldName::Version, path, TextRule::NonEmpty)?;
        let status = self.choice(
            self.get(node, FieldName::Status),
            path.field(FieldName::Status),
            &[("Draft", GuideStatus::Draft), ("Final", GuideStatus::Final)],
            None,
        )?;
        Ok(name
            .zip(version)
            .zip(status)
            .map(|((name, version), status)| ImplementationGuide {
                origin,
                name,
                version,
                status,
            }))
    }
    fn document(
        &mut self,
        node: usize,
        parent: Origin,
        path: Path,
    ) -> Result<Option<DocumentReference>, Cause> {
        let origin = Origin {
            container: parent.node.expect("document list occurrence"),
            key: None,
            node: Some(node),
            field: FieldName::Documents,
        };
        if !self.mapping(origin, path, DOCUMENT)? {
            return Ok(None);
        }
        let id = self.required_text(node, FieldName::Id, path, TextRule::Identifier)?;
        let kind = self.choice(
            self.get(node, FieldName::Kind),
            path.field(FieldName::Kind),
            &[
                ("annotated_crf", DocumentKind::AnnotatedCrf),
                ("supplemental", DocumentKind::Supplemental),
                ("other", DocumentKind::Other),
            ],
            None,
        )?;
        let link = self.required_text(node, FieldName::Link, path, TextRule::Href)?;
        let title = self.required_text(node, FieldName::Title, path, TextRule::NonEmpty)?;
        Ok(id
            .zip(kind)
            .zip(link)
            .zip(title)
            .map(|(((id, kind), link), title)| DocumentReference {
                node,
                id,
                kind,
                link,
                title,
            }))
    }
    fn section(
        &mut self,
        family: Submission,
        origin: Origin,
        path: Path,
    ) -> Result<Option<Section>, Cause> {
        if !self.mapping(origin, path, SECTION)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let standard = self.guide(
            self.get(node, FieldName::Standard),
            path.field(FieldName::Standard),
        )?;
        let specs_origin = self.get(node, FieldName::Specs);
        let specs_path = path.field(FieldName::Specs);
        // Copy only the admitted final list; measurement iterates borrowed nodes.
        let spec_nodes = self
            .sequence(specs_origin, specs_path, false)?
            .unwrap_or(&[]);
        let mut specs = if self.model {
            Vec::with_capacity(spec_nodes.len())
        } else {
            Vec::new()
        };
        for (index, &item) in spec_nodes.iter().enumerate() {
            let item_origin = Origin {
                container: specs_origin.node.unwrap(),
                key: None,
                node: Some(item),
                field: FieldName::Specs,
            };
            if let Some(value) = self.text(
                item_origin,
                specs_path.child(Step::Index(index)),
                TextRule::NonEmpty,
                None,
            )? {
                if self.model {
                    specs.push(value);
                }
            }
        }
        let define = self.required_text(node, FieldName::Define, path, TextRule::NonEmpty)?;
        let docs_origin = self.get(node, FieldName::Documents);
        let docs_path = path.field(FieldName::Documents);
        let doc_nodes = self.sequence(docs_origin, docs_path, true)?.unwrap_or(&[]);
        let mut documents = if self.model {
            Vec::with_capacity(doc_nodes.len())
        } else {
            Vec::new()
        };
        for (index, &item) in doc_nodes.iter().enumerate() {
            if let Some(value) =
                self.document(item, docs_origin, docs_path.child(Step::Index(index)))?
            {
                if self.model {
                    documents.push(value);
                }
            }
        }
        let dataset_json = self.boolean(
            self.get(node, FieldName::DatasetJson),
            path.field(FieldName::DatasetJson),
        )?;
        let context = self.choice(
            self.get(node, FieldName::Context),
            path.field(FieldName::Context),
            &[
                ("Submission", Context::Submission),
                ("Other", Context::Other),
            ],
            Some(Context::Submission),
        )?;
        let xml_lang = self.text(
            self.get(node, FieldName::XmlLang),
            path.field(FieldName::XmlLang),
            TextRule::Language,
            Some("en"),
        )?;
        let file_oid = self.optional_text(node, FieldName::FileOid, path, TextRule::DefineId)?;
        let metadata_origin = self.get(node, FieldName::MetadataVersion);
        let metadata_version = if metadata_origin.node.is_some() {
            self.metadata(metadata_origin, path.field(FieldName::MetadataVersion))?
        } else {
            None
        };
        let originator =
            self.optional_text(node, FieldName::Originator, path, TextRule::NonEmpty)?;
        let stylesheet = self.optional_text(node, FieldName::Stylesheet, path, TextRule::Href)?;
        let source_system_version = self.optional_text(
            node,
            FieldName::SourceSystemVersion,
            path,
            TextRule::NonEmpty,
        )?;
        Ok(standard
            .zip(define)
            .zip(dataset_json)
            .zip(context)
            .zip(xml_lang)
            .map(
                |((((standard, define), dataset_json), context), xml_lang)| Section {
                    family,
                    origin,
                    standard,
                    specs: Field {
                        value: specs,
                        origin: specs_origin,
                    },
                    define,
                    documents: Field {
                        value: documents,
                        origin: docs_origin,
                    },
                    dataset_json,
                    context,
                    xml_lang,
                    file_oid,
                    metadata_version,
                    originator,
                    stylesheet,
                    source_system_version,
                },
            ))
    }
    fn run(&mut self, root: usize) -> Result<Declarations, Cause> {
        let N::Mapping(fields) = &self.input.nodes()[root] else {
            unreachable!("preflight root")
        };
        let mut study = None;
        let count = fields.iter().filter(|&&(key, _)| matches!(&self.input.nodes()[key], N::Text(text) if family(text).is_some())).count();
        let mut sections = if self.model {
            Vec::with_capacity(count)
        } else {
            Vec::new()
        };
        for &(key, node) in fields {
            let N::Text(text) = &self.input.nodes()[key] else {
                self.fault_at(
                    key,
                    None,
                    Some((root, Some(key))),
                    Path::root(),
                    Kind::NonTextKey,
                )?;
                continue;
            };
            if text == "study" {
                let origin = Origin {
                    container: root,
                    key: Some(key),
                    node: Some(node),
                    field: FieldName::Study,
                };
                let value = self.study(origin, Path::root().field(FieldName::Study))?;
                if self.model {
                    study = value;
                }
            } else if let Some((family, field)) = family(text) {
                let origin = Origin {
                    container: root,
                    key: Some(key),
                    node: Some(node),
                    field,
                };
                let value = self.section(family, origin, Path::root().field(field))?;
                if self.model {
                    if let Some(value) = value {
                        sections.push(value);
                    }
                }
            }
        }
        Ok(Declarations {
            root,
            study,
            sections,
        })
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
