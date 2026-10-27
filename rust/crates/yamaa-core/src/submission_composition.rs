//! Shared submission composition over admitted metadata. No study or host ports.
use crate::{
    application_issue as issue, ast,
    cdisc_vocabulary::{Core, DataType, DefineContext, ModelKind, OriginSource, OriginType},
    diagnostic::{ConditionCode, Definition},
    project_environment::Submission,
    project_terminology as ct,
    schema::{Document, DocumentNode as N},
    specification::{OutputFinding, PreflightFinding},
    specification_submission_document as spec,
    submission_origin::{self as origin, Reads},
    submission_values as values,
};
use std::collections::BTreeSet;
mod historical;

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
    /// An engine fault at `path`, naming its step and closed code (D73).
    pub fn fault(path: &str, step: issue::Step, fault: issue::Fault) -> Self {
        Self::refused(path, issue::rejected(step, fault))
    }
    /// The bound an engine pass hit at `path`, with its resource facts.
    pub fn limit(
        path: &str,
        step: issue::Step,
        fault: issue::Fault,
        bound: crate::limits::LimitExceeded,
    ) -> Self {
        Self::refused(
            path,
            issue::resource_rejected(
                step,
                fault,
                bound.resource,
                bound.limit,
                bound.actual,
                Vec::new(),
            ),
        )
    }
    /// An admitted operation the implementation does not support at `path`,
    /// naming the operation as every such refusal does (COLUMN-15).
    pub fn unsupported(path: &str, operation: &str) -> Self {
        Self::refused(path, issue::unsupported(operation, None))
    }
    /// A listed specification's own refusal, with its condition and context,
    /// at its spec path under `listing` (`sdtm.specs[0]`).
    fn listed(listing: &str, refusal: crate::diagnostic::Diagnostic) -> Self {
        let path = match refusal.spec_paths.first() {
            Some(path) => format!("{listing}.{path}"),
            None => listing.into(),
        };
        Self {
            code: refusal.code,
            path,
            context: refusal.context,
        }
    }
    /// Retain display detail without using it to classify the failure.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.context
            .insert("detail".into(), crate::json::Value::String(detail.into()));
        self
    }
    /// Name one more fact beside `detail`, under the key the specification
    /// run records for the same cause, so a message can name it (P-MSG-1).
    pub fn with(mut self, key: &str, value: impl Into<crate::json::Value>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
    fn refused(path: &str, refusal: crate::diagnostic::Diagnostic) -> Self {
        Self {
            code: refusal.code,
            path: path.into(),
            context: refusal.context,
        }
    }
}
pub struct Input<'a> {
    pub document: &'a Document,
    /// The specification lowered into the AST, whose derivations SUB-19 and
    /// SUB-20 read in the language it is written in (#3030 D76).
    pub tree: &'a ast::Spec,
    pub source: &'a str,
    pub artifact: &'a str,
    /// Relative publication-package reference already admitted by the file
    /// adapter; absent for a dataset with no data and no transport file.
    pub href: Option<&'a str>,
}
#[derive(Clone, Debug)]
pub struct Column {
    pub(crate) declaration: spec::Column,
    pub(crate) data_type: DataType,
    /// The document's `Length`: declared, fixed by the family, or measured
    /// from the published values when a transport is requested (SUB-24).
    pub(crate) length: Option<String>,
    /// Declared, or fixed for an ADaM numeric date (SUB-25).
    pub(crate) display_format: Option<String>,
    pub(crate) mandatory: bool,
    /// One per declared origin, in declaration order (DEFINE-19).
    pub(crate) origins: Vec<OriginDefinition>,
    pub(crate) key_sequence: Option<usize>,
    pub(crate) values: Vec<ValueDefinition>,
    /// The width a transport file gives a character column (SUB-24).
    pub(crate) transport_length: Option<usize>,
}
/// One composed origin of a column or value-level entry: its declaration, the
/// source the model admits or derives (SUB-15) and its document references,
/// with the annotated case report form reference `context: Other` derives
/// (SUB-17).
#[derive(Clone, Debug)]
pub struct OriginDefinition {
    pub(crate) declaration: spec::OriginMetadata,
    pub(crate) source: Option<OriginSource>,
    pub(crate) documents: Vec<spec::DocumentReference>,
}
impl OriginDefinition {
    pub fn declaration(&self) -> &spec::OriginMetadata {
        &self.declaration
    }
    /// The origin's source, when its type takes one (SUB-15).
    pub fn source(&self) -> Option<OriginSource> {
        self.source
    }
    /// The origin's document references.
    pub fn documents(&self) -> &[spec::DocumentReference] {
        &self.documents
    }
}
/// One value-level entry's definition: the rows its where clause selects take
/// the column's metadata merged with the entry's (SUB-12, DEFINE-27).
#[derive(Clone, Debug)]
pub struct ValueDefinition {
    pub(crate) id: String,
    /// Each where column's position in the dataset's columns, the values it
    /// lists and whether it excludes them (`NE`, `NOTIN`), in written order.
    pub(crate) clause: Vec<crate::dataset::WhereTerm>,
    pub(crate) path: String,
    pub(crate) column: Column,
}
#[derive(Clone, Debug)]
pub struct Dataset {
    pub(crate) id: String,
    pub(crate) domain: String,
    /// The name of the section guide that binds the dataset, none for a
    /// non-standard dataset (DEFINE-6).
    pub(crate) standard: Option<String>,
    pub(crate) metadata: spec::Dataset,
    pub(crate) columns: Vec<Column>,
    pub(crate) href: Option<String>,
    pub(crate) artifact: String,
}
#[derive(Clone, Debug)]
pub struct Codelist {
    pub declaration: ast::Codelist,
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
/// an unadmitted declaration or fitting prefix of the selected dataset set: the
/// adapters' Define-XML, Dataset-JSON and XPT encoders read it only through
/// these accessors (#3030 section 2.10).
#[derive(Debug)]
pub struct Study {
    pub(crate) study: ast::Study,
    pub(crate) section: ast::Section,
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
    /// The project's study declaration.
    pub fn study(&self) -> &ast::Study {
        &self.study
    }
    /// The selected submission section.
    pub fn section(&self) -> &ast::Section {
        &self.section
    }
    /// The document's `FileOID`: the section's, or one derived from the study
    /// and the section.
    pub fn file_oid(&self) -> &str {
        &self.file_oid
    }
    pub fn metadata_id(&self) -> &str {
        &self.metadata_id
    }
    pub fn metadata_name(&self) -> &str {
        &self.metadata_name
    }
    pub fn metadata_description(&self) -> Option<&str> {
        self.metadata_description.as_deref()
    }
    /// The source-system version the documents name: declared, or the
    /// installed version (DEFINE-12).
    pub fn source_version(&self) -> &str {
        &self.source_version
    }
    pub fn installed_version(&self) -> &str {
        &self.installed_version
    }
    /// The selected creation time, `YYYY-MM-DDThh:mm:ss` (DEFINE-3).
    pub fn created(&self) -> &str {
        &self.created
    }
    /// The standards the document cites: every implementation guide of the
    /// section in declaration order, then the terminology standards its bound
    /// codelists cite (DEFINE-6).
    pub fn standards(&self) -> &[Standard] {
        &self.standards
    }
    pub fn datasets(&self) -> &[Dataset] {
        &self.datasets
    }
    pub fn codelists(&self) -> &[Codelist] {
        &self.codelists
    }
    /// The transport file the selected section requests, if any.
    pub fn transport(&self) -> Option<ast::Transport> {
        self.section.transport
    }
}
impl Dataset {
    pub fn id(&self) -> &str {
        &self.id
    }
    /// The declared `domain`, which a tabulation `ItemGroupDef` names.
    pub fn domain(&self) -> &str {
        &self.domain
    }
    /// The name of the implementation guide that binds the dataset: the
    /// section guide its `submission.standard` selects, or the section's
    /// first, and none when it declares `non_standard` (DEFINE-6, DEFINE-8).
    pub fn standard(&self) -> Option<&str> {
        self.standard.as_deref()
    }
    /// The dataset's composed submission metadata.
    pub fn metadata(&self) -> &spec::Dataset {
        &self.metadata
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
    /// The archive location's reference inside the publication package;
    /// absent for a dataset with no data and no transport file.
    pub fn href(&self) -> Option<&str> {
        self.href.as_deref()
    }
    /// The published artifact's path as written.
    pub fn artifact(&self) -> &str {
        &self.artifact
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
    /// The resolved submission data type.
    pub fn data_type(&self) -> DataType {
        self.data_type
    }
    /// The document's `Length`: declared, fixed by the family, or measured
    /// when a transport is requested (SUB-24).
    pub fn length(&self) -> Option<&str> {
        self.length.as_deref()
    }
    /// Declared, or fixed for an ADaM numeric date (SUB-25).
    pub fn display_format(&self) -> Option<&str> {
        self.display_format.as_deref()
    }
    pub fn mandatory(&self) -> bool {
        self.mandatory
    }
    /// The composed origins, one per declared origin in declaration order
    /// (DEFINE-19).
    pub fn origins(&self) -> &[OriginDefinition] {
        &self.origins
    }
    pub fn key_sequence(&self) -> Option<usize> {
        self.key_sequence
    }
    /// The width a transport file gives a character column, once measured
    /// (SUB-24).
    pub fn transport_length(&self) -> Option<usize> {
        self.transport_length
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
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Each where column's position in the dataset's columns, the values it
    /// lists and whether it excludes them (`NE`, `NOTIN`), in written order.
    pub fn clause(&self) -> &[crate::dataset::WhereTerm] {
        &self.clause
    }
    /// The entry's spec path.
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn column(&self) -> &Column {
        &self.column
    }
    /// Whether the entry's where clause selects a row: `holds` answers whether
    /// the row's value of one where column is one of the listed values, and
    /// an `NE` or `NOTIN` term selects the rows where it is not, a missing
    /// value included (SUB-12).
    pub(crate) fn selects<E>(
        &self,
        mut holds: impl FnMut(usize, &[String]) -> Result<bool, E>,
    ) -> Result<bool, E> {
        for term in &self.clause {
            if holds(term.column, &term.values)? == term.excludes {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
pub fn family_name(family: Submission) -> &'static str {
    match family {
        Submission::Sdtm => "sdtm",
        Submission::Adam => "adam",
        Submission::Send => "send",
    }
}
/// The model whose specifications a section lists (SUB-3).
pub fn section_model(family: Submission) -> ModelKind {
    match family {
        Submission::Sdtm => ModelKind::Sdtm,
        Submission::Adam => ModelKind::ADaM,
        Submission::Send => ModelKind::Send,
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
            findings.push(
                Finding::cause(ConditionCode::SubmissionUnknownDocument, path, id)
                    .with("document", id),
            );
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
/// The column of `tree` that `name` declares.
fn tree_column<'t>(tree: &'t ast::Spec, name: &str) -> Option<&'t ast::Column> {
    tree.columns.iter().find(|column| column.name.name == name)
}
/// The first derivation that refutes the origin type under SUB-19, if any.
fn refuting(
    tree: &ast::Spec,
    derivations: &[ast::ExprId],
    kind: OriginType,
    reads: &Reads,
) -> Option<origin::DerivationKind> {
    derivations
        .iter()
        .map(|&expr| origin::classify_expr(tree, expr, reads))
        .find(|derivation| !derivation.admits(kind))
}
/// SUB-20: the first derivation that is not the copy a Predecessor claims.
fn copy_failure(
    tree: &ast::Spec,
    derivations: &[ast::ExprId],
    predecessor: &origin::Predecessor<'_>,
    reads: &Reads,
) -> Option<origin::CopyFailure> {
    derivations
        .iter()
        .find_map(|&expr| origin::copies_expr(tree, expr, predecessor, reads).err())
}
/// SUB-2: a `metadata` key that names a governed field, at the root or, for
/// `column`, at that column, naming the key, its level and the column as the
/// specification run does.
fn reserved_metadata(
    document: &Document,
    node: usize,
    column: Option<&str>,
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let governed = if column.is_some() {
        spec::COLUMN_METADATA_FIELDS
    } else {
        spec::DATASET_METADATA_FIELDS
    };
    if let Some(N::Mapping(fields)) = document
        .field(node, "metadata")
        .and_then(|n| document.nodes().get(n))
    {
        for &(key, _) in fields {
            if let N::Text(key) = &document.nodes()[key] {
                if governed.iter().any(|field| field.name() == key) {
                    let mut finding = Finding::cause(
                        ConditionCode::SubmissionReservedMetadata,
                        &format!("{path}.metadata.{key}"),
                        key,
                    )
                    .with("key", key.as_str())
                    .with("level", if column.is_some() { "column" } else { "root" });
                    if let Some(column) = column {
                        finding = finding.with("column", column);
                    }
                    findings.push(finding);
                }
            }
        }
    }
}
const DATASET_NAME: &str = "^[A-Z][A-Z0-9]{0,7}$";
const COLUMN_NAME: &str = "^[A-Z][A-Z0-9_]{0,7}$";
/// Names and labels meet the transport limits under `Submission`, and under
/// `xpt` in every context (DEFINE-4, DEFINE-22, XPT-5).
fn transport_limits(section: &ast::Section) -> bool {
    section.context == DefineContext::Submission || section.transport == Some(ast::Transport::Xpt)
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
/// A canonical integer the document declares, as the number it spells.
fn integer(text: &str) -> crate::json::Value {
    text.parse::<crate::json::Number>()
        .map_or_else(|_| crate::json::Value::String(text.into()), Into::into)
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
/// The column or value-level item an origin is admitted for: its
/// `submission` path, its name, the derivations its origins are refuted on,
/// and its level.
#[derive(Clone, Copy)]
struct ColumnSite<'c> {
    path: &'c str,
    name: &'c str,
    derivations: &'c [ast::ExprId],
    level: Level,
}

/// Admit the complete inherited column metadata for a base or per-value item.
struct ColumnAdmission<'a> {
    historical: bool,
    doc: &'a Document,
    /// The specification's tree, whose derivations SUB-19 and SUB-20 read.
    tree: &'a ast::Spec,
    /// The dataset name, reported with transport findings.
    dataset: &'a str,
    section: &'a ast::Section,
    catalogue: &'a ct::Catalogue,
    known: &'a BTreeSet<&'a str>,
    acrfs: &'a [&'a ast::SupportingDocument],
    keys: &'a [String],
    reads: &'a Reads,
    /// The dataset's class decides whether `core` derives `mandatory` (SUB-10).
    tabulation: bool,
    /// The dataset's class, reported with a refused `datetime` column (SUB-27).
    class: &'static str,
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
        derivations: &[ast::ExprId],
        level: Level,
        findings: &mut Vec<Finding>,
        bound: &mut BTreeSet<String>,
    ) -> Option<Column> {
        if self.historical {
            return historical::column(self, column, column_path, findings, bound);
        }
        let Self {
            section,
            known,
            keys,
            ..
        } = *self;
        let name = &column.name.value;
        let Some(metadata) = &column.submission else {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, column_path, name)
                    .with("field", "submission"),
            );
            return None;
        };
        if metadata.key_sequence.is_some() {
            findings.push(crate::define_diagnostics::xml_invalid(
                column_path,
                "historical key_sequence requires mode historical",
                crate::json::Value::String(name.clone()),
            ));
        }
        if metadata.origins.is_empty() {
            findings.push(
                Finding::cause(ConditionCode::SubmissionOriginMissing, column_path, name)
                    .with("column", name.as_str()),
            );
            return None;
        }
        if column.label.is_none() {
            findings.push(
                Finding::cause(
                    ConditionCode::SubmissionMetadataMissing,
                    column_path,
                    "label",
                )
                .with("field", "label"),
            );
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
        let model = section_model(section.family);
        if core.is_none_or(|core| !core.admitted_by(model))
            || (section.family == Submission::Adam && metadata.mandatory.is_none())
            || (section.family != Submission::Adam
                && core == Some(Core::Req)
                && metadata.mandatory.as_ref().is_some_and(|m| !m.value))
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionCoreMandatory, column_path, name)
                    .with("column", name.as_str()),
            );
        }
        if let Some(role) = metadata
            .role
            .as_ref()
            .filter(|_| section.family == Submission::Adam)
        {
            findings.push(
                Finding::cause(
                    ConditionCode::SubmissionAdamField,
                    &format!("{column_path}.role"),
                    name,
                )
                .with("column", name.as_str())
                .with("value", role.value.as_str())
                .with("model", model.name()),
            );
        }
        // Each origin is admitted on its own, at its own position,
        // `origins[<i>]` (P-ORIGIN-1).
        let origins = metadata
            .origins
            .iter()
            .enumerate()
            .map(|(index, declared)| {
                let at = format!("{column_path}.origins[{index}]");
                let column = ColumnSite {
                    path: column_path,
                    name,
                    derivations,
                    level,
                };
                self.admit_origin(declared, &at, column, findings)
            })
            .collect();
        // SUB-18: a `Derived` origin anywhere needs the method that derives it.
        if metadata
            .origins
            .iter()
            .any(|declared| declared.kind.value == OriginType::Derived)
            && metadata.method.is_none()
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMethodMissing, column_path, name)
                    .with("column", name.as_str()),
            );
        }
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
            findings.push(
                Finding::cause(ConditionCode::SubmissionDataType, column_path, name)
                    .with("column", name.as_str())
                    .with("declared_type", column_type.name())
                    .with("data_type", data_type.name()),
            );
        }
        // SUB-27: an `sdtm` or `send` dataset carries its date and time text
        // in a `str` column, never in a `datetime` one.
        if !adam && level == Level::Column && column_type == spec::ColumnType::Datetime {
            let type_path = column_path
                .strip_suffix(".submission")
                .map_or_else(|| format!("{column_path}.type"), |p| format!("{p}.type"));
            let mut finding =
                Finding::cause(ConditionCode::SubmissionDatetimeColumn, &type_path, name);
            let text = |value: &str| crate::json::Value::String(value.into());
            for (key, value) in [
                ("column", text(name)),
                ("value", text("datetime")),
                ("class", text(self.class)),
                (
                    "permitted",
                    crate::json::Value::Array(vec![text("str"), text("date")]),
                ),
            ] {
                finding.context.insert(key.into(), value);
            }
            findings.push(finding);
        }
        let display_format = match column_type.numeric_date_format(adam) {
            Some(fixed) => {
                if let Some(declared) = metadata
                    .display_format
                    .as_ref()
                    .filter(|declared| declared.value != fixed)
                {
                    findings.push(
                        Finding::cause(
                            ConditionCode::SubmissionDisplayFormatConflict,
                            &format!("{column_path}.display_format"),
                            fixed,
                        )
                        .with("column", name.as_str())
                        .with("value", declared.value.as_str())
                        .with("permitted", fixed),
                    );
                }
                Some(fixed.to_string())
            }
            None => metadata.display_format.as_ref().map(|d| d.value.clone()),
        };
        // XPT-9: a numeric variable's declared display format must be a SAS
        // format the transport can carry; only the column's own one is written.
        if level == Level::Column
            && section.transport == Some(ast::Transport::Xpt)
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
            origins,
            key_sequence: keys.iter().position(|key| key == name).map(|k| k + 1),
            values: Vec::new(),
            transport_length: None,
        })
    }
    /// Admit one declared origin of the column or entry at `column_path`, at
    /// `at`, its own position: its type and source pair (SUB-15), its
    /// description (SUB-16), its annotated case report form reference
    /// (SUB-17) and the derivations its type claims (SUB-19, SUB-20), each
    /// refutation naming the refuted type.
    fn admit_origin(
        &self,
        declared: &spec::OriginMetadata,
        at: &str,
        column: ColumnSite<'_>,
        findings: &mut Vec<Finding>,
    ) -> OriginDefinition {
        let ColumnSite {
            path: column_path,
            name,
            derivations,
            level,
        } = column;
        let Self {
            tree,
            section,
            known,
            acrfs,
            reads,
            ..
        } = *self;
        let kind = declared.kind.value;
        let model = section_model(section.family);
        let source = declared.source.as_ref().map(|s| s.value);
        // D65: a specification takes no `Not Available` origin under SDTM.
        if !kind.admits_source(model, source) {
            let mut finding = Finding::cause(ConditionCode::SubmissionOriginSource, at, name)
                .with("column", name)
                .with("origin_type", kind.name());
            if let Some(source) = source {
                finding = finding.with("origin_source", source.name());
            }
            findings.push(finding.with("model", model.name()));
        }
        let description = declared.description.as_ref().map(|d| d.value.as_str());
        let predecessor = description
            .filter(|_| kind == OriginType::Predecessor)
            .and_then(origin::predecessor);
        if matches!(
            kind,
            OriginType::Other | OriginType::Predecessor | OriginType::NotAvailable
        ) && description.is_none()
        {
            findings.push(
                Finding::cause(ConditionCode::SubmissionOriginDescription, at, name)
                    .with("column", name)
                    .with("origin_type", kind.name()),
            );
        } else if kind == OriginType::Predecessor && predecessor.is_none() {
            // SUB-16: a Predecessor description names `<dataset>.<column>`.
            findings.push(
                Finding::cause(
                    ConditionCode::SubmissionOriginDescription,
                    &format!("{at}.description"),
                    name,
                )
                .with("column", name)
                .with("origin_type", kind.name())
                .with("reason", "predecessor_form"),
            );
        }
        // SUB-19 and SUB-20: the derivations refute the declared type, or are
        // not the copy a Predecessor claims. A Predecessor that is not the
        // named copy is refuted by a bare `source`, which SUB-19 alone admits.
        let refuted = refuting(tree, derivations, kind, reads)
            .map(|derivation| (derivation, None))
            .or_else(|| {
                predecessor
                    .as_ref()
                    .and_then(|predecessor| copy_failure(tree, derivations, predecessor, reads))
                    .map(|failure| {
                        (
                            origin::classify_expr(tree, derivations[0], reads),
                            Some(failure),
                        )
                    })
            });
        if let Some((derivation, failure)) = refuted {
            let code = match level {
                Level::Column => ConditionCode::SubmissionOriginRefuted,
                Level::Value => ConditionCode::SubmissionValueOriginRefuted,
            };
            let mut finding = Finding::cause(code, column_path, name).with("column", name);
            let text = |value: &str| crate::json::Value::String(value.into());
            finding
                .context
                .insert("origin_type".into(), text(kind.name()));
            if level == Level::Value {
                finding
                    .context
                    .insert("derivation_kind".into(), text(derivation.name()));
            }
            if let Some(failure) = failure {
                finding
                    .context
                    .insert("reason".into(), text(failure.reason()));
            }
            findings.push(finding);
        }
        let mut refs = declared.documents.value.clone();
        if kind == OriginType::Collected
            && matches!(
                source,
                Some(OriginSource::Investigator | OriginSource::Subject)
            )
        {
            // SUB-17: under `Submission` the reference says where in the form
            // the value was collected; only `Other` derives a page-less one.
            let submission = section.context == DefineContext::Submission;
            if acrfs.len() != 1 {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionAnnotatedCrfMissing,
                    at,
                    name,
                ));
            } else if declared.documents.origin.key.is_none() {
                if submission {
                    findings.push(Finding::cause(
                        ConditionCode::SubmissionOriginDocument,
                        at,
                        name,
                    ));
                } else {
                    refs.push(spec::DocumentReference::Plain(spec::Field {
                        value: acrfs[0].id.name.clone(),
                        origin: declared.documents.origin,
                    }));
                }
            } else if refs.is_empty()
                || refs.iter().any(|r| reference_id(r) != acrfs[0].id.name)
                || (submission && !refs.iter().any(paged))
            {
                findings.push(Finding::cause(
                    ConditionCode::SubmissionOriginDocument,
                    &format!("{at}.documents"),
                    name,
                ));
            }
        }
        references(findings, &refs, known, &format!("{at}.documents"));
        OriginDefinition {
            declaration: declared.clone(),
            // SUB-15: an ADaM source is derived, never declared.
            source: if section.family == Submission::Adam && kind != OriginType::Predecessor {
                Some(OriginSource::Sponsor)
            } else {
                source
            },
            documents: refs,
        }
    }
    /// Admit length and significant-digit declarations; return the effective length.
    fn admit_length(
        &self,
        column: &spec::Column,
        metadata: &spec::ColumnSubmission,
        data_type: DataType,
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
        // Each finding names the column, and the field it is about, as the
        // specification run does (SUB-7, SUB-8).
        let length_finding = |code, field: &str| {
            let detail = if field == "length" { name } else { field };
            let finding = Finding::cause(code, column_path, detail)
                .with("column", name)
                .with("field", field);
            if field == "length" {
                finding.with("data_type", data_type.name())
            } else {
                finding
            }
        };
        if column_type.temporal() {
            if metadata.length.is_some() {
                findings.push(length_finding(
                    ConditionCode::SubmissionLengthNotApplicable,
                    "length",
                ));
            }
            if metadata.significant_digits.is_some() {
                findings.push(length_finding(
                    ConditionCode::SubmissionLengthNotApplicable,
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
        let takes_length = matches!(
            data_type,
            DataType::Text | DataType::Integer | DataType::Float
        );
        // SUB-7: a text column without `length` takes the published length,
        // which only a transport measures (SUB-24).
        let needs_length =
            takes_length && (data_type != DataType::Text || self.section.transport.is_none());
        if needs_length && length.is_none() {
            findings.push(length_finding(
                ConditionCode::SubmissionLengthMissing,
                "length",
            ));
        }
        if !takes_length && metadata.length.is_some() {
            findings.push(length_finding(
                ConditionCode::SubmissionLengthNotApplicable,
                "length",
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
            findings.push(
                Finding::cause(ConditionCode::SubmissionLengthInvalid, column_path, name)
                    .with("column", name),
            );
        }
        if let Some((declared, max)) = metadata.length.as_ref().and_then(|declared| {
            maxima
                .iter()
                .find(|max| **max != declared.value)
                .map(|max| (declared, max))
        }) {
            findings.push(
                Finding::cause(ConditionCode::SubmissionLengthConflict, column_path, name)
                    .with("column", name)
                    .with("length", integer(&declared.value))
                    .with("max_length", integer(max)),
            );
        }
        if (data_type == DataType::Float) != metadata.significant_digits.is_some() {
            findings.push(length_finding(
                if data_type == DataType::Float {
                    ConditionCode::SubmissionLengthMissing
                } else {
                    ConditionCode::SubmissionLengthNotApplicable
                },
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
                // Each binding finding names the column and the codelist, and
                // a type mismatch both types, as the specification run does
                // (CT-1, CT-8).
                let finding = |code| {
                    Finding::cause(code, &codelist_path, &list.value)
                        .with("column", column.name.value.as_str())
                        .with("codelist", list.value.as_str())
                };
                let finding = match binding {
                    ct::BindingFinding::UnknownCodelist => {
                        finding(ConditionCode::CodelistUnknownBinding)
                    }
                    ct::BindingFinding::TypeMismatch { expected, actual } => {
                        finding(ConditionCode::CodelistBindingTypeMismatch)
                            .with(
                                "codelist_data_type",
                                crate::project_codelist_binding::codelist_data_type(expected),
                            )
                            .with(
                                "column_type",
                                crate::project_codelist_binding::column_type(actual),
                            )
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
                findings.push(finding);
            }
        }
    }
}

/// SUB-10 for one column at composition: a key column declares no
/// `mandatory: false`. Preparation reports a `not_missing` verification that
/// restates a resolution (SUB-11) before composition.
fn key_column_mandatory(
    column: &spec::Column,
    keys: &[String],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let name = &column.name.value;
    let declared = column
        .submission
        .as_ref()
        .and_then(|metadata| metadata.mandatory.as_ref());
    if keys.contains(name) && declared.is_some_and(|mandatory| !mandatory.value) {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionKeyMandatory,
                &format!("{path}.columns.{name}.submission.mandatory"),
                name,
            )
            .with("column", name.as_str())
            .with("value", false),
        );
    }
}

/// Compose every column's value-level entries (SUB-12, SUB-13): the static
/// checks of each column's entries, then one definition per entry carrying
/// the column's metadata merged with the entry's.
fn compose_values(
    admission: &ColumnAdmission<'_>,
    declarations: &spec::Declarations,
    output_columns: &[String],
    selected: &mut [Column],
    path: &str,
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) {
    let doc = admission.doc;
    let columns = &declarations
        .columns()
        .expect("admitted dataset columns")
        .value;
    let positions: Vec<String> = selected
        .iter()
        .map(|column| column.declaration.name.value.clone())
        .collect();
    for base in columns {
        let name = &base.name.value;
        let Some(metadata) = base.submission.as_ref() else {
            continue;
        };
        if metadata.value_level.is_empty() || !output_columns.contains(name) {
            continue;
        }
        let entries = values::entries(doc, base.node);
        let written = entries.iter().map(|entry| (entry.id, &entry.clause));
        let mode = if admission.historical {
            spec::SubmissionMode::Historical
        } else {
            spec::SubmissionMode::Validated
        };
        let static_findings = values::admit(written, name, mode, |where_column| {
            let declared = columns.iter().find(|c| c.name.value == where_column)?;
            Some(values::Declared {
                output: output_columns.iter().any(|c| c == where_column),
                text: declared.column_type.value == spec::ColumnType::String,
                codelist: declared.submission.as_ref().and_then(|metadata| {
                    values::admitted(
                        admission.catalogue,
                        metadata.codelist.as_ref().map(|list| list.value.as_str()),
                        metadata
                            .value_level
                            .iter()
                            .filter_map(|entry| entry.metadata.codelist.as_ref())
                            .map(|list| list.value.as_str()),
                    )
                }),
            })
        });
        for finding in &static_findings {
            let text = |value: &str| crate::json::Value::String(value.into());
            let (code, suffix, detail, mut context) = match *finding {
                values::Finding::DuplicateId { id } => (
                    ConditionCode::SubmissionValueDuplicateId,
                    format!("{}.id", values::path(name, id)),
                    id.to_owned(),
                    vec![("entry", text(id)), ("identifier", text(id))],
                ),
                values::Finding::Where {
                    id,
                    column,
                    ref fault,
                } => {
                    // A clause that names no column has no where column (SUB-12).
                    let where_column = column.map_or(crate::json::Value::Null, text);
                    let mut context = vec![
                        ("entry", text(id)),
                        ("reason", text(fault.reason())),
                        ("where_column", where_column),
                    ];
                    // The refused where values, in declared order.
                    context.extend(fault.values().map(|refused| {
                        let refused = refused.iter().map(|value| text(value)).collect();
                        ("values", crate::json::Value::Array(refused))
                    }));
                    // The comparator whose value count is refused.
                    context.extend(
                        fault
                            .comparator()
                            .map(|comparator| ("comparator", text(comparator))),
                    );
                    (
                        ConditionCode::SubmissionValueWhereInvalid,
                        column.map_or_else(
                            || format!("{}.where", values::path(name, id)),
                            |column| format!("{}.where.{column}", values::path(name, id)),
                        ),
                        fault.reason().to_owned(),
                        context,
                    )
                }
                // A finding carries one path: the later clause's, naming the
                // earlier entry beside it.
                values::Finding::Overlap { id, other } => (
                    ConditionCode::SubmissionValueOverlap,
                    format!("{}.where", values::path(name, id)),
                    other.to_owned(),
                    vec![(
                        "entries",
                        crate::json::Value::Array(vec![text(other), text(id)]),
                    )],
                ),
            };
            context.push(("column", text(name)));
            let mut finding = Finding::cause(code, &format!("{path}.{suffix}"), &detail);
            finding
                .context
                .extend(context.into_iter().map(|(key, value)| (key.into(), value)));
            findings.push(finding);
        }
        if !static_findings.is_empty() {
            continue;
        }
        let Some(position) = positions.iter().position(|column| column == name) else {
            continue;
        };
        for entry in &metadata.value_level {
            let id = &entry.id.value;
            let entry_path = format!("{path}.{}", values::path(name, id));
            let clause = entry
                .clause
                .iter()
                .map(|term| {
                    positions
                        .iter()
                        .position(|column| column == &term.column.value)
                        .map(|index| crate::dataset::WhereTerm {
                            column: index,
                            values: term
                                .values
                                .iter()
                                .map(|value| value.value.clone())
                                .collect(),
                            excludes: term.comparator.excludes(),
                        })
                })
                .collect::<Option<Vec<_>>>();
            let mut effective = base.clone();
            effective.submission = Some(entry.metadata.inherit(metadata));
            if let Some(label) = &entry.label {
                effective.label = Some(label.clone());
            }
            let first_finding = findings.len();
            if !admission.historical
                && admission.keys.contains(name)
                && entry.metadata.mandatory.as_ref().is_some_and(|m| !m.value)
            {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionKeyMandatory,
                        &format!("{entry_path}.mandatory"),
                        name,
                    )
                    .with("value", false),
                );
            }
            // An entry without origins of its own answers with the column's,
            // which the column's own refutation already covers (SUB-13).
            let derivations = match tree_column(admission.tree, name) {
                Some(column) if !entry.metadata.origins.is_empty() => values::levels(column)
                    .iter()
                    .find(|level| level.id.name == *id)
                    .map_or_else(Vec::new, |level| {
                        values::entry_expressions(admission.tree, column, level)
                    }),
                _ => Vec::new(),
            };
            let value = admission.admit(
                &effective,
                &entry_path,
                &derivations,
                Level::Value,
                findings,
                bound,
            );
            for finding in &mut findings[first_finding..] {
                for (key, value) in [("column", name.as_str()), ("entry", id.as_str())] {
                    finding
                        .context
                        .insert(key.into(), crate::json::Value::String(value.into()));
                }
            }
            if let (Some(value), Some(clause)) = (value, clause) {
                selected[position].values.push(ValueDefinition {
                    id: id.clone(),
                    clause,
                    path: entry_path,
                    column: value,
                });
            }
        }
    }
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
        if metadata.kind.value != OriginType::Predecessor {
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

/// Whether `guide` is an implementation guide of the model a `family`
/// section lists (SUB-3's table): the guide names are closed (DEFINE-6), so
/// each model's guides are the names that start with its own guide's.
fn guide_of(family: Submission, guide: &str) -> bool {
    match family {
        Submission::Sdtm => guide.starts_with("SDTMIG"),
        Submission::Adam => guide.starts_with("ADaMIG"),
        Submission::Send => guide.starts_with("SENDIG"),
    }
}

/// DEFINE-6's findings for a 1.0 section's `standards`: a guide outside the
/// section's model fails as `standard_model_mismatch`, and a name an earlier
/// entry takes as `duplicate_identifier`, each at the entry's `name`.
fn section_standards(
    family: Submission,
    guides: &[ast::ImplementationGuide],
    findings: &mut Vec<Finding>,
) {
    let section = family_name(family);
    let text = |value: &str| crate::json::Value::String(value.into());
    for (index, guide) in guides.iter().enumerate() {
        let name = guide.name.text.as_str();
        let (code, context) = if guides[..index]
            .iter()
            .any(|earlier| earlier.name.text == name)
        {
            (
                ConditionCode::SectionStandardDuplicate,
                vec![("identifier", text(name))],
            )
        } else if !guide_of(family, name) {
            (
                ConditionCode::SectionStandardModel,
                vec![
                    ("value", text(name)),
                    ("model", text(section_model(family).name())),
                    ("section", text(section)),
                ],
            )
        } else {
            continue;
        };
        findings.push(Finding {
            code,
            path: format!("{section}.standards[{index}].name"),
            context: context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        });
    }
}

/// The section guide a dataset's `submission.standard` selects, or the
/// section's first when it selects none (DEFINE-8). A name the section does
/// not list fails as `unknown_standard` at `path`, naming the section and its
/// standards, and the dataset composes on under the first.
fn bound_standard(
    family: Submission,
    guides: &[ast::ImplementationGuide],
    selected: Option<&spec::Field<String>>,
    path: &str,
    findings: &mut Vec<Finding>,
) -> String {
    let first = || guides[0].name.text.clone();
    let Some(selected) = selected else {
        return first();
    };
    if let Some(guide) = guides
        .iter()
        .find(|guide| guide.name.text == selected.value)
    {
        return guide.name.text.clone();
    }
    let text = |value: &str| crate::json::Value::String(value.into());
    findings.push(Finding {
        code: ConditionCode::SubmissionUnknownStandard,
        path: path.into(),
        context: [
            ("value".to_string(), text(&selected.value)),
            ("section".to_string(), text(family_name(family))),
            (
                "standards".to_string(),
                crate::json::Value::Array(
                    guides.iter().map(|guide| text(&guide.name.text)).collect(),
                ),
            ),
        ]
        .into(),
    });
    first()
}

/// SUB-3's finding for a 1.x specification a section lists at `path`, unless
/// it declares the section's model: `missing_required_field` without
/// `model`, and `standard_model_mismatch` for another model, naming the
/// section and its model.
fn listed_model(tree: &ast::Spec, family: Submission, path: &str) -> Option<Finding> {
    let model = section_model(family);
    let text = |value: &str| crate::json::Value::String(value.into());
    let (code, context) = match &tree.model {
        None => (
            ConditionCode::SubmissionModelRequired,
            vec![
                ("field", text("model")),
                ("model", text(model.name())),
                ("section", text(family_name(family))),
            ],
        ),
        Some(declared) if declared.kind != model => (
            ConditionCode::SubmissionModelMismatch,
            vec![
                ("value", text(declared.kind.name())),
                ("model", text(model.name())),
                ("section", text(family_name(family))),
            ],
        ),
        Some(_) => return None,
    };
    Some(Finding {
        code,
        path: format!("{path}.model"),
        context: context
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    })
}

/// What composing a section's datasets reads besides each listed
/// specification: the section and its implementation guides, never empty;
/// the CT catalogue; every specification any submission section lists
/// (`study`); the identifiers the document already takes; and the
/// section's annotated CRFs.
struct Composition<'a> {
    section: &'a ast::Section,
    guides: &'a [ast::ImplementationGuide],
    catalogue: &'a ct::Catalogue,
    study: &'a [&'a Document],
    known: &'a BTreeSet<&'a str>,
    acrfs: &'a [&'a ast::SupportingDocument],
}

/// Compose every captured dataset in order, collecting codelist bindings.
fn compose_datasets(
    composition: &Composition<'_>,
    inputs: &[Input<'_>],
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) -> Vec<Dataset> {
    let Composition {
        section,
        guides,
        catalogue,
        study,
        known,
        acrfs,
    } = *composition;
    let family = family_name(section.family);
    let mut datasets = Vec::new();
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for (index, input) in inputs.iter().enumerate() {
        let doc = input.document;
        let path = format!("{family}.specs[{index}]");
        // A section lists only specifications that declare its model (SUB-3,
        // D25). A missing `model` or another model fails at the listed
        // specification's `model`, and its dataset composes no further,
        // since every rule after this one reads the model.
        if let Some(finding) = listed_model(input.tree, section.family, &path) {
            findings.push(finding);
            continue;
        }
        reserved_metadata(doc, doc.root(), None, &path, findings);
        let reads = Reads::of_spec(input.tree);
        let historical = input
            .tree
            .submission
            .as_ref()
            .is_some_and(|metadata| metadata.mode == spec::SubmissionMode::Historical);
        let id = text(doc, doc.root(), "domain").unwrap_or_default();
        if !ids.insert(id.clone()) || known.contains(id.as_str()) {
            findings.push(
                Finding::cause(ConditionCode::SubmissionDuplicateIdentifier, &path, &id)
                    .with("identifier", id.as_str()),
            );
        }
        if !sources.insert(input.source) || (!historical && !artifacts.insert(input.artifact)) {
            findings.push(Finding::cause(
                ConditionCode::SubmissionDuplicateDataset,
                &path,
                &id,
            ));
        }
        transport_decimals(section, doc, index, &path, &id, findings);
        let declarations = match spec::decode(doc, doc.root()) {
            Ok(value) => value,
            Err(_) => {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionMetadataMissing,
                        &path,
                        "invalid closed submission declarations",
                    )
                    .with("field", "submission"),
                );
                continue;
            }
        };
        let Some(dataset) = declarations.dataset() else {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, &path, &id)
                    .with("field", "submission"),
            );
            continue;
        };
        historical::dataset(section, dataset, &path, findings);
        let Some(columns) = declarations.columns() else {
            findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, &path, "columns")
                    .with("field", "columns"),
            );
            continue;
        };
        // A non-standard dataset binds no guide (DEFINE-6); one that also
        // selects a guide fails in `admit_dataset` instead (SUB-4).
        let standard = (!dataset.non_standard.value).then(|| {
            bound_standard(
                section.family,
                guides,
                dataset.standard.as_ref(),
                &format!("{path}.submission.standard"),
                findings,
            )
        });
        admit_dataset(section, known, dataset, &path, &id, findings);
        let tabulation = dataset.class.value.tabulation();
        let output = doc.field(doc.root(), "output");
        let output_columns = output.map_or_else(Vec::new, |out| strings(doc, out, "columns"));
        let keys = strings(doc, doc.root(), "keys");
        let mut names = BTreeSet::new();
        for column in &columns.value {
            if !names.insert(&column.name.value) {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionDuplicateIdentifier,
                        &format!("{path}.columns"),
                        &column.name.value,
                    )
                    .with("identifier", column.name.value.as_str()),
                );
            }
        }
        admit_keys(doc, columns, &keys, &output_columns, &path, findings);
        let mut selected = Vec::new();
        for column in &columns.value {
            reserved_metadata(
                doc,
                column.node,
                Some(&column.name.value),
                &format!("{path}.columns.{}", column.name.value),
                findings,
            );
            if column.submission.is_some() && !output_columns.contains(&column.name.value) {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionColumnNotOutput,
                        &format!("{path}.columns.{}.submission", column.name.value),
                        &column.name.value,
                    )
                    .with("column", column.name.value.as_str()),
                );
            }
        }
        let admission = ColumnAdmission {
            historical,
            doc,
            tree: input.tree,
            dataset: &id,
            section,
            catalogue,
            known,
            acrfs,
            keys: &keys,
            reads: &reads,
            tabulation,
            class: dataset.class.value.name(),
        };
        for (position, name) in output_columns.iter().enumerate() {
            // OUTPUT-1: an entry that repeats an earlier one, or else names no
            // declared column, fails as checking the specification refuses
            // it, at the entry under the listing, and DEFINE-7 reads no
            // metadata for it.
            let repeated = output_columns[..position].contains(name);
            let column = match columns.value.iter().find(|c| &c.name.value == name) {
                Some(column) if !repeated => column,
                _ => {
                    let name = name.clone();
                    let refusal = if repeated {
                        OutputFinding::DuplicateColumn { position, name }
                    } else {
                        OutputFinding::UndeclaredColumn { position, name }
                    };
                    findings.push(Finding::listed(&path, refusal.diagnostic()));
                    continue;
                }
            };
            let column_path = format!("{path}.columns.{name}.submission");
            if !historical {
                key_column_mandatory(column, admission.keys, &path, findings);
            }
            // Every origin the column and its entries declare.
            let origins: Vec<_> = column
                .submission
                .iter()
                .flat_map(|metadata| {
                    metadata.origins.iter().chain(
                        metadata
                            .value_level
                            .iter()
                            .flat_map(|entry| &entry.metadata.origins),
                    )
                })
                .collect();
            if !historical {
                predecessor_labels(study, column, &origins, &path, findings);
            }
            // The column's origin answers for every construction whose rows an
            // entry declaring its own origin does not all select (SUB-19).
            let derivations = tree_column(input.tree, name).map_or_else(Vec::new, |column| {
                values::column_expressions(input.tree, column)
            });
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
        redundant_unique(doc, &keys, &path, findings);
        compose_values(
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
            standard,
            metadata: dataset.clone(),
            columns: selected,
            artifact: input.artifact.into(),
            href: if historical {
                dataset.artifact.as_ref().map(|link| link.value.clone())
            } else {
                input.href.map(String::from)
            },
        });
    }
    datasets
}

/// JSON-5: a transport re-reads the published artifact, so display digits
/// the csv profile writes would become the submitted values.
fn transport_decimals(
    section: &ast::Section,
    doc: &Document,
    index: usize,
    path: &str,
    id: &str,
    findings: &mut Vec<Finding>,
) {
    if section.transport.is_some() {
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
                .insert("dataset".into(), crate::json::Value::String(id.into()));
            // The artifact the decimals would round, as `output.path`
            // writes it (CSV-6).
            let written = doc
                .field(doc.root(), "output")
                .and_then(|output| text(doc, output, "path"));
            finding.context.insert(
                "path".into(),
                written.map_or(crate::json::Value::Null, crate::json::Value::String),
            );
            if let Some(written) = section.specs.get(index) {
                finding.context.insert(
                    "specification".into(),
                    crate::json::Value::String(written.text.clone()),
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
}

/// The declarations of a listed specification's dataset the section reads:
/// its name and label in a regulator's transport (DEFINE-22, XPT-5), reference
/// data that does not repeat, no ADaM `domain`, a class of the section's
/// family, and a comment for a dataset without data, whose references name
/// known documents.
fn admit_dataset(
    section: &ast::Section,
    known: &BTreeSet<&str>,
    dataset: &spec::Dataset,
    path: &str,
    id: &str,
    findings: &mut Vec<Finding>,
) {
    if transport_limits(section) {
        if !dataset_name(id) {
            findings.push(name_finding(path, id, DATASET_NAME));
        }
        if let Some(cause) = label_cause(&dataset.label.value) {
            findings.push(label_finding(
                &format!("{path}.submission.label"),
                id,
                None,
                &dataset.label.value,
                cause,
            ));
        }
    }
    if dataset.reference_data.value && dataset.repeating.value {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionReferenceRepeating,
                &format!("{path}.submission"),
                id,
            )
            .with("dataset", id),
        );
    }
    if let Some(standard) = dataset
        .standard
        .as_ref()
        .filter(|_| dataset.non_standard.value)
    {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionNonStandardBound,
                &format!("{path}.submission.standard"),
                id,
            )
            .with("value", standard.value.as_str())
            .with("non_standard", true),
        );
    }
    let model = section_model(section.family).name();
    if let Some(domain) = dataset
        .domain
        .as_ref()
        .filter(|_| section.family == Submission::Adam)
    {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionAdamField,
                &format!("{path}.submission.domain"),
                id,
            )
            .with("value", domain.value.as_str())
            .with("model", model),
        );
    }
    if dataset.non_standard.value && section.family == Submission::Adam {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionAdamField,
                &format!("{path}.submission.non_standard"),
                id,
            )
            .with("value", true)
            .with("model", model),
        );
    }
    // The domain build reads the family group from the class alone, so the
    // class must belong to the family this document binds the dataset to.
    let tabulation = dataset.class.value.tabulation();
    if tabulation == (section.family == Submission::Adam) {
        let class = dataset.class.value.name();
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionClassFamily,
                &format!("{path}.submission.class"),
                class,
            )
            .with("value", class)
            .with("model", model),
        );
    }
    if dataset.has_no_data.value && dataset.comment.is_none() {
        findings.push(
            Finding::cause(
                ConditionCode::SubmissionNoDataComment,
                &format!("{path}.submission.comment"),
                id,
            )
            .with("field", "comment"),
        );
    }
    comment_references(
        findings,
        dataset.comment.as_ref(),
        known,
        &format!("{path}.submission.comment"),
    );
}

/// OUTPUT-2: `keys` fails as checking the specification refuses it, with
/// the specification's own refusal at its spec path under the listing:
/// an empty list, and each key an earlier key repeats, no column
/// declares, or `output.columns` leaves out. A missing list is the
/// schema's to refuse.
fn admit_keys(
    doc: &Document,
    columns: &spec::Field<Vec<spec::Column>>,
    keys: &[String],
    output_columns: &[String],
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let declared = |name: &String| {
        columns
            .value
            .iter()
            .any(|column| &column.name.value == name)
    };
    let written = doc.field(doc.root(), "keys").map(|node| &doc.nodes()[node]);
    if matches!(written, Some(N::Sequence(items)) if items.is_empty()) {
        findings.push(Finding::listed(
            path,
            PreflightFinding::EmptyKeys.diagnostic(),
        ));
    }
    for (position, key) in keys.iter().enumerate() {
        if keys[..position].contains(key) {
            let refusal = PreflightFinding::DuplicateKey {
                position,
                identifier: key.clone(),
            };
            findings.push(Finding::listed(path, refusal.diagnostic()));
        }
        let refusal = if !declared(key) {
            PreflightFinding::UndeclaredKey {
                position,
                column: key.clone(),
            }
            .diagnostic()
        } else if !output_columns.contains(key) {
            OutputFinding::InternalKey {
                position,
                name: key.clone(),
            }
            .diagnostic()
        } else {
            continue;
        };
        findings.push(Finding::listed(path, refusal));
    }
}

/// A `unique` over exactly the keys restates their asserted identity.
fn redundant_unique(doc: &Document, keys: &[String], path: &str, findings: &mut Vec<Finding>) {
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
}

/// The source-system version Define-XML and Dataset-JSON metadata carry: the
/// section's authored override, or else the version the calling package
/// supplies as installed (DEFINE-12). Core's own crate version is never a
/// substitute for the Python or R generator, and neither may be empty.
fn source_version<'a>(section: &'a ast::Section, installed: &'a str) -> Option<&'a str> {
    if installed.is_empty() {
        return None;
    }
    let metadata = section
        .source_system_version
        .as_ref()
        .map_or(installed, |version| version.text.as_str());
    (!metadata.is_empty()).then_some(metadata)
}

/// The declarations composition requires beyond the specifications a run uses.
#[derive(Clone, Copy, Debug)]
pub struct Required<'a> {
    pub study: &'a ast::Study,
    pub section: &'a ast::Section,
    /// The section's implementation guides, never empty: its `standards` in
    /// declaration order (DEFINE-6).
    pub standards: &'a [ast::ImplementationGuide],
    pub define: &'a ast::Text,
}
impl<'a> Required<'a> {
    /// A section admits for runs with only its `specs`. Composition of the
    /// selected `section` requires the project `study` and the section's
    /// guide and `define`, and reports every missing one together as a
    /// missing required field: the guide as `standards`, whose list may also
    /// be empty (DEFINE-28).
    pub fn admit(
        study: Option<&'a ast::Study>,
        section: &'a ast::Section,
    ) -> Result<Self, Vec<Finding>> {
        let family = family_name(section.family);
        let missing = |path: String, field: &str| {
            Finding::cause(ConditionCode::DefineRequiredDeclaration, &path, field)
                .with("field", field)
        };
        let standards = (!section.standards.is_empty()).then_some(section.standards.as_slice());
        let define = section.define.as_ref();
        if let (Some(study), Some(standards), Some(define)) = (study, standards, define) {
            return Ok(Self {
                study,
                section,
                standards,
                define,
            });
        }
        let mut findings = Vec::new();
        if study.is_none() {
            findings.push(missing("study".into(), "study"));
        }
        if standards.is_none() {
            findings.push(missing(format!("{family}.standards"), "standards"));
        }
        if define.is_none() {
            findings.push(missing(format!("{family}.define"), "define"));
        }
        Err(findings)
    }
}

/// Whether composing the specification `document` reads the specifications
/// the study's other sections list: it requests the study's dataset
/// inventory (CT-11) or names a predecessor's columns (SUB-20).
pub fn reads_other_sections(document: &Document) -> bool {
    crate::submission_inventory::requested(document) || origin::names_predecessor(document)
}

/// DEFINE-4: a regulator takes a dataset only in a transport file, so a
/// `Submission` section declares one and its document never names the CSV or
/// Parquet artifact. Publication preparation asks this before it locates any
/// target, so the missing transport is the first finding.
pub fn transport_required(section: &ast::Section) -> Option<Finding> {
    (section.context == DefineContext::Submission && section.transport.is_none()).then(|| {
        Finding::cause(
            ConditionCode::SubmissionTransportRequired,
            &format!("{}.transport", family_name(section.family)),
            "transport",
        )
        .with("field", "transport")
    })
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
        standards: guides,
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
    let cause = |code, suffix: &str, detail: &str| {
        Finding::cause(code, &format!("{prefix}.{suffix}"), detail)
    };
    if inputs.len() != section.specs.len() {
        findings.push(
            cause(
                ConditionCode::SubmissionMetadataMissing,
                "specs",
                "incomplete captured dataset set",
            )
            .with("field", "specs"),
        );
    }
    // Every guide a section lists belongs to its model under a name of its
    // own (DEFINE-6, D16).
    section_standards(section.family, guides, &mut findings);
    if !define.text.to_ascii_lowercase().ends_with(".xml") {
        findings.push(
            cause(
                ConditionCode::SubmissionDefineExtension,
                "define",
                "expected .xml extension",
            )
            .with("value", define.text.as_str()),
        );
    } else if section.transport.is_some() && define.text.rsplit('/').next() != Some("define.xml") {
        // DEFINE-2: the transport files sit beside a document named define.xml.
        findings.push(
            cause(
                ConditionCode::SubmissionDefineName,
                "define",
                "a transport requires the document file name define.xml",
            )
            .with("value", define.text.as_str()),
        );
    }
    findings.extend(transport_required(section));
    let source_version = match source_version(section, installed) {
        Some(version) => version.into(),
        None => {
            findings.push(
                cause(
                    ConditionCode::SubmissionSourceVersion,
                    "source_system_version",
                    "empty generator version",
                )
                .with("section", family),
            );
            String::new()
        }
    };
    let mut known = BTreeSet::new();
    for document in &section.documents {
        if !known.insert(document.id.name.as_str()) {
            findings.push(
                cause(
                    ConditionCode::SubmissionDuplicateIdentifier,
                    "documents",
                    &document.id.name,
                )
                .with("identifier", document.id.name.as_str()),
            );
        }
    }
    let acrfs: Vec<_> = section
        .documents
        .iter()
        .filter(|d| d.kind == ast::DocumentKind::AnnotatedCrf)
        .collect();
    if acrfs.len() > 1 {
        // The annotated case report forms, in declaration order (DEFINE-14).
        let declared = acrfs
            .iter()
            .map(|document| crate::json::Value::String(document.id.name.clone()))
            .collect::<Vec<_>>();
        findings.push(
            cause(
                ConditionCode::SubmissionAnnotatedCrfMultiplicity,
                "documents",
                "multiple annotated CRFs",
            )
            .with("value", declared),
        );
    }
    let mut bound = BTreeSet::new();
    let composition = Composition {
        section,
        guides,
        catalogue,
        study: &specifications,
        known: &known,
        acrfs: &acrfs,
    };
    let datasets = compose_datasets(&composition, inputs, &mut findings, &mut bound);
    // The document lists every guide of the section in declaration order,
    // whether or not a dataset binds it, then the terminology standards
    // (DEFINE-6).
    let mut standards: Vec<_> = guides
        .iter()
        .map(|guide| Standard {
            id: guide.name.text.clone(),
            name: guide.name.text.clone(),
            kind: "IG",
            publishing_set: None,
            version: guide.version.text.clone(),
            status: guide.status.name(),
        })
        .collect();
    let mut codelists = Vec::new();
    for (index, source) in catalogue.sources().iter().enumerate() {
        if !source.codelists.iter().any(|c| bound.contains(&c.id.name)) {
            continue;
        }
        let standard = source.standard.as_ref().map(|ct| {
            // DEFINE-9: a terminology standard is identified by its publishing set.
            let id = format!("CT_{}", ct.publishing_set.text);
            match standards.iter().find(|s| s.id == id) {
                // Sources citing one standard share its definition.
                Some(prior)
                    if prior.kind == "CT"
                        && prior.name == ct.name.text
                        && prior.version == ct.version.text => {}
                // DEFINE-10: another standard under the same identifier is
                // refused, never numbered.
                Some(_) => findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionDuplicateIdentifier,
                        &format!("codelists[{index}].standard.publishing_set"),
                        &format!("STD.{id}"),
                    )
                    .with("identifier", format!("STD.{id}")),
                ),
                None => standards.push(Standard {
                    id: id.clone(),
                    name: ct.name.text.clone(),
                    kind: "CT",
                    publishing_set: Some(ct.publishing_set.text.clone()),
                    version: ct.version.text.clone(),
                    status: "Final",
                }),
            }
            id
        });
        for list in &source.codelists {
            if !bound.contains(&list.id.name) {
                continue;
            }
            // DEFINE-4: a sponsor-extended item has no NCI code, so only the
            // published items carry one (CT-6 refuses an alias on the others).
            if section.context == DefineContext::Submission
                && source
                    .standard
                    .as_ref()
                    .is_some_and(|s| s.name.text == "CDISC/NCI")
                && list.items.as_ref().is_some_and(|items| {
                    list.alias.is_none() || items.iter().any(|i| !i.extended && i.alias.is_none())
                })
            {
                findings.push(
                    Finding::cause(
                        ConditionCode::SubmissionCdiscAliasMissing,
                        &format!("codelists.{}", list.id.name),
                        "alias",
                    )
                    .with("field", "alias"),
                );
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
        || format!("{}.{}", study.id.name, family),
        |m| m.id.name.clone(),
    );
    let metadata_name = section.metadata_version.as_ref().map_or_else(
        || format!("{} {} metadata", study.name.text, family),
        |m| m.name.text.clone(),
    );
    Ok(Study {
        study: study.clone(),
        section: section.clone(),
        file_oid: section.file_oid.as_ref().map_or_else(
            || {
                format!(
                    "{}.{}.Define-XML_{}",
                    study.id.name,
                    family,
                    section.define_version.family()
                )
            },
            |f| f.text.clone(),
        ),
        metadata_id,
        metadata_name,
        metadata_description: section
            .metadata_version
            .as_ref()
            .and_then(|m| m.description.as_ref())
            .map(|f| f.text.clone()),
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

    /// DEFINE-12: an authored source-system version overrides the version
    /// the calling package supplies as installed; neither may be empty.
    #[test]
    fn the_source_system_version_is_the_override_or_else_the_installed_package() {
        let text = |text: &str| ast::Text {
            text: text.into(),
            meta: ast::Meta {
                path: ast::Paths::new().root(),
                span: ast::Span::UNKNOWN,
            },
        };
        let mut section = ast::Section {
            family: Submission::Sdtm,
            specs: Vec::new(),
            standards: Vec::new(),
            define: None,
            define_version: ast::project::DefineVersion::default(),
            documents: Vec::new(),
            transport: None,
            context: DefineContext::Submission,
            xml_lang: text("en"),
            file_oid: None,
            metadata_version: None,
            originator: None,
            stylesheet: None,
            source_system_version: None,
            meta: text("").meta,
        };
        assert_eq!(source_version(&section, "0.3.0"), Some("0.3.0"));
        assert_eq!(source_version(&section, ""), None);
        section.source_system_version = Some(text("1.0"));
        assert_eq!(source_version(&section, "0.3.0"), Some("1.0"));
        assert_eq!(source_version(&section, ""), None);
        section.source_system_version = Some(text(""));
        assert_eq!(source_version(&section, "0.3.0"), None);
    }

    /// An engine refusal at a submission path names its step and closed
    /// code; a bound adds its resource facts, without an unknown size.
    #[test]
    fn engine_refusals_name_their_stage_code_and_bound() {
        use crate::json::Value as V;
        use crate::limits::{LimitExceeded, Limits};
        let text = |value: &str| V::String(value.into());
        let fault = Finding::fault(
            "define",
            issue::Step::Define,
            issue::Fault::OutputProjection,
        );
        assert_eq!(fault.code, ConditionCode::ApplicationRejected);
        assert_eq!(fault.path, "define");
        assert_eq!(
            fault.context,
            [
                ("code".into(), text("output_projection")),
                ("step".into(), text("define")),
            ]
            .into()
        );
        let rows = Limits::table(Limits::TABLE_ROWS + 1, 1).unwrap_err();
        let unsized_output = LimitExceeded {
            resource: "output_bytes",
            limit: Limits::OUTPUT_BYTES,
            actual: None,
        };
        for (bound, fault, actual) in [
            (
                rows,
                issue::Fault::ResourceLimit,
                Some(Limits::TABLE_ROWS + 1),
            ),
            (unsized_output, issue::Fault::OutputLimit, None),
        ] {
            let finding =
                Finding::limit("datasets.DM.transport", issue::Step::Define, fault, bound);
            assert_eq!(finding.code, ConditionCode::ApplicationRejected);
            assert_eq!(finding.path, "datasets.DM.transport");
            let mut expected: crate::diagnostic::Context = [
                ("code".into(), text(fault.as_str())),
                ("limit".into(), V::from(bound.limit)),
                ("resource".into(), text(bound.resource)),
                ("step".into(), text("define")),
            ]
            .into();
            if let Some(actual) = actual {
                expected.insert("actual".into(), V::from(actual));
            }
            assert_eq!(finding.context, expected);
        }
    }

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
