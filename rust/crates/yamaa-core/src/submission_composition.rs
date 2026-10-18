//! Shared submission composition over admitted metadata. No study or host ports.
use crate::{
    diagnostic::{ConditionCode, Definition},
    project_environment::Submission,
    project_terminology as ct,
    schema::{Document, DocumentNode as N},
    specification_submission_document as spec, submission_document as env,
    submission_origin::{self as origin, Reads},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub code: ConditionCode,
    pub path: String,
    pub context: crate::diagnostic::Context,
}
impl Finding {
    pub fn definition(&self) -> Definition {
        self.code.definition()
    }
    /// Retain the detector's typed cause and complete occurrence context.
    pub fn cause(code: ConditionCode, path: &str, detail: &str) -> Self {
        Self {
            code,
            path: path.into(),
            context: [("detail".into(), crate::json::Value::String(detail.into()))].into(),
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
    pub(crate) values: Vec<ValueDefinition>,
}
#[derive(Clone, Debug)]
pub struct ValueDefinition {
    pub(crate) test_code: String,
    pub(crate) path: String,
    pub(crate) column: Column,
}
#[derive(Clone, Debug)]
pub struct Dataset {
    pub(crate) id: String,
    pub(crate) domain: String,
    pub(crate) metadata: spec::Dataset,
    pub(crate) columns: Vec<Column>,
    pub(crate) href: String,
    pub(crate) artifact: String,
    pub(crate) discriminator: Option<usize>,
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
    /// Every dataset any submission section of the study declares (CT-11).
    pub(crate) inventory: BTreeSet<String>,
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
    pub fn values(&self) -> &[ValueDefinition] {
        &self.values
    }
}
impl ValueDefinition {
    pub fn test_code(&self) -> &str {
        &self.test_code
    }
    pub fn column(&self) -> &Column {
        &self.column
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
        Some(N::Sequence(values)) => texts(document, values),
        _ => Vec::new(),
    }
}
fn texts(document: &Document, values: &[usize]) -> Vec<String> {
    values
        .iter()
        .filter_map(|&n| match &document.nodes()[n] {
            N::Text(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}
fn verification(document: &Document, node: usize, operation: &str) -> Vec<usize> {
    let Some(v) = document.field(node, "verifications") else {
        return Vec::new();
    };
    let N::Sequence(nodes) = &document.nodes()[v] else {
        return Vec::new();
    };
    nodes
        .iter()
        .filter_map(|&node| document.field(node, operation))
        .collect()
}
fn reference_id(reference: &spec::DocumentReference) -> &str {
    match reference {
        spec::DocumentReference::Plain(field) => &field.value,
        spec::DocumentReference::Class(class) => &class.document.value,
    }
}
/// Whether a document reference says where in the document it points.
fn paged(reference: &spec::DocumentReference) -> bool {
    matches!(reference, spec::DocumentReference::Class(class) if class.pages.is_some())
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
            findings.push(Finding::cause(
                ConditionCode::SubmissionUnknownDocument,
                path,
                id,
            ));
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
/// SUB-15: the origin type and source pairs each family admits. `send` takes
/// any source the `sdtm` table admits for the type, or none.
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
            T::Predecessor | T::NotAvailable => source.is_none(),
        },
        Submission::Send => source.is_none() || origin_admits(Submission::Sdtm, kind, source),
        Submission::Adam => {
            source.is_none() && matches!(kind, T::Derived | T::Assigned | T::Predecessor | T::Other)
        }
    }
}
/// The first derivation that refutes the origin type under SUB-19, if any.
fn refuting(
    document: &Document,
    derivations: &[usize],
    kind: spec::OriginType,
    reads: &Reads,
) -> Option<origin::DerivationKind> {
    derivations
        .iter()
        .map(|&node| origin::classify(document, node, reads))
        .find(|derivation| !derivation.admits(kind))
}
/// SUB-20: the first derivation that is not the copy a Predecessor claims.
fn copy_failure(
    document: &Document,
    derivations: &[usize],
    predecessor: &origin::Predecessor<'_>,
) -> Option<origin::CopyFailure> {
    derivations
        .iter()
        .find_map(|&node| origin::copies(document, node, predecessor).err())
}
fn reserved_metadata(
    document: &Document,
    node: usize,
    governed: &[spec::FieldName],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    if let Some(N::Mapping(fields)) = document
        .field(node, "metadata")
        .and_then(|n| document.nodes().get(n))
    {
        for &(key, _) in fields {
            if let N::Text(key) = &document.nodes()[key] {
                if governed.iter().any(|field| field.name() == key) {
                    findings.push(Finding::cause(
                        ConditionCode::SubmissionReservedMetadata,
                        &format!("{path}.metadata.{key}"),
                        key,
                    ));
                }
            }
        }
    }
}
/// A redundant declaration: the path declares a fact `stated_by` already states.
fn redundant(code: ConditionCode, path: &str, detail: &str, stated_by: &str) -> Finding {
    let mut finding = Finding::cause(code, path, detail);
    finding.context.insert(
        "stated_by".into(),
        crate::json::Value::String(stated_by.into()),
    );
    finding
}
/// The paths of one column's or one root's verifications of an operation.
fn verification_paths(
    document: &Document,
    node: usize,
    operation: &str,
    prefix: &str,
) -> Vec<String> {
    let Some(N::Sequence(entries)) = document
        .field(node, "verifications")
        .map(|v| &document.nodes()[v])
    else {
        return Vec::new();
    };
    entries
        .iter()
        .enumerate()
        .filter(|&(_, &entry)| document.field(entry, operation).is_some())
        .map(|(index, _)| format!("{prefix}.verifications[{index}].{operation}"))
        .collect()
}
/// Whether a column's metadata is admitted as the column's own or as one test
/// code's value-level metadata, which governs only that code's rows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Column,
    Value,
}
/// Admit the complete inherited column metadata for a base or per-value item.
struct ColumnAdmission<'a> {
    doc: &'a Document,
    section: &'a env::Section,
    catalogue: &'a ct::Catalogue,
    known: &'a BTreeSet<&'a str>,
    acrfs: &'a [&'a env::DocumentReference],
    keys: &'a [String],
    reads: &'a Reads,
    /// The dataset's class decides whether `core` derives `mandatory` (SUB-10).
    tabulation: bool,
}
impl ColumnAdmission<'_> {
    /// SUB-10: the one resolution the domain build also enforces.
    fn mandatory(&self, name: &str, metadata: &spec::ColumnSubmission) -> bool {
        spec::resolve_mandatory(
            self.keys.iter().any(|key| key == name),
            metadata.mandatory.as_ref().map(|field| field.value),
            metadata.core.as_ref().map(|field| field.value),
            self.tabulation,
        )
    }
    fn admit(
        &self,
        column: &spec::Column,
        column_path: &str,
        derivations: &[usize],
        level: Level,
        findings: &mut Vec<Finding>,
        bound: &mut BTreeSet<String>,
    ) -> Option<Column> {
        let Self {
            doc,
            section,
            known,
            acrfs,
            keys,
            reads,
            ..
        } = *self;
        let refutation = match level {
            Level::Column => ConditionCode::SubmissionOriginRefuted,
            Level::Value => ConditionCode::SubmissionValueOriginRefuted,
        };
        let name = &column.name.value;
        let Some(metadata) = &column.submission else {
            findings.push(Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                column_path,
                name,
            ));
            return None;
        };
        let Some(origin) = &metadata.origin_metadata else {
            findings.push(Finding::cause(
                ConditionCode::SubmissionOriginMissing,
                column_path,
                name,
            ));
            return None;
        };
        if column.label.is_none() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                column_path,
                "label",
            ));
        }
        if section.context.value == env::Context::Submission && !sas_name(name) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionSasName,
                column_path,
                name,
            ));
        }
        let core = metadata.core.as_ref().map(|c| c.value);
        let mandatory = self.mandatory(name, metadata);
        if core.is_none()
            || (section.family == Submission::Adam
                && (core == Some(spec::Core::Expected) || metadata.mandatory.is_none()))
            || (section.family != Submission::Adam
                && (core == Some(spec::Core::Conditional)
                    || (core == Some(spec::Core::Required)
                        && metadata.mandatory.as_ref().is_some_and(|m| !m.value))))
        {
            findings.push(Finding::cause(
                ConditionCode::SubmissionCoreMandatory,
                column_path,
                name,
            ));
        }
        if section.family == Submission::Adam && metadata.role.is_some() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionAdamField,
                &format!("{column_path}.role"),
                name,
            ));
        }
        let source = origin.source.as_ref().map(|s| s.value);
        if !origin_admits(section.family, origin.kind.value, source) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionOriginSource,
                &format!("{column_path}.origin"),
                name,
            ));
        }
        let description = origin.description.as_ref().map(|d| d.value.as_str());
        let predecessor = description
            .filter(|_| origin.kind.value == spec::OriginType::Predecessor)
            .and_then(origin::predecessor);
        if matches!(
            origin.kind.value,
            spec::OriginType::Other
                | spec::OriginType::Predecessor
                | spec::OriginType::NotAvailable
        ) && description.is_none()
        {
            findings.push(Finding::cause(
                ConditionCode::SubmissionOriginDescription,
                &format!("{column_path}.origin"),
                name,
            ));
        } else if origin.kind.value == spec::OriginType::Predecessor && predecessor.is_none() {
            // SUB-16: a Predecessor description names `<dataset>.<column>`.
            let mut finding = Finding::cause(
                ConditionCode::SubmissionOriginDescription,
                &format!("{column_path}.origin.description"),
                name,
            );
            finding.context.insert(
                "reason".into(),
                crate::json::Value::String("predecessor_form".into()),
            );
            findings.push(finding);
        }
        if origin.kind.value == spec::OriginType::Derived && metadata.method.is_none() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionMethodMissing,
                column_path,
                name,
            ));
        }
        if refuting(doc, derivations, origin.kind.value, reads).is_some() {
            findings.push(Finding::cause(refutation, column_path, name));
        } else if let Some(failure) = predecessor
            .as_ref()
            .and_then(|predecessor| copy_failure(doc, derivations, predecessor))
        {
            // SUB-20: a Predecessor is a bare `source` of the column it names.
            let mut finding = Finding::cause(refutation, column_path, name);
            finding.context.insert(
                "reason".into(),
                crate::json::Value::String(failure.reason().into()),
            );
            findings.push(finding);
        }
        let mut refs = origin.documents.value.clone();
        if origin.kind.value == spec::OriginType::Collected
            && matches!(
                source,
                Some(spec::OriginSource::Investigator | spec::OriginSource::Subject)
            )
        {
            // SUB-17: under `Submission` the reference says where in the form
            // the value was collected; only `Other` derives a page-less one.
            let submission = section.context.value == env::Context::Submission;
            if acrfs.len() != 1 {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionAnnotatedCrfMissing,
                    &format!("{column_path}.origin"),
                    name,
                ));
            } else if origin.documents.origin.key.is_none() {
                if submission {
                    findings.push(Finding::cause(
                        ConditionCode::SubmissionOriginDocument,
                        &format!("{column_path}.origin"),
                        name,
                    ));
                } else {
                    refs.push(spec::DocumentReference::Plain(spec::Field {
                        value: acrfs[0].id.value.clone(),
                        origin: origin.documents.origin,
                    }));
                }
            } else if refs.is_empty()
                || refs.iter().any(|r| reference_id(r) != acrfs[0].id.value)
                || (submission && !refs.iter().any(paged))
            {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionOriginDocument,
                    &format!("{column_path}.origin.documents"),
                    name,
                ));
            }
        }
        references(
            findings,
            &refs,
            known,
            &format!("{column_path}.origin.documents"),
        );
        if let Some(spec::Method::Class(class)) = &metadata.method {
            references(
                findings,
                &class.documents.value,
                known,
                &format!("{column_path}.method"),
            );
        }
        comment_references(
            findings,
            metadata.comment.as_ref(),
            known,
            &format!("{column_path}.comment"),
        );
        let data_type = metadata
            .data_type
            .as_ref()
            .map_or(column.column_type.value.default_data_type(), |d| d.value);
        if !column.column_type.value.admits_data_type(data_type) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDataType,
                column_path,
                name,
            ));
        }
        let length = self.admit_length(column, metadata, data_type, column_path, name, findings);
        self.admit_codelist(column, metadata, column_path, level, findings, bound);
        Some(Column {
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
            values: Vec::new(),
        })
    }
    /// Admit length and significant-digit declarations; return the effective length.
    fn admit_length(
        &self,
        column: &spec::Column,
        metadata: &spec::ColumnSubmission,
        data_type: spec::DataType,
        column_path: &str,
        name: &str,
        findings: &mut Vec<Finding>,
    ) -> Option<String> {
        let maxima: Vec<_> = verification(self.doc, column.node, "max_length")
            .iter()
            .filter_map(|&n| self.doc.field(n, "max"))
            .filter_map(|n| match &self.doc.nodes()[n] {
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
            findings.push(Finding::cause(
                ConditionCode::SubmissionLengthMissing,
                column_path,
                name,
            ));
        }
        if !needs_length && metadata.length.is_some() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionLengthNotApplicable,
                column_path,
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
            findings.push(Finding::cause(
                ConditionCode::SubmissionLengthInvalid,
                column_path,
                name,
            ));
        }
        if metadata
            .length
            .as_ref()
            .is_some_and(|n| maxima.iter().any(|max| max != &n.value))
        {
            findings.push(Finding::cause(
                ConditionCode::SubmissionLengthConflict,
                column_path,
                name,
            ));
        }
        if (data_type == spec::DataType::Float) != metadata.significant_digits.is_some() {
            findings.push(Finding::cause(
                if data_type == spec::DataType::Float {
                    ConditionCode::SubmissionLengthMissing
                } else {
                    ConditionCode::SubmissionLengthNotApplicable
                },
                column_path,
                "significant_digits",
            ));
        }
        length
    }
    /// Bind an admitted codelist declaration and report any binding findings.
    /// A column-level list of items states the column's value set, so an
    /// `allowed_values` verification beside it is redundant (CT-9).
    fn admit_codelist(
        &self,
        column: &spec::Column,
        metadata: &spec::ColumnSubmission,
        column_path: &str,
        level: Level,
        findings: &mut Vec<Finding>,
        bound: &mut BTreeSet<String>,
    ) {
        if let Some(list) = &metadata.codelist {
            bound.insert(list.value.clone());
            let kind = match column.column_type.value {
                spec::ColumnType::String => crate::value::ColumnType::Str,
                spec::ColumnType::Integer => crate::value::ColumnType::Int,
                spec::ColumnType::Float => crate::value::ColumnType::Float,
                spec::ColumnType::Date => crate::value::ColumnType::Date,
                spec::ColumnType::Datetime => crate::value::ColumnType::DateTime,
            };
            let codelist_path = format!("{column_path}.codelist");
            let allowed = match level {
                Level::Column => verification_paths(
                    self.doc,
                    column.node,
                    "allowed_values",
                    column_path
                        .strip_suffix(".submission")
                        .unwrap_or(column_path),
                ),
                Level::Value => Vec::new(),
            };
            for binding in self.catalogue.bind(&list.value, kind, !allowed.is_empty()) {
                let code = match binding {
                    ct::BindingFinding::UnknownCodelist => ConditionCode::CodelistUnknownBinding,
                    ct::BindingFinding::TypeMismatch { .. } => {
                        ConditionCode::CodelistBindingTypeMismatch
                    }
                    ct::BindingFinding::RedundantAllowedValues => {
                        for path in &allowed {
                            findings.push(redundant(
                                ConditionCode::CodelistAllowedValuesRedundant,
                                path,
                                &column.name.value,
                                &codelist_path,
                            ));
                        }
                        continue;
                    }
                };
                findings.push(Finding::cause(code, &codelist_path, &list.value));
            }
        }
    }
}

/// SUB-10 and SUB-11 for one column at composition: a key column declares no
/// `mandatory: false`, and a `not_missing` verification beside a column that is
/// mandatory on every row restates the resolution and is redundant.
fn redundant_mandatory_checks(
    admission: &ColumnAdmission<'_>,
    declarations: &spec::Declarations,
    column: &spec::Column,
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let name = &column.name.value;
    let prefix = format!("{path}.columns.{name}");
    let position = admission.keys.iter().position(|key| key == name);
    let base = column.submission.as_ref();
    let declared = base.and_then(|metadata| metadata.mandatory.as_ref());
    if position.is_some() && declared.is_some_and(|mandatory| !mandatory.value) {
        findings.push(Finding::cause(
            ConditionCode::SubmissionKeyMandatory,
            &format!("{prefix}.submission.mandatory"),
            name,
        ));
    }
    // A value-level entry merges field by field over the column's metadata.
    let resolve = |entry: Option<&spec::ColumnSubmission>| {
        spec::resolve_mandatory(
            position.is_some(),
            entry
                .and_then(|metadata| metadata.mandatory.as_ref())
                .or(declared)
                .map(|field| field.value),
            entry
                .and_then(|metadata| metadata.core.as_ref())
                .or(base.and_then(|metadata| metadata.core.as_ref()))
                .map(|field| field.value),
            admission.tabulation,
        )
    };
    let everywhere = resolve(None)
        && declarations
            .value_rows()
            .iter()
            .flat_map(|row| &row.overrides)
            .filter(|entry| &entry.name.value == name)
            .all(|entry| resolve(Some(&entry.submission)));
    if !everywhere {
        return;
    }
    let (code, stated_by) = match position {
        Some(position) => (
            ConditionCode::VerificationNotMissingRedundant,
            format!("{path}.keys[{position}]"),
        ),
        None if declared.is_some() => (
            ConditionCode::SubmissionMandatoryRedundant,
            format!("{prefix}.submission.mandatory"),
        ),
        None => (
            ConditionCode::SubmissionMandatoryRedundant,
            format!("{prefix}.submission.core"),
        ),
    };
    for check in verification_paths(admission.doc, column.node, "not_missing", &prefix) {
        findings.push(redundant(code, &check, name, &stated_by));
    }
}

fn row_derivation(doc: &Document, row: usize, name: &str) -> Option<usize> {
    doc.field(row, "derivations")
        .and_then(|n| doc.field(n, name))
}
fn string_literal(doc: &Document, mut node: usize) -> Option<&String> {
    node = doc.field(node, "value").unwrap_or(node);
    let N::Mapping(fields) = &doc.nodes()[node] else {
        return None;
    };
    let [(key, value)] = fields.as_slice() else {
        return None;
    };
    match (&doc.nodes()[*key], &doc.nodes()[*value]) {
        (N::Text(handler), N::Text(value)) if handler == "literal" => Some(value),
        _ => None,
    }
}
fn column_derivations(
    doc: &Document,
    column: &spec::Column,
    overrides: &[spec::ValueRow],
    row: Option<usize>,
) -> Vec<usize> {
    if let Some(row) = row {
        return row_derivation(doc, row, &column.name.value)
            .or_else(|| doc.field(column.node, "derivation"))
            .into_iter()
            .collect();
    }
    let covered: BTreeSet<_> = overrides
        .iter()
        .flat_map(|row| {
            row.overrides
                .iter()
                .map(move |o| (row.node, o.name.value.as_str()))
        })
        .collect();
    let mut nodes: Vec<_> = doc.field(column.node, "derivation").into_iter().collect();
    if let Some(N::Sequence(rows)) = doc.field(doc.root(), "rows").map(|n| &doc.nodes()[n]) {
        for &row in rows {
            // Each covered row is checked against its merged metadata below.
            if covered.contains(&(row, column.name.value.as_str())) {
                continue;
            }
            if let Some(node) = row_derivation(doc, row, &column.name.value) {
                nodes.push(node);
            }
        }
    }
    nodes
}

fn compose_values(
    admission: &ColumnAdmission<'_>,
    declarations: &spec::Declarations,
    output_columns: &[String],
    selected: &mut [Column],
    path: &str,
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) -> Option<usize> {
    let doc = admission.doc;
    let columns = &declarations
        .columns()
        .expect("admitted dataset columns")
        .value;
    let domain = text(doc, doc.root(), "domain").unwrap_or_default();
    let name = format!("{domain}TESTCD");
    let discriminator = selected
        .iter()
        .position(|c| c.declaration.name.value == name);
    let mut seen = BTreeMap::<(&str, &str), &spec::ColumnSubmission>::new();
    for row in declarations.value_rows() {
        let row_path = format!("{path}.rows.{}.submission", row.id.value);
        if !columns.iter().any(|c| c.name.value == name) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionValueTestcdMissing,
                &row_path,
                &domain,
            ));
            continue;
        }
        let Some(test_code) =
            row_derivation(doc, row.node, &name).and_then(|n| string_literal(doc, n))
        else {
            findings.push(Finding::cause(
                ConditionCode::SubmissionValueTestcdDynamic,
                &row_path,
                &name,
            ));
            continue;
        };
        if discriminator.is_none() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                &row_path,
                "discriminator omitted from submission output",
            ));
        }
        for value in &row.overrides {
            let column_path = format!("{row_path}.{}", value.name.value);
            let Some(base) = columns.iter().find(|c| c.name.value == value.name.value) else {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionValueColumnUnknown,
                    &column_path,
                    &value.name.value,
                ));
                continue;
            };
            let derivations =
                column_derivations(doc, base, declarations.value_rows(), Some(row.node));
            if derivations.is_empty() {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionValueColumnUnknown,
                    &column_path,
                    &value.name.value,
                ));
                continue;
            }
            if !output_columns.contains(&value.name.value) {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionValueColumnNotOutput,
                    &column_path,
                    &value.name.value,
                ));
                continue;
            }
            let Some(column) = selected
                .iter_mut()
                .find(|c| c.declaration.name.value == value.name.value)
            else {
                continue;
            };
            let key = (value.name.value.as_str(), test_code.as_str());
            let duplicate = if let Some(first) = seen.get(&key) {
                if !first.same_values(&value.submission) {
                    findings.push(Finding::cause(
                        ConditionCode::SubmissionValueMetadataConflict,
                        &column_path,
                        test_code,
                    ));
                    continue;
                }
                true
            } else {
                seen.insert(key, &value.submission);
                false
            };
            let mut effective = base.clone();
            effective.submission = Some(
                value
                    .submission
                    .inherit(base.submission.as_ref().expect("selected column metadata")),
            );
            let first_finding = findings.len();
            if admission.keys.contains(&value.name.value)
                && value
                    .submission
                    .mandatory
                    .as_ref()
                    .is_some_and(|m| !m.value)
            {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionKeyMandatory,
                    &format!("{column_path}.mandatory"),
                    &value.name.value,
                ));
            }
            let column_value = admission.admit(
                &effective,
                &column_path,
                &derivations,
                Level::Value,
                findings,
                bound,
            );
            for finding in &mut findings[first_finding..] {
                for (key, value) in [
                    ("column", value.name.value.as_str()),
                    ("testcd", test_code.as_str()),
                    ("row", row.id.value.as_str()),
                ] {
                    finding
                        .context
                        .insert(key.into(), crate::json::Value::String(value.into()));
                }
                if finding.code == ConditionCode::SubmissionValueOriginRefuted {
                    let origin = effective
                        .submission
                        .as_ref()
                        .and_then(|m| m.origin_metadata.as_ref())
                        .expect("refuted admitted origin");
                    // A Predecessor that is not the named copy is refuted by a
                    // bare `source`, which SUB-19 alone admits.
                    let kind = refuting(doc, &derivations, origin.kind.value, admission.reads)
                        .unwrap_or_else(|| origin::classify(doc, derivations[0], admission.reads));
                    for (key, value) in [
                        ("derivation_kind", kind.name()),
                        ("origin_type", origin.kind.value.name()),
                    ] {
                        finding
                            .context
                            .insert(key.into(), crate::json::Value::String(value.into()));
                    }
                }
            }
            if let Some(column_value) = column_value {
                if !duplicate {
                    column.values.push(ValueDefinition {
                        test_code: test_code.clone(),
                        path: column_path,
                        column: column_value,
                    });
                }
            }
        }
    }
    discriminator
}

/// The label a study specification declares for one column of one dataset.
fn declared_label(document: &Document, dataset: &str, column: &str) -> Option<String> {
    if text(document, document.root(), "domain").as_deref() != Some(dataset) {
        return None;
    }
    match document
        .field(document.root(), "columns")
        .map(|n| &document.nodes()[n])
    {
        Some(N::Sequence(columns)) => columns
            .iter()
            .find(|&&node| text(document, node, "name").as_deref() == Some(column))
            .and_then(|&node| text(document, node, "label")),
        _ => None,
    }
}
/// SUB-20: a Predecessor column that keeps its predecessor's name keeps its
/// label, wherever in the study a specification declares that column.
fn predecessor_labels(
    study: &[&Document],
    column: &spec::Column,
    origins: &[&spec::OriginMetadata],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let Some(label) = &column.label else {
        return;
    };
    let mut checked = BTreeSet::new();
    for metadata in origins {
        if metadata.kind.value != spec::OriginType::Predecessor {
            continue;
        }
        let Some(predecessor) = metadata
            .description
            .as_ref()
            .and_then(|description| origin::predecessor(&description.value))
        else {
            continue;
        };
        if predecessor.column != column.name.value || !checked.insert(predecessor.name()) {
            continue;
        }
        for document in study {
            let Some(declared) = declared_label(document, predecessor.dataset, predecessor.column)
            else {
                continue;
            };
            if declared != label.value {
                let mut finding = Finding::cause(
                    ConditionCode::SubmissionPredecessorLabel,
                    &format!("{path}.columns.{}.label", column.name.value),
                    &column.name.value,
                );
                for (key, value) in [
                    ("predecessor", predecessor.name()),
                    ("label", label.value.clone()),
                    ("predecessor_label", declared),
                ] {
                    finding
                        .context
                        .insert(key.into(), crate::json::Value::String(value));
                }
                findings.push(finding);
                break;
            }
        }
    }
}

/// Compose every captured dataset in order, collecting codelist bindings.
/// `study` holds every specification any submission section lists.
#[allow(clippy::too_many_arguments)]
fn compose_datasets(
    section: &env::Section,
    catalogue: &ct::Catalogue,
    inputs: &[Input<'_>],
    study: &[&Document],
    known: &BTreeSet<&str>,
    acrfs: &[&env::DocumentReference],
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) -> Vec<Dataset> {
    let family = family_name(section.family);
    let mut datasets = Vec::new();
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for (index, input) in inputs.iter().enumerate() {
        let doc = input.document;
        let path = format!("{family}.specs[{index}]");
        reserved_metadata(
            doc,
            doc.root(),
            spec::DATASET_METADATA_FIELDS,
            &path,
            findings,
        );
        let reads = Reads::of(doc);
        let id = text(doc, doc.root(), "domain").unwrap_or_default();
        if !ids.insert(id.clone()) || known.contains(id.as_str()) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDuplicateIdentifier,
                &path,
                &id,
            ));
        }
        if !sources.insert(input.source) || !artifacts.insert(input.artifact) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDuplicateDataset,
                &path,
                &id,
            ));
        }
        let declarations = match spec::decode(doc, doc.root()) {
            Ok(value) => value,
            Err(_) => {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    &path,
                    "invalid closed submission declarations",
                ));
                continue;
            }
        };
        let Some(dataset) = declarations.dataset() else {
            findings.push(Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                &path,
                &id,
            ));
            continue;
        };
        let Some(columns) = declarations.columns() else {
            findings.push(Finding::cause(
                ConditionCode::SubmissionMetadataMissing,
                &path,
                "columns",
            ));
            continue;
        };
        if section.context.value == env::Context::Submission && !sas_name(&id) {
            findings.push(Finding::cause(ConditionCode::SubmissionSasName, &path, &id));
        }
        if dataset.reference_data.value && dataset.repeating.value {
            findings.push(Finding::cause(
                ConditionCode::SubmissionReferenceRepeating,
                &format!("{path}.submission"),
                &id,
            ));
        }
        if section.family == Submission::Adam && dataset.domain.is_some() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionAdamField,
                &format!("{path}.submission.domain"),
                &id,
            ));
        }
        // The domain build reads the family group from the class alone, so the
        // class must belong to the family this document binds the dataset to.
        let tabulation = dataset.class.value.tabulation();
        if tabulation == (section.family == Submission::Adam) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionClassFamily,
                &format!("{path}.submission.class"),
                dataset.class.value.name(),
            ));
        }
        if dataset.has_no_data.value && dataset.comment.is_none() {
            findings.push(Finding::cause(
                ConditionCode::SubmissionNoDataComment,
                &format!("{path}.submission.comment"),
                &id,
            ));
        }
        if dataset.has_no_data.value && section.dataset_json.value {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDatasetJsonNoData,
                &path,
                &id,
            ));
        }
        comment_references(
            findings,
            dataset.comment.as_ref(),
            known,
            &format!("{path}.submission.comment"),
        );
        let output = doc.field(doc.root(), "output");
        let output_columns = output.map_or_else(Vec::new, |out| strings(doc, out, "columns"));
        let keys = strings(doc, doc.root(), "keys");
        let mut names = BTreeSet::new();
        for column in &columns.value {
            if !names.insert(&column.name.value) {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionDuplicateIdentifier,
                    &format!("{path}.columns"),
                    &column.name.value,
                ));
            }
        }
        let mut ordered = BTreeSet::new();
        for name in &output_columns {
            if !ordered.insert(name) {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionDuplicateIdentifier,
                    &format!("{path}.output.columns"),
                    name,
                ));
            }
        }
        for key in &keys {
            if !output_columns.contains(key) {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionKeyNotOutput,
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
                spec::COLUMN_METADATA_FIELDS,
                &format!("{path}.columns.{}", column.name.value),
                findings,
            );
            if column.submission.is_some() && !output_columns.contains(&column.name.value) {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionColumnNotOutput,
                    &format!("{path}.columns.{}.submission", column.name.value),
                    &column.name.value,
                ));
            }
        }
        let admission = ColumnAdmission {
            doc,
            section,
            catalogue,
            known,
            acrfs,
            keys: &keys,
            reads: &reads,
            tabulation,
        };
        for name in &output_columns {
            let column_path = format!("{path}.columns.{name}.submission");
            let Some(column) = columns.value.iter().find(|c| &c.name.value == name) else {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    &column_path,
                    name,
                ));
                continue;
            };
            redundant_mandatory_checks(&admission, &declarations, column, &path, findings);
            let own = column
                .submission
                .as_ref()
                .and_then(|metadata| metadata.origin_metadata.as_ref());
            let origins: Vec<_> = own
                .into_iter()
                .chain(
                    declarations
                        .value_rows()
                        .iter()
                        .flat_map(|row| &row.overrides)
                        .filter(|entry| &entry.name.value == name)
                        .filter_map(|entry| entry.submission.origin_metadata.as_ref()),
                )
                .collect();
            predecessor_labels(study, column, &origins, &path, findings);
            let derivations = column_derivations(doc, column, declarations.value_rows(), None);
            if let Some(column) = admission.admit(
                column,
                &column_path,
                &derivations,
                Level::Column,
                findings,
                bound,
            ) {
                selected.push(column);
            }
        }
        // A `unique` over exactly the keys restates their asserted identity.
        let key_set: BTreeSet<_> = keys.iter().collect();
        if let Some(N::Sequence(entries)) = doc
            .field(doc.root(), "verifications")
            .map(|v| &doc.nodes()[v])
        {
            for (index, &entry) in entries.iter().enumerate() {
                let Some(payload) = doc.field(entry, "unique") else {
                    continue;
                };
                let columns = strings(doc, payload, "columns");
                if !columns.is_empty() && columns.iter().collect::<BTreeSet<_>>() == key_set {
                    findings.push(redundant(
                        ConditionCode::VerificationUniqueRedundant,
                        &format!("{path}.verifications[{index}].unique"),
                        &columns.join(", "),
                        &format!("{path}.keys"),
                    ));
                }
            }
        }
        let discriminator = compose_values(
            &admission,
            &declarations,
            &output_columns,
            &mut selected,
            &path,
            findings,
            bound,
        );
        datasets.push(Dataset {
            id: id.clone(),
            domain: dataset.domain.as_ref().map_or(id, |d| d.value.clone()),
            metadata: dataset.clone(),
            columns: selected,
            artifact: input.artifact.into(),
            href: input.href.into(),
            discriminator,
        });
    }
    datasets
}

/// Compose the selected section. `others` holds the specifications the study's
/// other submission sections list: the dataset inventory (CT-11) and the
/// predecessor columns (SUB-20) span every section, while only `inputs` are
/// represented in the document.
pub fn compose(
    study: &env::Study,
    section: &env::Section,
    catalogue: &ct::Catalogue,
    inputs: &[Input<'_>],
    others: &[&Document],
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    let mut findings = Vec::new();
    let specifications: Vec<&Document> = inputs
        .iter()
        .map(|input| input.document)
        .chain(others.iter().copied())
        .collect();
    let inventory = specifications
        .iter()
        .filter_map(|document| text(document, document.root(), "domain"))
        .collect();
    let family = family_name(section.family);
    let prefix = family.to_string();
    let mut fail = |code, suffix: &str, detail: &str| {
        findings.push(Finding::cause(code, &format!("{prefix}.{suffix}"), detail));
    };
    if inputs.len() != section.specs.value.len() {
        fail(
            ConditionCode::SubmissionMetadataMissing,
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
            ConditionCode::SubmissionStandardFamily,
            "standard.name",
            guide,
        );
    }
    if !section.define.value.to_ascii_lowercase().ends_with(".xml") {
        fail(
            ConditionCode::SubmissionDefineExtension,
            "define",
            "expected .xml extension",
        );
    }
    let source_version = match section.source_versions(installed) {
        Ok(v) => v.metadata().into(),
        Err(_) => {
            fail(
                ConditionCode::SubmissionSourceVersion,
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
                ConditionCode::SubmissionDuplicateIdentifier,
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
            ConditionCode::SubmissionAnnotatedCrfMultiplicity,
            "documents",
            "multiple annotated CRFs",
        );
    }
    let mut bound = BTreeSet::new();
    let datasets = compose_datasets(
        section,
        catalogue,
        inputs,
        &specifications,
        &known,
        &acrfs,
        &mut findings,
        &mut bound,
    );
    let mut standards = std::vec![Standard {
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
                findings.push(Finding::cause(
                    ConditionCode::SubmissionCdiscAliasMissing,
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
        inventory,
    })
}
