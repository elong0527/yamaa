//! Closed declarations and normalized occurrence facts; no written-file claim.
use crate::diagnostic::ScalarSubmissionRule;
use alloc::{boxed::Box, string::String, vec::Vec};

macro_rules! vocabulary {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn name(self) -> &'static str { match self { $(Self::$variant => $text),+ } }
            pub(super) const VALUES: &'static [(&'static str, Self)] = &[$(($text, Self::$variant)),+];
        }
    }
}
vocabulary!(FieldName {
    Columns => "columns", Rows => "rows", Id => "id", Submission => "submission", Name => "name", Type => "type", Label => "label",
    HasNoData => "has_no_data", Class => "class", Subclass => "subclass", Structure => "structure",
    Repeating => "repeating", ReferenceData => "reference_data", Domain => "domain", Comment => "comment",
    Core => "core", Mandatory => "mandatory", Role => "role", DataType => "data_type", Length => "length",
    SignificantDigits => "significant_digits", DisplayFormat => "display_format", Codelist => "codelist",
    InventoryVocabulary => "inventory_vocabulary", Origin => "origin", Method => "method", Source => "source",
    Description => "description", Documents => "documents", Expression => "expression", Text => "text",
    Context => "context", Code => "code", Document => "document", Pages => "pages", Refs => "refs", Title => "title"
});
vocabulary!(ColumnType { String => "str", Integer => "int", Float => "float", Date => "date", Datetime => "datetime" });
vocabulary!(Core { Required => "Req", Expected => "Exp", Permissible => "Perm", Conditional => "Cond" });
vocabulary!(OriginType { Assigned => "Assigned", Collected => "Collected", Derived => "Derived", NotAvailable => "Not Available", Other => "Other", Predecessor => "Predecessor", Protocol => "Protocol" });
vocabulary!(OriginSource { Investigator => "Investigator", Sponsor => "Sponsor", Subject => "Subject", Vendor => "Vendor" });
vocabulary!(MethodType { Computation => "Computation", Imputation => "Imputation" });
vocabulary!(PageType { Physical => "PhysicalRef", Named => "NamedDestination" });
vocabulary!(DataType {
    Text => "text", Integer => "integer", Float => "float", Date => "date", Datetime => "datetime", Time => "time",
    PartialDate => "partialDate", PartialTime => "partialTime", PartialDatetime => "partialDatetime",
    IncompleteDate => "incompleteDate", IncompleteTime => "incompleteTime", IncompleteDatetime => "incompleteDatetime",
    DurationDatetime => "durationDatetime", IntervalDatetime => "intervalDatetime", Uri => "URI"
});
vocabulary!(DatasetClass {
    AdamOther => "ADAM OTHER", BasicData => "BASIC DATA STRUCTURE", DeviceLevel => "DEVICE LEVEL ANALYSIS DATASET",
    Events => "EVENTS", Findings => "FINDINGS", FindingsAbout => "FINDINGS ABOUT", Interventions => "INTERVENTIONS",
    MedicalDeviceBasic => "MEDICAL DEVICE BASIC DATA STRUCTURE", MedicalDeviceOccurrence => "MEDICAL DEVICE OCCURRENCE DATA STRUCTURE",
    Occurrence => "OCCURRENCE DATA STRUCTURE", Reference => "REFERENCE DATA STRUCTURE", Relationship => "RELATIONSHIP",
    SpecialPurpose => "SPECIAL PURPOSE", StudyReference => "STUDY REFERENCE", SubjectLevel => "SUBJECT LEVEL ANALYSIS DATASET", TrialDesign => "TRIAL DESIGN"
});
vocabulary!(DatasetSubclass {
    AdverseEvent => "ADVERSE EVENT", MedicalDeviceTimeToEvent => "MEDICAL DEVICE TIME-TO-EVENT",
    NonCompartmental => "NON-COMPARTMENTAL ANALYSIS", PopulationPharmacokinetic => "POPULATION PHARMACOKINETIC ANALYSIS", TimeToEvent => "TIME-TO-EVENT"
});
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub container: usize,
    pub key: Option<usize>,
    pub node: Option<usize>,
    pub field: FieldName,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<T> {
    pub value: T,
    pub origin: Origin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Field(FieldName),
    Index(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    pub(super) steps: [Option<Step>; 10],
    pub(super) len: usize,
}
impl Path {
    pub fn steps(&self) -> impl Iterator<Item = Step> + '_ {
        self.steps[..self.len].iter().flatten().copied()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Missing,
    ExpectedMapping,
    ExpectedSequence,
    ExpectedText,
    ExpectedBoolean,
    ExpectedInteger,
    NonTextKey,
    UnknownField,
    EmptyText,
    InvalidIdentifier,
    InvalidChoice,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub node: usize,
    pub container: Option<usize>,
    pub key: Option<usize>,
    pub origin: Option<Origin>,
    pub path: Path,
    pub kind: Kind,
    pub(super) scalar_rule: Option<ScalarSubmissionRule>,
}
impl Finding {
    /// Scalar ownership is separate from the retained raw document shape facts.
    pub fn scalar_rule(&self) -> Option<ScalarSubmissionRule> {
        self.scalar_rule
    }
    /// Compatibility view of the original optional scalar citation.
    pub fn requirement(&self) -> Option<&'static str> {
        self.scalar_rule.map(ScalarSubmissionRule::requirement)
    }
}
/// Every class keeps its original normalized mapping order as occurrence facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dataset {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub has_no_data: Field<bool>,
    pub label: Field<String>,
    pub class: Field<DatasetClass>,
    pub subclass: Option<Field<DatasetSubclass>>,
    pub structure: Field<String>,
    pub repeating: Field<bool>,
    pub reference_data: Field<bool>,
    pub domain: Option<Field<String>>,
    pub comment: Option<Comment>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub node: usize,
    pub fields: Vec<Origin>,
    pub name: Field<String>,
    pub column_type: Field<ColumnType>,
    pub label: Option<Field<String>>,
    pub submission: Option<ColumnSubmission>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSubmission {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub core: Option<Field<Core>>,
    pub mandatory: Option<Field<bool>>,
    pub role: Option<Field<String>>,
    pub data_type: Option<Field<DataType>>,
    /// Canonical arbitrary-width decimal declarations; later semantics own range checks.
    pub length: Option<Field<String>>,
    pub significant_digits: Option<Field<String>>,
    pub display_format: Option<Field<String>>,
    pub codelist: Option<Field<String>>,
    pub inventory_vocabulary: Option<Field<bool>>,
    pub origin_metadata: Option<OriginMetadata>,
    pub method: Option<Method>,
    pub comment: Option<Comment>,
}
/// A row template's metadata overrides, independent of its executable expressions.
/// Mapping order and the written key/value occurrences remain available.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRow {
    pub node: usize,
    pub id: Field<String>,
    pub origin: Origin,
    pub overrides: Vec<ValueOverride>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueOverride {
    pub name: Field<String>,
    pub submission: ColumnSubmission,
}
impl ColumnSubmission {
    /// Missing override fields inherit the complete column declaration. Nested
    /// class values replace their counterpart; occurrence facts stay attached.
    pub fn inherit(&self, base: &Self) -> Self {
        let mut merged = self.clone();
        macro_rules! inherit {
            ($($field:ident),+ $(,)?) => { $(
                if merged.$field.is_none() { merged.$field = base.$field.clone(); }
            )+ };
        }
        inherit!(
            core,
            mandatory,
            role,
            data_type,
            length,
            significant_digits,
            display_format,
            codelist,
            inventory_vocabulary,
            origin_metadata,
            method,
            comment
        );
        merged
    }
    /// Declaration equality deliberately ignores where identical values occur.
    /// It compares normalized defaults as well as every nested metadata field.
    pub fn same_values(&self, other: &Self) -> bool {
        macro_rules! fields {
            ($($field:ident),+ $(,)?) => { true $(&& field_eq(&self.$field, &other.$field))+ };
        }
        fields!(
            core,
            mandatory,
            role,
            data_type,
            length,
            significant_digits,
            display_format,
            codelist,
            inventory_vocabulary
        ) && optional_eq(&self.origin_metadata, &other.origin_metadata, origin_eq)
            && optional_eq(&self.method, &other.method, method_eq)
            && optional_eq(&self.comment, &other.comment, comment_eq)
    }
}
fn optional_eq<T>(left: &Option<T>, right: &Option<T>, equal: impl Fn(&T, &T) -> bool) -> bool {
    match (left, right) {
        (Some(a), Some(b)) => equal(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn field_eq<T: PartialEq>(left: &Option<Field<T>>, right: &Option<Field<T>>) -> bool {
    optional_eq(left, right, |a, b| a.value == b.value)
}
fn documents_eq(left: &[DocumentReference], right: &[DocumentReference]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| match (a, b) {
            (DocumentReference::Plain(a), DocumentReference::Plain(b)) => a.value == b.value,
            (DocumentReference::Class(a), DocumentReference::Class(b)) => {
                a.document.value == b.document.value
                    && optional_eq(&a.pages, &b.pages, |a, b| {
                        a.kind.value == b.kind.value
                            && a.refs.value == b.refs.value
                            && field_eq(&a.title, &b.title)
                    })
            }
            _ => false,
        })
}
fn origin_eq(a: &OriginMetadata, b: &OriginMetadata) -> bool {
    a.kind.value == b.kind.value
        && field_eq(&a.source, &b.source)
        && field_eq(&a.description, &b.description)
        && documents_eq(&a.documents.value, &b.documents.value)
}
fn method_eq(a: &Method, b: &Method) -> bool {
    match (a, b) {
        (Method::Plain(a), Method::Plain(b)) => a.value == b.value,
        (Method::Class(a), Method::Class(b)) => {
            field_eq(&a.name, &b.name)
                && a.kind.value == b.kind.value
                && a.description.value == b.description.value
                && optional_eq(&a.expression, &b.expression, |a, b| {
                    a.context.value == b.context.value && a.code.value == b.code.value
                })
                && documents_eq(&a.documents.value, &b.documents.value)
        }
        _ => false,
    }
}
fn comment_eq(a: &Comment, b: &Comment) -> bool {
    match (a, b) {
        (Comment::Plain(a), Comment::Plain(b)) => a.value == b.value,
        (Comment::Class(a), Comment::Class(b)) => {
            a.text.value == b.text.value && documents_eq(&a.documents.value, &b.documents.value)
        }
        _ => false,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginMetadata {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub kind: Field<OriginType>,
    pub source: Option<Field<OriginSource>>,
    pub description: Option<Field<String>>,
    pub documents: Field<Vec<DocumentReference>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    Plain(Field<String>),
    Class(Box<MethodClass>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodClass {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub name: Option<Field<String>>,
    pub kind: Field<MethodType>,
    pub description: Field<String>,
    pub expression: Option<FormalExpression>,
    pub documents: Field<Vec<DocumentReference>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormalExpression {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub context: Field<String>,
    pub code: Field<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comment {
    Plain(Field<String>),
    Class(CommentClass),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentClass {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub text: Field<String>,
    pub documents: Field<Vec<DocumentReference>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentReference {
    Plain(Field<String>),
    Class(Box<DocumentReferenceClass>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentReferenceClass {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub document: Field<String>,
    pub pages: Option<Pages>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pages {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub kind: Field<PageType>,
    pub refs: Field<String>,
    pub title: Option<Field<String>>,
}
/// Only complete bounded decoding can construct this metadata owner. It is not
/// semantic submission admission or a file plan; no execution capability exists.
#[derive(Debug, PartialEq, Eq)]
pub struct Declarations {
    pub(super) root: usize,
    pub(super) fields: Vec<Origin>,
    pub(super) dataset: Option<Dataset>,
    pub(super) columns: Option<Field<Vec<Column>>>,
    pub(super) value_rows: Vec<ValueRow>,
}
impl Declarations {
    pub fn root(&self) -> usize {
        self.root
    }
    pub fn fields(&self) -> &[Origin] {
        &self.fields
    }
    pub fn dataset(&self) -> Option<&Dataset> {
        self.dataset.as_ref()
    }
    pub fn columns(&self) -> Option<&Field<Vec<Column>>> {
        self.columns.as_ref()
    }
    pub fn value_rows(&self) -> &[ValueRow] {
        &self.value_rows
    }
}
