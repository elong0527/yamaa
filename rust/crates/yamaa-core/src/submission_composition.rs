//! Shared submission composition over admitted metadata. No study or host ports.
use crate::{
    application_issue as issue, ast,
    cdisc_vocabulary::{Core, DataType, DefineContext, ModelKind, OriginSource, OriginType},
    diagnostic::{ConditionCode, Definition},
    names::Positions,
    project_environment::Submission,
    project_terminology as ct,
    schema::{spec_path_step, Document, DocumentNode as N},
    specification::{OutputFinding, PreflightFinding},
    specification_submission_document as spec,
    submission_origin::{self as origin, Reads},
    submission_values as values,
};
use std::collections::BTreeSet;
mod analysis;
mod columns;
mod datasets;
mod reproduced;
mod value_entries;
pub use analysis::{AnalysisDataset, AnalysisDisplay, AnalysisResult, AnalysisTerm};
use columns::{declared_column, key_column_mandatory};
use datasets::compose_datasets;
pub use reproduced::mode_refused;
use value_entries::compose_values;

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
    /// The `ItemRef`'s `OrderNumber` reproduced metadata declares; the
    /// column's position in `output.columns` otherwise (DEFINE-18, DEFINE-30).
    pub(crate) order: Option<usize>,
    pub(crate) values: Vec<ValueDefinition>,
    /// The width a transport file gives a character column (SUB-24).
    pub(crate) transport_length: Option<usize>,
    /// The `ItemRef`'s `def:HasNoData`: measured from the published rows
    /// when a transport is requested (SUB-28), or declared in reproduced
    /// metadata (DEFINE-30).
    pub(crate) has_no_data: bool,
    /// The spec path of the column's `submission`, or of the value-level
    /// entry, where its declarations are reported.
    pub(crate) path: String,
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
/// the column's metadata merged with the entry's (SUB-12, DEFINE-27), or a
/// reproduced entry's own (DEFINE-30).
#[derive(Clone, Debug)]
pub struct ValueDefinition {
    pub(crate) id: String,
    /// Each where column's position in the dataset's columns and its
    /// comparison, in written order.
    pub(crate) clause: Vec<crate::dataset::WhereTerm>,
    pub(crate) path: String,
    pub(crate) column: Column,
    /// The received per-value `ItemDef` OID and `Name` and `WhereClauseDef`
    /// OID reproduced metadata declares (DEFINE-9, DEFINE-30).
    pub(crate) received: Received,
}
/// The identifiers and name a reproduced value-level entry keeps from its
/// received document; each is `None` where the document generates it.
#[derive(Clone, Debug, Default)]
pub struct Received {
    pub oid: Option<String>,
    pub name: Option<String>,
    pub where_oid: Option<String>,
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
    /// The listed specification's spec path (`adam.specs[0]`).
    pub(crate) path: String,
}
#[derive(Clone, Debug)]
pub struct Codelist {
    pub declaration: ast::Codelist,
    pub standard: Option<String>,
    /// Where the project declares it: `codelists[<source>].codelists.<id>`.
    pub path: String,
}
#[derive(Clone, Debug)]
pub struct Standard {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub publishing_set: Option<String>,
    pub version: String,
    pub status: &'static str,
    /// Where the project declares it: a section guide, or the `standard` of
    /// the codelist source that cites it.
    pub path: String,
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
    pub(crate) study_oid: String,
    pub(crate) metadata_oid: String,
    /// Whether the section lists a reproduced specification (DEFINE-30).
    pub(crate) reproduced: bool,
    pub(crate) metadata_name: String,
    pub(crate) metadata_description: Option<String>,
    pub(crate) source_version: String,
    pub(crate) installed_version: String,
    pub(crate) created: String,
    pub(crate) standards: Vec<Standard>,
    pub(crate) datasets: Vec<Dataset>,
    pub(crate) codelists: Vec<Codelist>,
    /// The result displays of the section's analysis results sources, in
    /// source and declaration order (DEFINE-32).
    pub(crate) analysis: Vec<AnalysisDisplay>,
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
    /// The `Study` OID Define-XML and Dataset-JSON carry: the section's, or
    /// `STDY.<study.id>` (DEFINE-9).
    pub fn study_oid(&self) -> &str {
        &self.study_oid
    }
    /// The `MetaDataVersion` OID Define-XML and Dataset-JSON carry: the
    /// section's received `metadata_version.oid`, or `MDV.<id>` (DEFINE-9).
    pub fn metadata_oid(&self) -> &str {
        &self.metadata_oid
    }
    /// Whether the section lists a reproduced specification, so the document
    /// may carry what only a received document records, such as a codelist
    /// item's `OrderNumber` of 0 (DEFINE-20, DEFINE-30).
    pub fn reproduced(&self) -> bool {
        self.reproduced
    }
    pub fn metadata_name(&self) -> &str {
        &self.metadata_name
    }
    pub fn metadata_description(&self) -> Option<&str> {
        self.metadata_description.as_deref()
    }
    /// The source-system name the documents carry: declared, or `yamaa`
    /// (DEFINE-12).
    pub fn source_system(&self) -> &str {
        self.section
            .source_system
            .as_ref()
            .map_or("yamaa", |name| name.text.as_str())
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
    /// The result displays the document's analysis results metadata holds,
    /// in source and declaration order (DEFINE-31, DEFINE-32).
    pub fn analysis_displays(&self) -> &[AnalysisDisplay] {
        &self.analysis
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
    /// The listed specification's spec path (`adam.specs[0]`).
    pub fn path(&self) -> &str {
        &self.path
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
    /// The spec path of the column's `submission`, or of the value-level
    /// entry, where its declarations are reported.
    pub fn path(&self) -> &str {
        &self.path
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
    /// The `OrderNumber` reproduced metadata declares for the column's
    /// `ItemRef`; `None` when the document numbers it by position
    /// (DEFINE-18, DEFINE-30).
    pub fn order(&self) -> Option<usize> {
        self.order
    }
    /// The width a transport file gives a character column, once measured
    /// (SUB-24).
    pub fn transport_length(&self) -> Option<usize> {
        self.transport_length
    }
    /// Whether the `ItemRef` says the column or entry has no data: no
    /// published row it describes holds a value (SUB-28), or reproduced
    /// metadata declares it (DEFINE-30).
    pub fn has_no_data(&self) -> bool {
        self.has_no_data
    }
    pub fn runtime_type(&self) -> crate::value::ColumnType {
        runtime_type(self.declaration.column_type.value)
    }
    pub fn values(&self) -> &[ValueDefinition] {
        &self.values
    }
}
impl ValueDefinition {
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Each where column's position in the dataset's columns and its
    /// comparison, in written order.
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
    /// The received identifiers and name the entry keeps (DEFINE-30).
    pub fn received(&self) -> &Received {
        &self.received
    }
    /// Whether the entry's where clause selects a row: `cell` reads the row's
    /// value of one where column, and every term must select it (SUB-12).
    pub(crate) fn selects<'r, E>(
        &self,
        mut cell: impl FnMut(usize) -> Result<crate::table::ValueRef<'r>, E>,
    ) -> Result<bool, E> {
        for term in &self.clause {
            if !term.selects(cell(term.column)?) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
/// The runtime type of a declared column type.
fn runtime_type(declared: spec::ColumnType) -> crate::value::ColumnType {
    use crate::value::ColumnType as T;
    match declared {
        spec::ColumnType::String => T::Str,
        spec::ColumnType::Integer => T::Int,
        spec::ColumnType::Float => T::Float,
        spec::ColumnType::Date => T::Date,
        spec::ColumnType::Datetime => T::DateTime,
        spec::ColumnType::Time => T::Time,
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
    unknown_documents(findings, references.iter().map(reference_id), known, path);
}
/// DEFINE-8: each document a declaration at `path` names is one the section
/// declares, whichever declaration names it: an origin, a method or a
/// comment of a specification, or an analysis result.
fn unknown_documents<'r>(
    findings: &mut Vec<Finding>,
    documents: impl IntoIterator<Item = &'r str>,
    known: &BTreeSet<&str>,
    path: &str,
) {
    for id in documents {
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
    reproduced: bool,
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
    /// The lists a dataset's columns are found in by name.
    names: &'a Names<'a>,
    reads: &'a Reads,
    /// The dataset's class decides whether `core` derives `mandatory` (SUB-10).
    tabulation: bool,
    /// The dataset's class, reported with a refused `datetime` column (SUB-27).
    class: &'static str,
}
/// A dataset's lists of columns, each looked up by name and never searched:
/// a dataset declares and lists as many columns as its file's bytes hold
/// (#4782).
struct Names<'a> {
    /// The columns the specification's tree declares.
    tree: Positions<'a>,
    /// The columns the dataset's document declares.
    declared: Positions<'a>,
    /// The entries of `output.columns`.
    output: Positions<'a>,
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
    /// The section's analysis results sources, inline and from the files it
    /// lists, in written order (DEFINE-32): none unless the caller read
    /// them ([`Required::with_analysis`]).
    pub analysis: &'a [ast::AnalysisResults],
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
                analysis: &[],
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
    /// These declarations with the analysis results sources the section
    /// declares, each lowered from the project file or from the file it
    /// lists (DEFINE-32).
    pub fn with_analysis(mut self, sources: &'a [ast::AnalysisResults]) -> Self {
        self.analysis = sources;
        self
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
        analysis: sources,
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
    // A section is reproduced once it lists a reproduced specification
    // (DEFINE-30).
    let section_reproduced = inputs.iter().any(reproduced::listed);
    reproduced::metadata_version_oid(section, section_reproduced, &mut findings);
    let datasets = compose_datasets(&composition, inputs, &mut findings, &mut bound);
    let analysis = analysis::compose(
        &analysis::Context {
            section: family,
            catalogue,
            known: &known,
            inputs,
            datasets: &datasets,
        },
        sources,
        &mut findings,
    );
    // The document lists every guide of the section in declaration order,
    // whether or not a dataset binds it, then the terminology standards
    // (DEFINE-6).
    let mut standards: Vec<_> = guides
        .iter()
        .enumerate()
        .map(|(index, guide)| Standard {
            path: format!("{family}.standards[{index}]"),
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
                    path: format!("codelists[{index}].standard"),
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
                path: format!(
                    "codelists[{index}].codelists{}",
                    spec_path_step(&list.id.name)
                ),
            });
        }
    }
    if !findings.is_empty() {
        return Err(findings);
    }
    let metadata_oid = section.metadata_version.as_ref().map_or_else(
        || format!("MDV.{}.{}", study.id.name, family),
        |m| {
            m.oid
                .as_ref()
                .map_or_else(|| format!("MDV.{}", m.id.name), |oid| oid.text.clone())
        },
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
        study_oid: section
            .study_oid
            .as_ref()
            .map_or_else(|| format!("STDY.{}", study.id.name), |oid| oid.text.clone()),
        metadata_oid,
        reproduced: section_reproduced,
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
        analysis,
        inventory,
    })
}

/// Refuse on validated metadata what only reproduced metadata declares
/// (DEFINE-30), each at its own key below `column_path`: a column left out
/// of the document, the ItemRef facts, which validated metadata measures
/// instead (SUB-28), and a received method or comment OID, which validated
/// metadata generates (DEFINE-9).
fn reproduced_only_fields(
    metadata: &spec::ColumnSubmission,
    column_path: &str,
    name: &str,
    findings: &mut Vec<Finding>,
) {
    let method_oid = metadata.method.as_ref().and_then(spec::Method::oid);
    let comment_oid = metadata.comment.as_ref().and_then(spec::Comment::oid);
    for (field, key, declared) in [
        ("omitted", "omitted", metadata.omitted.is_some()),
        ("order", "order", metadata.order.is_some()),
        (
            "key_sequence",
            "key_sequence",
            metadata.key_sequence.is_some(),
        ),
        ("has_no_data", "has_no_data", metadata.has_no_data.is_some()),
        ("method oid", "method.oid", method_oid.is_some()),
        ("comment oid", "comment.oid", comment_oid.is_some()),
    ] {
        if declared {
            findings.push(crate::define_diagnostics::xml_invalid(
                &format!("{column_path}.{key}"),
                &format!("reproduced {field} requires mode reproduced"),
                crate::json::Value::String(name.to_owned()),
            ));
        }
    }
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
            xml_lang_scope: ast::project::XmlLangScope::All,
            file_oid: None,
            study_oid: None,
            metadata_version: None,
            originator: None,
            stylesheet: None,
            source_system: None,
            source_system_version: None,
            analysis_results: Vec::new(),
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
        let rows = LimitExceeded {
            resource: "source_bytes",
            limit: Limits::SOURCE_BYTES,
            actual: Some(Limits::SOURCE_BYTES + 1),
        };
        let unsized_output = LimitExceeded {
            resource: "output_bytes",
            limit: Limits::OUTPUT_BYTES,
            actual: None,
        };
        for (bound, fault, actual) in [
            (
                rows,
                issue::Fault::ResourceLimit,
                Some(Limits::SOURCE_BYTES + 1),
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
