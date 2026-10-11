//! Shared submission composition over admitted metadata. No study or host ports.
use crate::{
    project_environment::Submission,
    project_terminology as ct,
    schema::{Document, DocumentNode as N},
    specification_submission_document as spec, submission_document as env,
};
use alloc::{
    collections::BTreeSet,
    format,
    string::{String, ToString},
    vec::Vec,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub definition: crate::diagnostic::Definition,
    pub path: String,
    pub context: crate::diagnostic::Context,
}
impl Finding {
    pub fn new(
        condition: &'static str,
        requirement: &'static str,
        path: &str,
        detail: &str,
    ) -> Self {
        Self {
            definition: crate::diagnostic::Definition {
                phase: "validation",
                condition,
                requirement: Some(requirement),
            },
            path: path.into(),
            context: [(
                "detail".into(),
                crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(detail.into())),
            )]
            .into(),
        }
    }
    /// Existing migrated causes keep their canonical phase and requirement.
    pub fn cause(code: crate::diagnostic::ConditionCode, path: &str, detail: &str) -> Self {
        Self {
            definition: code.definition(),
            path: path.into(),
            context: [(
                "detail".into(),
                crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(detail.into())),
            )]
            .into(),
        }
    }
}
pub struct Input<'a> {
    pub document: &'a Document,
    pub source: &'a str,
    pub artifact: &'a str,
    /// Relative publication-package reference already admitted by the file adapter.
    pub href: &'a str,
}
#[derive(Clone, Debug)]
pub struct Column {
    pub(crate) declaration: spec::Column,
    pub(crate) data_type: spec::DataType,
    pub(crate) length: Option<String>,
    pub(crate) mandatory: bool,
    pub(crate) source: Option<spec::OriginSource>,
    pub(crate) documents: Vec<spec::DocumentReference>,
    pub(crate) key_sequence: Option<usize>,
}
#[derive(Clone, Debug)]
pub struct Dataset {
    pub(crate) id: String,
    pub(crate) domain: String,
    pub(crate) metadata: spec::Dataset,
    pub(crate) columns: Vec<Column>,
    pub(crate) href: String,
    pub(crate) artifact: String,
}
#[derive(Clone, Debug)]
pub struct Codelist {
    pub declaration: ct::Codelist,
    pub standard: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Standard {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub publishing_set: Option<String>,
    pub version: String,
    pub status: &'static str,
}
/// Construction remains private to complete composition. Renderers cannot accept
/// an unadmitted declaration or fitting prefix of the selected dataset set.
#[derive(Debug)]
pub struct Study {
    pub(crate) study: env::Study,
    pub(crate) section: env::Section,
    pub(crate) file_oid: String,
    pub(crate) metadata_id: String,
    pub(crate) metadata_name: String,
    pub(crate) metadata_description: Option<String>,
    pub(crate) source_version: String,
    pub(crate) installed_version: String,
    pub(crate) created: String,
    pub(crate) standards: Vec<Standard>,
    pub(crate) datasets: Vec<Dataset>,
    pub(crate) codelists: Vec<Codelist>,
}
impl Study {
    pub fn installed_version(&self) -> &str {
        &self.installed_version
    }
    pub fn datasets(&self) -> &[Dataset] {
        &self.datasets
    }
    pub fn codelists(&self) -> &[Codelist] {
        &self.codelists
    }
}
impl Dataset {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
}
impl Column {
    pub fn declaration(&self) -> &spec::Column {
        &self.declaration
    }
    pub fn runtime_type(&self) -> crate::value::ColumnType {
        use crate::value::ColumnType as T;
        match self.declaration.column_type.value {
            spec::ColumnType::String => T::Str,
            spec::ColumnType::Integer => T::Int,
            spec::ColumnType::Float => T::Float,
            spec::ColumnType::Date => T::Date,
            spec::ColumnType::Datetime => T::DateTime,
        }
    }
}
pub fn family_name(family: Submission) -> &'static str {
    match family {
        Submission::Sdtm => "sdtm",
        Submission::Adam => "adam",
        Submission::Send => "send",
    }
}
pub fn sas_name(name: &str) -> bool {
    let mut chars = name.bytes();
    name.len() <= 8
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
pub fn text(document: &Document, node: usize, field: &str) -> Option<String> {
    match document.nodes().get(document.field(node, field)?)? {
        N::Text(value) => Some(value.clone()),
        _ => None,
    }
}
fn strings(document: &Document, node: usize, field: &str) -> Vec<String> {
    match document
        .field(node, field)
        .and_then(|n| document.nodes().get(n))
    {
        Some(N::Sequence(values)) => values
            .iter()
            .filter_map(|&n| match &document.nodes()[n] {
                N::Text(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
fn verification(document: &Document, node: usize, operation: &str) -> Vec<usize> {
    let Some(v) = document.field(node, "verifications") else {
        return Vec::new();
    };
    let nodes = match &document.nodes()[v] {
        N::Sequence(values) => values.as_slice(),
        _ => core::slice::from_ref(&v),
    };
    nodes
        .iter()
        .filter_map(|&node| document.field(node, operation))
        .collect()
}
fn default_type(kind: spec::ColumnType) -> spec::DataType {
    use spec::{ColumnType as C, DataType as D};
    match kind {
        C::String => D::Text,
        C::Integer => D::Integer,
        C::Float => D::Float,
        C::Date => D::Date,
        C::Datetime => D::Datetime,
    }
}
fn reference_id(reference: &spec::DocumentReference) -> &str {
    match reference {
        spec::DocumentReference::Plain(field) => &field.value,
        spec::DocumentReference::Class(class) => &class.document.value,
    }
}
fn references(
    findings: &mut Vec<Finding>,
    references: &[spec::DocumentReference],
    known: &BTreeSet<&str>,
    path: &str,
) {
    for reference in references {
        let id = reference_id(reference);
        if !known.contains(id) {
            findings.push(Finding::new("unknown_document", "REQ-1016", path, id));
        }
    }
}
fn comment_references(
    findings: &mut Vec<Finding>,
    comment: Option<&spec::Comment>,
    known: &BTreeSet<&str>,
    path: &str,
) {
    if let Some(spec::Comment::Class(class)) = comment {
        references(findings, &class.documents.value, known, path);
    }
}
fn origin_admits(
    family: Submission,
    kind: spec::OriginType,
    source: Option<spec::OriginSource>,
) -> bool {
    use spec::{OriginSource as S, OriginType as T};
    match family {
        Submission::Sdtm => match kind {
            T::Collected => matches!(source, Some(S::Subject | S::Investigator | S::Vendor)),
            T::Assigned | T::Derived => matches!(source, Some(S::Vendor | S::Sponsor)),
            T::Protocol => source == Some(S::Sponsor),
            T::Other => source.is_some(),
            T::Predecessor => source.is_none(),
            T::NotAvailable => false,
        },
        Submission::Send => source.is_none(),
        Submission::Adam => {
            source.is_none() && matches!(kind, T::Derived | T::Assigned | T::Predecessor | T::Other)
        }
    }
}
fn derivation_admits(
    document: &Document,
    mut node: usize,
    kind: spec::OriginType,
    intermediates: &BTreeSet<String>,
) -> bool {
    use spec::OriginType as T;
    if let Some(value) = document.field(node, "value") {
        node = value;
    }
    let literal = document.field(node, "literal").is_some();
    let source_node = document.field(node, "source").unwrap_or(node);
    let variable = match &document.nodes()[source_node] {
        N::Text(value) => Some(value.as_str()),
        _ => document
            .field(source_node, "variable")
            .and_then(|n| match &document.nodes()[n] {
                N::Text(value) => Some(value.as_str()),
                _ => None,
            }),
    };
    let intermediate = variable
        .and_then(|v| v.split_once('.'))
        .is_some_and(|(qualifier, _)| intermediates.contains(qualifier));
    let source = !intermediate && (variable.is_some() || document.field(node, "odm").is_some());
    if literal {
        matches!(kind, T::Assigned | T::Protocol | T::Other)
    } else if source {
        matches!(
            kind,
            T::Collected | T::Assigned | T::Protocol | T::Predecessor | T::Other | T::NotAvailable
        )
    } else {
        matches!(kind, T::Derived | T::Assigned | T::Other)
    }
}
fn reserved_metadata(
    document: &Document,
    node: usize,
    governed: &[&str],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    if let Some(N::Mapping(fields)) = document
        .field(node, "metadata")
        .and_then(|n| document.nodes().get(n))
    {
        for &(key, _) in fields {
            if let N::Text(key) = &document.nodes()[key] {
                if governed.contains(&key.as_str()) {
                    findings.push(Finding::new(
                        "reserved_metadata_key",
                        "REQ-0910",
                        &format!("{path}.metadata.{key}"),
                        key,
                    ));
                }
            }
        }
    }
}
/// Compose the complete bounded column-metadata profile. Value-level metadata
/// retains an explicit unsupported finding pending its own shared composition.
pub fn compose(
    study: &env::Study,
    section: &env::Section,
    catalogue: &ct::Catalogue,
    inputs: &[Input<'_>],
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    // Admit cumulative ownership and work before cloning any selected metadata.
    let mut nodes = 0usize;
    let mut text_bytes = 0usize;
    let mut work = 0usize;
    let mut allowed = inputs.len() <= 1024;
    for input in inputs {
        nodes = match nodes.checked_add(input.document.nodes().len()) {
            Some(n) => n,
            None => {
                allowed = false;
                break;
            }
        };
        let Some(mut input_text) = input
            .source
            .len()
            .checked_add(input.artifact.len())
            .and_then(|n| n.checked_add(input.href.len()))
        else {
            allowed = false;
            break;
        };
        for node in input.document.nodes() {
            if let N::Text(value) | N::Integer(value) = node {
                input_text = match input_text.checked_add(value.len()) {
                    Some(n) => n,
                    None => {
                        allowed = false;
                        break;
                    }
                };
            }
        }
        text_bytes = match text_bytes.checked_add(input_text) {
            Some(n) => n,
            None => {
                allowed = false;
                break;
            }
        };
        // Each selected column can scan the column/key lists and every row's
        // derivation mapping. Charge those complete scans, including textual
        // comparisons, rather than unrelated pairs of arena nodes. Constant
        // passes cover declaration/verification/reference and metadata scans.
        let output_columns = input
            .document
            .field(input.document.root(), "output")
            .and_then(|output| input.document.field(output, "columns"))
            .and_then(|columns| match &input.document.nodes()[columns] {
                N::Sequence(values) => Some(values.len()),
                _ => None,
            })
            .unwrap_or(0);
        work = match input
            .document
            .nodes()
            .len()
            .checked_mul(32)
            .and_then(|n| {
                input_text
                    .checked_mul(8)
                    .and_then(|text| n.checked_add(text))
            })
            .and_then(|n| {
                output_columns
                    .checked_add(1)
                    .and_then(|count| n.checked_mul(count))
            })
            .and_then(|n| work.checked_add(n))
        {
            Some(n) => n,
            None => {
                allowed = false;
                break;
            }
        };
    }
    if !allowed || nodes > 524_288 || text_bytes > 16_777_216 || work > 268_435_456 {
        return Err(alloc::vec![Finding::cause(
            crate::diagnostic::ConditionCode::ApplicationRejected,
            "specs",
            "composition_admission_limit"
        )]);
    }
    let mut findings = Vec::new();
    let family = family_name(section.family);
    let prefix = family.to_string();
    let mut fail = |condition, requirement, suffix: &str, detail: &str| {
        findings.push(Finding::new(
            condition,
            requirement,
            &format!("{prefix}.{suffix}"),
            detail,
        ));
    };
    if inputs.len() != section.specs.value.len() {
        fail(
            "submission_metadata_missing",
            "REQ-1015",
            "specs",
            "incomplete captured dataset set",
        );
    }
    let guide = section.standard.name.value.name();
    let compatible = match section.family {
        Submission::Sdtm => guide.starts_with("SDTMIG"),
        Submission::Adam => guide.starts_with("ADaMIG"),
        Submission::Send => guide.starts_with("SENDIG"),
    };
    if !compatible {
        fail(
            "unknown_standard_family",
            "REQ-0924",
            "standard.name",
            guide,
        );
    }
    if !section.define.value.to_ascii_lowercase().ends_with(".xml") {
        fail(
            "value_not_permitted",
            "REQ-1014",
            "define",
            "expected .xml extension",
        );
    }
    let source_version = match section.source_versions(installed) {
        Ok(v) => v.metadata().into(),
        Err(_) => {
            fail(
                "source_system_incomplete",
                "REQ-0979",
                "source_system_version",
                "empty generator version",
            );
            String::new()
        }
    };
    let mut known = BTreeSet::new();
    for document in &section.documents.value {
        if !known.insert(document.id.value.as_str()) {
            fail(
                "duplicate_define_identifier",
                "REQ-1017",
                "documents",
                &document.id.value,
            );
        }
    }
    let acrfs: Vec<_> = section
        .documents
        .value
        .iter()
        .filter(|d| d.kind.value == env::DocumentKind::AnnotatedCrf)
        .collect();
    if acrfs.len() > 1 {
        fail(
            "value_not_permitted",
            "REQ-1019",
            "documents",
            "multiple annotated CRFs",
        );
    }
    let mut datasets = Vec::new();
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    let mut bound = BTreeSet::new();
    for (index, input) in inputs.iter().enumerate() {
        let doc = input.document;
        let path = format!("{family}.specs[{index}]");
        reserved_metadata(
            doc,
            doc.root(),
            &[
                "label",
                "class",
                "subclass",
                "structure",
                "repeating",
                "reference_data",
                "has_no_data",
                "domain",
                "comment",
            ],
            &path,
            &mut findings,
        );
        let intermediates: BTreeSet<_> = doc
            .field(doc.root(), "intermediates")
            .and_then(|n| match &doc.nodes()[n] {
                N::Sequence(nodes) => Some(nodes),
                _ => None,
            })
            .into_iter()
            .flatten()
            .filter_map(|&n| text(doc, n, "id"))
            .collect();
        let id = text(doc, doc.root(), "domain").unwrap_or_default();
        if !ids.insert(id.clone()) || known.contains(id.as_str()) {
            findings.push(Finding::new(
                "duplicate_define_identifier",
                "REQ-1017",
                &path,
                &id,
            ));
        }
        if !sources.insert(input.source) || !artifacts.insert(input.artifact) {
            findings.push(Finding::new(
                "duplicate_dataset_entry",
                "REQ-1018",
                &path,
                &id,
            ));
        }
        let declarations = match spec::decode(doc, doc.root(), Default::default()) {
            Ok(value) => value,
            Err(_) => {
                findings.push(Finding::new(
                    "submission_metadata_missing",
                    "REQ-1015",
                    &path,
                    "invalid closed submission declarations",
                ));
                continue;
            }
        };
        let Some(dataset) = declarations.dataset() else {
            findings.push(Finding::new(
                "submission_metadata_missing",
                "REQ-1015",
                &path,
                &id,
            ));
            continue;
        };
        let Some(columns) = declarations.columns() else {
            findings.push(Finding::new(
                "submission_metadata_missing",
                "REQ-1015",
                &path,
                "columns",
            ));
            continue;
        };
        if section.context.value == env::Context::Submission && !sas_name(&id) {
            findings.push(Finding::new("sas_name_too_long", "REQ-1023", &path, &id));
        }
        if dataset.reference_data.value && dataset.repeating.value {
            findings.push(Finding::new(
                "reference_data_repeating_conflict",
                "REQ-0925",
                &format!("{path}.submission"),
                &id,
            ));
        }
        if section.family == Submission::Adam && dataset.domain.is_some() {
            findings.push(Finding::new(
                "value_not_permitted",
                "REQ-0917",
                &format!("{path}.submission.domain"),
                &id,
            ));
        }
        if dataset.has_no_data.value && dataset.comment.is_none() {
            findings.push(Finding::new(
                "submission_metadata_missing",
                "REQ-1022",
                &format!("{path}.submission.comment"),
                &id,
            ));
        }
        if dataset.has_no_data.value && section.dataset_json.value {
            findings.push(Finding::new(
                "dataset_json_without_data",
                "REQ-1227",
                &path,
                &id,
            ));
        }
        comment_references(
            &mut findings,
            dataset.comment.as_ref(),
            &known,
            &format!("{path}.submission.comment"),
        );
        let output = doc.field(doc.root(), "output");
        let output_columns = output.map_or_else(Vec::new, |out| strings(doc, out, "columns"));
        let keys = strings(doc, doc.root(), "keys");
        let mut names = BTreeSet::new();
        for column in &columns.value {
            if !names.insert(&column.name.value) {
                findings.push(Finding::new(
                    "duplicate_define_identifier",
                    "REQ-1017",
                    &format!("{path}.columns"),
                    &column.name.value,
                ));
            }
        }
        let mut ordered = BTreeSet::new();
        for name in &output_columns {
            if !ordered.insert(name) {
                findings.push(Finding::new(
                    "duplicate_define_identifier",
                    "REQ-1017",
                    &format!("{path}.output.columns"),
                    name,
                ));
            }
        }
        for key in &keys {
            if !output_columns.contains(key) {
                findings.push(Finding::new(
                    "value_not_permitted",
                    "REQ-1015",
                    &format!("{path}.keys"),
                    "key omitted from submission output",
                ));
            }
        }
        let mut selected = Vec::new();
        for column in &columns.value {
            reserved_metadata(
                doc,
                column.node,
                &[
                    "core",
                    "mandatory",
                    "role",
                    "data_type",
                    "length",
                    "significant_digits",
                    "display_format",
                    "codelist",
                    "inventory_vocabulary",
                    "origin",
                    "method",
                    "comment",
                ],
                &format!("{path}.columns.{}", column.name.value),
                &mut findings,
            );
            if column.submission.is_some() && !output_columns.contains(&column.name.value) {
                findings.push(Finding::new(
                    "submission_not_output_column",
                    "REQ-0909",
                    &format!("{path}.columns.{}.submission", column.name.value),
                    &column.name.value,
                ));
            }
        }
        for name in &output_columns {
            let column_path = format!("{path}.columns.{name}.submission");
            let Some(column) = columns.value.iter().find(|c| &c.name.value == name) else {
                findings.push(Finding::new(
                    "submission_metadata_missing",
                    "REQ-1015",
                    &column_path,
                    name,
                ));
                continue;
            };
            let Some(metadata) = &column.submission else {
                findings.push(Finding::new(
                    "submission_metadata_missing",
                    "REQ-1015",
                    &column_path,
                    name,
                ));
                continue;
            };
            let Some(origin) = &metadata.origin_metadata else {
                findings.push(Finding::new(
                    "origin_missing",
                    "REQ-0918",
                    &column_path,
                    name,
                ));
                continue;
            };
            if column.label.is_none() {
                findings.push(Finding::new(
                    "submission_metadata_missing",
                    "REQ-1015",
                    &column_path,
                    "label",
                ));
            }
            if section.context.value == env::Context::Submission && !sas_name(name) {
                findings.push(Finding::new(
                    "sas_name_too_long",
                    "REQ-1023",
                    &column_path,
                    name,
                ));
            }
            let core = metadata.core.as_ref().map(|c| c.value);
            let mandatory = metadata
                .mandatory
                .as_ref()
                .map_or(core == Some(spec::Core::Required), |c| c.value);
            if core.is_none()
                || (section.family == Submission::Adam
                    && (core == Some(spec::Core::Expected) || metadata.mandatory.is_none()))
                || (section.family != Submission::Adam
                    && (core == Some(spec::Core::Conditional)
                        || (core == Some(spec::Core::Required) && !mandatory)))
            {
                findings.push(Finding::new(
                    "core_mandatory_conflict",
                    "REQ-0915",
                    &column_path,
                    name,
                ));
            }
            if mandatory
                && !keys.contains(name)
                && verification(doc, column.node, "not_missing").is_empty()
            {
                findings.push(Finding::new(
                    "mandatory_not_enforced",
                    "REQ-0916",
                    &column_path,
                    name,
                ));
            }
            if section.family == Submission::Adam && metadata.role.is_some() {
                findings.push(Finding::new(
                    "value_not_permitted",
                    "REQ-0917",
                    &format!("{column_path}.role"),
                    name,
                ));
            }
            let source = origin.source.as_ref().map(|s| s.value);
            if !origin_admits(section.family, origin.kind.value, source) {
                findings.push(Finding::new(
                    "origin_source_not_admitted",
                    "REQ-0919",
                    &format!("{column_path}.origin"),
                    name,
                ));
            }
            if matches!(
                origin.kind.value,
                spec::OriginType::Other
                    | spec::OriginType::Predecessor
                    | spec::OriginType::NotAvailable
            ) && origin.description.is_none()
            {
                findings.push(Finding::new(
                    "origin_description_missing",
                    "REQ-0920",
                    &format!("{column_path}.origin"),
                    name,
                ));
            }
            if origin.kind.value == spec::OriginType::Derived && metadata.method.is_none() {
                findings.push(Finding::new(
                    "method_missing",
                    "REQ-0923",
                    &column_path,
                    name,
                ));
            }
            let mut derivations: Vec<_> =
                doc.field(column.node, "derivation").into_iter().collect();
            if let Some(N::Sequence(rows)) = doc
                .field(doc.root(), "rows")
                .and_then(|n| doc.nodes().get(n))
            {
                for &row in rows {
                    if let Some(node) = doc
                        .field(row, "derivations")
                        .and_then(|n| doc.field(n, name))
                    {
                        derivations.push(node);
                    }
                }
            }
            if derivations
                .iter()
                .any(|&node| !derivation_admits(doc, node, origin.kind.value, &intermediates))
            {
                findings.push(Finding::new(
                    "origin_contradicts_derivation",
                    "REQ-0922",
                    &column_path,
                    name,
                ));
            }
            let mut refs = origin.documents.value.clone();
            if origin.kind.value == spec::OriginType::Collected
                && matches!(
                    source,
                    Some(spec::OriginSource::Investigator | spec::OriginSource::Subject)
                )
            {
                if acrfs.len() != 1 {
                    findings.push(Finding::new(
                        "origin_document_missing",
                        "REQ-0984",
                        &format!("{column_path}.origin"),
                        name,
                    ));
                } else if origin.documents.origin.key.is_none() {
                    refs.push(spec::DocumentReference::Plain(spec::Field {
                        value: acrfs[0].id.value.clone(),
                        origin: origin.documents.origin,
                    }));
                } else if refs.is_empty()
                    || refs.iter().any(|r| reference_id(r) != acrfs[0].id.value)
                {
                    findings.push(Finding::new(
                        "origin_document_missing",
                        "REQ-0893",
                        &format!("{column_path}.origin.documents"),
                        name,
                    ));
                }
            }
            references(
                &mut findings,
                &refs,
                &known,
                &format!("{column_path}.origin.documents"),
            );
            if let Some(spec::Method::Class(class)) = &metadata.method {
                references(
                    &mut findings,
                    &class.documents.value,
                    &known,
                    &format!("{column_path}.method"),
                );
            }
            comment_references(
                &mut findings,
                metadata.comment.as_ref(),
                &known,
                &format!("{column_path}.comment"),
            );
            let data_type = metadata
                .data_type
                .as_ref()
                .map_or(default_type(column.column_type.value), |d| d.value);
            if column.column_type.value == spec::ColumnType::String
                && matches!(data_type, spec::DataType::Integer | spec::DataType::Float)
                || column.column_type.value != spec::ColumnType::String
                    && data_type != default_type(column.column_type.value)
            {
                findings.push(Finding::new(
                    "submission_data_type_not_admitted",
                    "REQ-0914",
                    &column_path,
                    name,
                ));
            }
            let maxima: Vec<_> = verification(doc, column.node, "max_length")
                .iter()
                .filter_map(|&n| doc.field(n, "max"))
                .filter_map(|n| match &doc.nodes()[n] {
                    N::Integer(value) => Some(value.clone()),
                    _ => None,
                })
                .collect();
            let length = metadata
                .length
                .as_ref()
                .map(|v| v.value.clone())
                .or_else(|| {
                    if column.column_type.value == spec::ColumnType::String {
                        maxima.first().cloned()
                    } else {
                        None
                    }
                });
            let needs_length = matches!(
                data_type,
                spec::DataType::Text | spec::DataType::Integer | spec::DataType::Float
            );
            if needs_length && length.is_none() {
                findings.push(Finding::new(
                    "submission_length_missing",
                    "REQ-0912",
                    &column_path,
                    name,
                ));
            }
            if !needs_length && metadata.length.is_some() {
                findings.push(Finding::new(
                    "submission_length_not_applicable",
                    "REQ-0912",
                    &column_path,
                    name,
                ));
            }
            if length
                .as_ref()
                .is_some_and(|s| s.starts_with('-') || s == "0")
                || metadata
                    .significant_digits
                    .as_ref()
                    .is_some_and(|v| v.value.starts_with('-'))
            {
                findings.push(Finding::new(
                    "submission_length_invalid",
                    "REQ-0911",
                    &column_path,
                    name,
                ));
            }
            if metadata
                .length
                .as_ref()
                .is_some_and(|n| maxima.iter().any(|max| max != &n.value))
            {
                findings.push(Finding::new(
                    "declared_length_conflict",
                    "REQ-0913",
                    &column_path,
                    name,
                ));
            }
            if (data_type == spec::DataType::Float) != metadata.significant_digits.is_some() {
                findings.push(Finding::new(
                    if data_type == spec::DataType::Float {
                        "submission_length_missing"
                    } else {
                        "submission_length_not_applicable"
                    },
                    "REQ-0912",
                    &column_path,
                    "significant_digits",
                ));
            }
            if let Some(list) = &metadata.codelist {
                bound.insert(list.value.clone());
                let kind = match column.column_type.value {
                    spec::ColumnType::String => crate::value::ColumnType::Str,
                    spec::ColumnType::Integer => crate::value::ColumnType::Int,
                    spec::ColumnType::Float => crate::value::ColumnType::Float,
                    spec::ColumnType::Date => crate::value::ColumnType::Date,
                    spec::ColumnType::Datetime => crate::value::ColumnType::DateTime,
                };
                let allowed = verification(doc, column.node, "allowed_values");
                let mut reader = crate::project_function_document::Reader::new(doc);
                let allowed: Vec<Vec<_>> = allowed
                    .iter()
                    .filter_map(|&n| doc.field(n, "values"))
                    .filter_map(|n| match &doc.nodes()[n] {
                        N::Sequence(values) => Some(
                            values
                                .iter()
                                .map(|&v| reader.scalar(v, &column_path))
                                .collect(),
                        ),
                        _ => None,
                    })
                    .collect();
                if !reader.finish().is_empty() {
                    findings.push(Finding::new(
                        "value_not_permitted",
                        "REQ-0955",
                        &column_path,
                        "allowed_values literal admission",
                    ));
                }
                let bindings = if allowed.is_empty() {
                    catalogue.bind(&list.value, kind, None)
                } else {
                    allowed
                        .iter()
                        .flat_map(|values| catalogue.bind(&list.value, kind, Some(values)))
                        .collect()
                };
                for binding in bindings {
                    let (condition, requirement) = match binding {
                        ct::BindingFinding::UnknownCodelist => ("unknown_codelist", "REQ-0953"),
                        ct::BindingFinding::TypeMismatch { .. } => {
                            ("codelist_type_mismatch", "REQ-0954")
                        }
                        ct::BindingFinding::ValuesConflict => {
                            ("codelist_values_conflict", "REQ-0955")
                        }
                        ct::BindingFinding::Limit(_) => ("application_rejected", "REQ-0955"),
                    };
                    findings.push(Finding::new(
                        condition,
                        requirement,
                        &format!("{column_path}.codelist"),
                        &list.value,
                    ));
                }
            }
            selected.push(Column {
                declaration: column.clone(),
                data_type,
                length,
                mandatory,
                source: if section.family == Submission::Adam
                    && origin.kind.value != spec::OriginType::Predecessor
                {
                    Some(spec::OriginSource::Sponsor)
                } else {
                    source
                },
                documents: refs,
                key_sequence: keys.iter().position(|key| key == name).map(|k| k + 1),
            });
        }
        if let Some(node) = doc.field(doc.root(), "rows") {
            if let N::Sequence(rows) = &doc.nodes()[node] {
                for &row in rows {
                    if doc.field(row, "submission").is_some_and(|n| {
                        !matches!(&doc.nodes()[n], N::Null)
                            && !matches!(&doc.nodes()[n], N::Mapping(fields) if fields.is_empty())
                    }) {
                        findings.push(Finding::cause(
                            crate::diagnostic::ConditionCode::ApplicationUnsupported,
                            &format!("{path}.rows"),
                            "value_level_submission",
                        ));
                    }
                }
            }
        }
        datasets.push(Dataset {
            id: id.clone(),
            domain: dataset.domain.as_ref().map_or(id, |d| d.value.clone()),
            metadata: dataset.clone(),
            columns: selected,
            artifact: input.artifact.into(),
            href: input.href.into(),
        });
    }
    let mut standards = alloc::vec![Standard {
        id: guide.into(),
        name: guide.into(),
        kind: "IG",
        publishing_set: None,
        version: section.standard.version.value.clone(),
        status: match section.standard.status.value {
            env::GuideStatus::Draft => "Draft",
            env::GuideStatus::Final => "Final",
        }
    }];
    let mut codelists = Vec::new();
    for source in catalogue.sources() {
        if !source.codelists.iter().any(|c| bound.contains(&c.id)) {
            continue;
        }
        let standard = source.standard.as_ref().map(|ct| {
            let candidate = format!("CT_{}", ct.publishing_set);
            if let Some(prior) = standards.iter().find(|s| {
                s.kind == "CT"
                    && s.publishing_set.as_ref() == Some(&ct.publishing_set)
                    && s.name == ct.name
                    && s.version == ct.version
            }) {
                return prior.id.clone();
            }
            let id = if standards.iter().any(|s| s.id == candidate) {
                format!("{candidate}_{}", standards.len())
            } else {
                candidate
            };
            standards.push(Standard {
                id: id.clone(),
                name: ct.name.clone(),
                kind: "CT",
                publishing_set: Some(ct.publishing_set.clone()),
                version: ct.version.clone(),
                status: "Final",
            });
            id
        });
        for list in &source.codelists {
            if !bound.contains(&list.id) {
                continue;
            }
            if section.context.value == env::Context::Submission
                && source
                    .standard
                    .as_ref()
                    .is_some_and(|s| s.name == "CDISC/NCI")
                && list.items.as_ref().is_some_and(|items| {
                    list.alias.is_none() || items.iter().any(|i| i.alias.is_none())
                })
            {
                findings.push(Finding::new(
                    "missing_required_field",
                    "REQ-1004",
                    &format!("codelists.{}", list.id),
                    "alias",
                ));
            }
            codelists.push(Codelist {
                declaration: list.clone(),
                standard: standard.clone(),
            });
        }
    }
    if !findings.is_empty() {
        return Err(findings);
    }
    let metadata_id = section.metadata_version.as_ref().map_or_else(
        || format!("{}.{}", study.id.value, family),
        |m| m.id.value.clone(),
    );
    let metadata_name = section.metadata_version.as_ref().map_or_else(
        || format!("{} {} metadata", study.name.value, family),
        |m| m.name.value.clone(),
    );
    Ok(Study {
        study: study.clone(),
        section: section.clone(),
        file_oid: section.file_oid.as_ref().map_or_else(
            || format!("{}.{}.Define-XML_2.1", study.id.value, family),
            |f| f.value.clone(),
        ),
        metadata_id,
        metadata_name,
        metadata_description: section
            .metadata_version
            .as_ref()
            .and_then(|m| m.description.as_ref())
            .map(|f| f.value.clone()),
        source_version,
        installed_version: installed.into(),
        created: created.into(),
        standards,
        datasets,
        codelists,
    })
}
