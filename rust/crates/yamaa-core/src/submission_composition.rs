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
    /// Relative publication-package reference already admitted by the file
    /// adapter; absent for a dataset with no data and no transport file.
    pub href: Option<&'a str>,
}
#[derive(Clone, Debug)]
pub struct Column {
    pub(crate) declaration: spec::Column,
    pub(crate) data_type: spec::DataType,
    /// The document's `Length`: declared, fixed by the family, or measured
    /// from the published values when a transport is requested (SUB-24).
    pub(crate) length: Option<String>,
    /// Declared, or fixed for an ADaM numeric date (SUB-25).
    pub(crate) display_format: Option<String>,
    pub(crate) mandatory: bool,
    pub(crate) source: Option<spec::OriginSource>,
    pub(crate) documents: Vec<spec::DocumentReference>,
    pub(crate) key_sequence: Option<usize>,
    pub(crate) values: Vec<ValueDefinition>,
    /// The width a transport file gives a character column (SUB-24).
    pub(crate) transport_length: Option<usize>,
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
    pub(crate) href: Option<String>,
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
    /// The implementation guide every represented dataset follows.
    pub(crate) guide: String,
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
    /// The transport file the selected section requests, if any.
    pub fn transport(&self) -> Option<env::Transport> {
        self.section.datasets.as_ref().map(|field| field.value)
    }
}
impl Dataset {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
    /// A dataset declared to have no data is never written as a transport file.
    pub fn has_no_data(&self) -> bool {
        self.metadata.has_no_data.value
    }
    /// A SUPP-- dataset's transport lengths are measured on its own (SUB-24).
    pub fn supplemental(&self) -> bool {
        self.id.starts_with("SUPP")
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
/// A dataset name `SASDatasetName` and a transport member carry:
/// `^[A-Z][A-Z0-9]{0,7}$` (DEFINE-22).
pub fn dataset_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 8
        && bytes.next().is_some_and(|b| b.is_ascii_uppercase())
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// A column name `SASFieldName` and a transport variable carry:
/// `^[A-Z][A-Z0-9_]{0,7}$` (DEFINE-22).
pub fn column_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 8
        && bytes.next().is_some_and(|b| b.is_ascii_uppercase())
        && bytes.all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
/// Printable ASCII, `0x20` through `0x7E`, the only text a transport carries.
pub fn printable_ascii(text: &str) -> bool {
    text.bytes().all(|b| (0x20..=0x7e).contains(&b))
}
/// Why a dataset or column label cannot travel in a transport file (XPT-5).
pub fn label_cause(label: &str) -> Option<&'static str> {
    if !printable_ascii(label) {
        Some("ascii")
    } else if label.len() > 40 {
        Some("length")
    } else {
        None
    }
}
/// A SAS display format a transport variable carries (XPT-9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SasFormat {
    pub name: String,
    pub width: u16,
    pub decimals: u16,
}
/// Parse `name`, optional width, `.`, optional decimals. The name is at most
/// eight upper-case letters, digits and underscores, begins with a letter or
/// underscore and ends with one, and is empty only when a width is written.
/// Width and decimals are decimal integers without a leading zero.
pub fn sas_format(text: &str) -> Option<SasFormat> {
    let (head, decimals) = text.split_once('.')?;
    let number = |digits: &str, zero: bool| -> Option<u16> {
        let valid = !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && (digits == "0" && zero || !digits.starts_with('0'));
        valid
            .then(|| digits.parse::<u16>().ok())
            .flatten()
            .filter(|value| *value <= 32_767)
    };
    let split = head.len() - head.bytes().rev().take_while(u8::is_ascii_digit).count();
    let (name, width) = head.split_at(split);
    let mut bytes = name.bytes();
    let name_valid = name.len() <= 8
        && bytes
            .next()
            .is_none_or(|b| b.is_ascii_uppercase() || b == b'_')
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    let width = if width.is_empty() {
        0
    } else {
        number(width, false)?
    };
    let decimals = if decimals.is_empty() {
        0
    } else {
        number(decimals, true)?
    };
    (name_valid && (!name.is_empty() || width > 0)).then(|| SasFormat {
        name: name.into(),
        width,
        decimals,
    })
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
    reads: &Reads,
) -> Option<origin::CopyFailure> {
    derivations
        .iter()
        .find_map(|&node| origin::copies(document, node, predecessor, reads).err())
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
const DATASET_NAME: &str = "^[A-Z][A-Z0-9]{0,7}$";
const COLUMN_NAME: &str = "^[A-Z][A-Z0-9_]{0,7}$";
/// Names and labels meet the transport limits under `Submission`, and under
/// `xpt` in every context (DEFINE-4, DEFINE-22, XPT-5).
fn transport_limits(section: &env::Section) -> bool {
    section.context.value == env::Context::Submission
        || section
            .datasets
            .as_ref()
            .is_some_and(|field| field.value == env::Transport::Xpt)
}
fn name_finding(path: &str, name: &str, pattern: &str) -> Finding {
    let mut finding = Finding::cause(ConditionCode::SubmissionSasName, path, name);
    for (key, value) in [("name", name), ("pattern", pattern)] {
        finding
            .context
            .insert(key.into(), crate::json::Value::String(value.into()));
    }
    finding
}
fn label_finding(
    path: &str,
    dataset: &str,
    column: Option<&str>,
    label: &str,
    cause: &str,
) -> Finding {
    let detail = if cause == "ascii" {
        "label is not printable ASCII"
    } else {
        "label is longer than 40 characters"
    };
    let mut finding = Finding::cause(ConditionCode::SubmissionLabelInvalid, path, detail);
    let mut facts = vec![("dataset", dataset)];
    facts.extend(column.map(|column| ("column", column)));
    facts.extend([("label", label), ("cause", cause)]);
    for (key, value) in facts {
        finding
            .context
            .insert(key.into(), crate::json::Value::String(value.into()));
    }
    finding
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
    /// The dataset name, reported with transport findings.
    dataset: &'a str,
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
        // A name and label a regulator's transport cannot carry fail under
        // Submission, and under xpt whatever the context (DEFINE-22, XPT-5).
        if level == Level::Column && transport_limits(section) {
            if !column_name(name) {
                findings.push(name_finding(column_path, name, COLUMN_NAME));
            }
            if let Some(label) = &column.label {
                if let Some(cause) = label_cause(&label.value) {
                    let label_path = column_path
                        .strip_suffix(".submission")
                        .map_or_else(|| format!("{column_path}.label"), |p| format!("{p}.label"));
                    findings.push(label_finding(
                        &label_path,
                        self.dataset,
                        Some(name),
                        &label.value,
                        cause,
                    ));
                }
            }
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
            .and_then(|predecessor| copy_failure(doc, derivations, predecessor, reads))
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
        // SUB-6 and SUB-25: the family decides how a date or datetime travels.
        let adam = section.family == Submission::Adam;
        let column_type = column.column_type.value;
        let data_type = metadata
            .data_type
            .as_ref()
            .map_or(column_type.family_data_type(adam), |d| d.value);
        if !column_type.admits_family_data_type(data_type, adam) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDataType,
                column_path,
                name,
            ));
        }
        // SUB-27: an `sdtm` or `send` dataset carries its date and time text
        // in a `str` column, never in a `datetime` one.
        if !adam && level == Level::Column && column_type == spec::ColumnType::Datetime {
            let type_path = column_path
                .strip_suffix(".submission")
                .map_or_else(|| format!("{column_path}.type"), |p| format!("{p}.type"));
            let mut finding =
                Finding::cause(ConditionCode::SubmissionDatetimeColumn, &type_path, name);
            finding
                .context
                .insert("column".into(), crate::json::Value::String(name.clone()));
            findings.push(finding);
        }
        let display_format = match column_type.numeric_date_format(adam) {
            Some(fixed) => {
                if metadata
                    .display_format
                    .as_ref()
                    .is_some_and(|declared| declared.value != fixed)
                {
                    let mut finding = Finding::cause(
                        ConditionCode::SubmissionDisplayFormatConflict,
                        &format!("{column_path}.display_format"),
                        fixed,
                    );
                    finding
                        .context
                        .insert("column".into(), crate::json::Value::String(name.clone()));
                    findings.push(finding);
                }
                Some(fixed.to_string())
            }
            None => metadata.display_format.as_ref().map(|d| d.value.clone()),
        };
        // XPT-9: a numeric variable's declared display format must be a SAS
        // format the transport can carry; only the column's own one is written.
        if level == Level::Column
            && section
                .datasets
                .as_ref()
                .is_some_and(|field| field.value == env::Transport::Xpt)
            && matches!(
                column_type,
                spec::ColumnType::Integer | spec::ColumnType::Float
            )
        {
            if let Some(declared) = &metadata.display_format {
                if sas_format(&declared.value).is_none() {
                    let mut finding = Finding::cause(
                        ConditionCode::XptDisplayFormatInvalid,
                        &format!("{column_path}.display_format"),
                        &declared.value,
                    );
                    for (key, value) in [
                        ("dataset", self.dataset),
                        ("column", name.as_str()),
                        ("display_format", declared.value.as_str()),
                    ] {
                        finding
                            .context
                            .insert(key.into(), crate::json::Value::String(value.into()));
                    }
                    findings.push(finding);
                }
            }
        }
        let length = self.admit_length(column, metadata, data_type, column_path, name, findings);
        self.admit_codelist(column, metadata, column_path, level, findings, bound);
        Some(Column {
            declaration: column.clone(),
            data_type,
            length,
            display_format,
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
            transport_length: None,
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
        // SUB-7: a date or datetime column never declares a length; under
        // `adam` it travels as an eight-byte SAS numeric date (SUB-25).
        let column_type = column.column_type.value;
        if column_type.temporal() {
            if metadata.length.is_some() {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionLengthNotApplicable,
                    column_path,
                    name,
                ));
            }
            if metadata.significant_digits.is_some() {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionLengthNotApplicable,
                    column_path,
                    "significant_digits",
                ));
            }
            return column_type
                .numeric_date_format(self.section.family == Submission::Adam)
                .map(|_| "8".into());
        }
        let length = metadata
            .length
            .as_ref()
            .map(|v| v.value.clone())
            .or_else(|| {
                if column_type == spec::ColumnType::String {
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
        // JSON-5: a transport re-reads the published artifact, so display digits
        // the csv profile writes would become the submitted values.
        if section.datasets.is_some() {
            if let Some(decimals) = doc
                .field(doc.root(), "output")
                .and_then(|output| doc.field(output, "decimals"))
            {
                let mut finding = Finding::cause(
                    ConditionCode::SubmissionTransportDecimals,
                    &format!("{path}.output.decimals"),
                    "display precision would become the submitted values",
                );
                finding
                    .context
                    .insert("dataset".into(), crate::json::Value::String(id.clone()));
                if let Some(written) = section.specs.value.get(index) {
                    finding.context.insert(
                        "specification".into(),
                        crate::json::Value::String(written.value.clone()),
                    );
                }
                if let N::Integer(value) = &doc.nodes()[decimals] {
                    if let Ok(number) = value.parse() {
                        finding
                            .context
                            .insert("decimals".into(), crate::json::Value::Number(number));
                    }
                }
                findings.push(finding);
            }
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
        if transport_limits(section) {
            if !dataset_name(&id) {
                findings.push(name_finding(&path, &id, DATASET_NAME));
            }
            if let Some(cause) = label_cause(&dataset.label.value) {
                findings.push(label_finding(
                    &format!("{path}.submission.label"),
                    &id,
                    None,
                    &dataset.label.value,
                    cause,
                ));
            }
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
            dataset: &id,
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
            href: input.href.map(String::from),
            discriminator,
        });
    }
    datasets
}

/// The declarations composition requires beyond the specifications a run uses.
#[derive(Clone, Copy, Debug)]
pub struct Required<'a> {
    pub study: &'a env::Study,
    pub section: &'a env::Section,
    pub standard: &'a env::ImplementationGuide,
    pub define: &'a env::Field<String>,
}
impl<'a> Required<'a> {
    /// A section admits for runs with only its `specs`. Composition requires the
    /// project `study`, the selected family's section and that section's
    /// `standard` and `define`, and reports every missing one together as a
    /// missing required field.
    pub fn admit(
        study: Option<&'a env::Study>,
        section: Option<&'a env::Section>,
        family: Submission,
    ) -> Result<Self, Vec<Finding>> {
        let family = family_name(family);
        let missing = |path: String, field: &str| {
            Finding::cause(ConditionCode::DefineRequiredDeclaration, &path, field)
        };
        let standard = section.and_then(|section| section.standard.as_ref());
        let define = section.and_then(|section| section.define.as_ref());
        if let (Some(study), Some(section), Some(standard), Some(define)) =
            (study, section, standard, define)
        {
            return Ok(Self {
                study,
                section,
                standard,
                define,
            });
        }
        let mut findings = Vec::new();
        if study.is_none() {
            findings.push(missing("study".into(), "study"));
        }
        if section.is_none() {
            findings.push(missing(family.into(), family));
        } else {
            if standard.is_none() {
                findings.push(missing(format!("{family}.standard"), "standard"));
            }
            if define.is_none() {
                findings.push(missing(format!("{family}.define"), "define"));
            }
        }
        Err(findings)
    }
}

/// Compose the selected section. `others` holds the specifications the study's
/// other submission sections list: the dataset inventory (CT-11) and the
/// predecessor columns (SUB-20) span every section, while only `inputs` are
/// represented in the document.
pub fn compose(
    required: Required<'_>,
    catalogue: &ct::Catalogue,
    inputs: &[Input<'_>],
    others: &[&Document],
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    let Required {
        study,
        section,
        standard: guide_declaration,
        define,
    } = required;
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
    let guide = guide_declaration.name.value.name();
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
    if !define.value.to_ascii_lowercase().ends_with(".xml") {
        fail(
            ConditionCode::SubmissionDefineExtension,
            "define",
            "expected .xml extension",
        );
    } else if section.datasets.is_some() && define.value.rsplit('/').next() != Some("define.xml") {
        // DEFINE-2: the transport files sit beside a document named define.xml.
        fail(
            ConditionCode::SubmissionDefineName,
            "define",
            "a transport requires the document file name define.xml",
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
        version: guide_declaration.version.value.clone(),
        status: match guide_declaration.status.value {
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
            // DEFINE-4: a sponsor-extended item has no NCI code, so only the
            // published items carry one (CT-6 refuses an alias on the others).
            if section.context.value == env::Context::Submission
                && source
                    .standard
                    .as_ref()
                    .is_some_and(|s| s.name == "CDISC/NCI")
                && list.items.as_ref().is_some_and(|items| {
                    list.alias.is_none() || items.iter().any(|i| !i.extended && i.alias.is_none())
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
        guide: guide.into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_names_are_upper_case_and_at_most_eight_characters() {
        for (name, dataset, column) in [
            ("DM", true, true),
            ("SUPPDM", true, true),
            ("ADSL", true, true),
            ("ETHNIC", true, true),
            ("LBORRES_", false, true),
            ("A1234567", true, true),
            ("ETHNICITY", false, false),
            ("dm", false, false),
            ("_DM", false, false),
            ("1DM", false, false),
            ("", false, false),
        ] {
            assert_eq!(dataset_name(name), dataset, "{name}");
            assert_eq!(column_name(name), column, "{name}");
        }
    }

    #[test]
    fn labels_are_printable_ascii_of_at_most_forty_characters() {
        assert_eq!(label_cause("Demographics"), None);
        assert_eq!(label_cause(&"L".repeat(40)), None);
        assert_eq!(label_cause(&"L".repeat(41)), Some("length"));
        assert_eq!(label_cause("Na\u{ef}ve"), Some("ascii"));
        assert_eq!(label_cause(&format!("{}\t", "L".repeat(45))), Some("ascii"));
    }

    #[test]
    fn sas_formats_have_a_name_an_optional_width_and_optional_decimals() {
        let format = |name: &str, width, decimals| {
            Some(SasFormat {
                name: name.into(),
                width,
                decimals,
            })
        };
        assert_eq!(sas_format("8.2"), format("", 8, 2));
        assert_eq!(sas_format("3."), format("", 3, 0));
        assert_eq!(sas_format("DATE9."), format("DATE", 9, 0));
        assert_eq!(sas_format("DATETIME20."), format("DATETIME", 20, 0));
        assert_eq!(sas_format("BEST."), format("BEST", 0, 0));
        assert_eq!(sas_format("E8601DA10."), format("E8601DA", 10, 0));
        assert_eq!(sas_format("COMMA10.0"), format("COMMA", 10, 0));
        assert_eq!(sas_format("_X5.1"), format("_X", 5, 1));
        for refused in [
            "",
            ".",
            ".2",
            "8",
            "8,2",
            "best12.",
            "DATE09.",
            "8.02",
            "TOOLONGNAME9.",
            "1A.",
            "8.2.1",
            "32768.",
            "8.-1",
        ] {
            assert_eq!(sas_format(refused), None, "{refused}");
        }
    }
}
