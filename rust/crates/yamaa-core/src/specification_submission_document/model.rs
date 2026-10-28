//! Closed declarations and normalized occurrence facts; no written-file claim.
use crate::cdisc_vocabulary::{
    Core, DataType, DatasetClass, DatasetSubclass, MethodType, ModelKind, OriginSource, OriginType,
    PageType,
};
use crate::diagnostic::ScalarSubmissionRule;

macro_rules! vocabulary {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn name(self) -> &'static str { match self { $(Self::$variant => $text),+ } }
            pub(super) const VALUES: &'static [(&'static str, Self)] = &[$(($text, Self::$variant)),+];
            /// Every spelling, in declaration order.
            pub const NAMES: &'static [&'static str] = &[$($text),+];
        }
    }
}
vocabulary!(FieldName {
    Columns => "columns", Id => "id", Submission => "submission", Name => "name", Type => "type", Label => "label",
    HasNoData => "has_no_data", Class => "class", Subclass => "subclass", Structure => "structure",
    Repeating => "repeating", ReferenceData => "reference_data", Domain => "domain", Comment => "comment",
    Core => "core", Mandatory => "mandatory", Role => "role", DataType => "data_type", Length => "length",
    SignificantDigits => "significant_digits", DisplayFormat => "display_format", Codelist => "codelist",
    InventoryVocabulary => "inventory_vocabulary", Origin => "origin", Method => "method", Source => "source",
    Description => "description", Documents => "documents", Expression => "expression", Text => "text",
    Context => "context", Code => "code", Document => "document", Pages => "pages", Refs => "refs", Title => "title",
    ValueLevel => "value_level", Where => "where", Origins => "origins", Standard => "standard",
    Mode => "mode", Artifact => "artifact", KeySequence => "key_sequence",
    NonStandard => "non_standard"
});
vocabulary!(SubmissionMode { Validated => "validated", Historical => "historical" });
/// Governed fields share one vocabulary in declaration and execution admission.
/// These are the dataset fields a specification declares, `standard` among
/// them (P-STD-1), which its root `metadata` map must not carry (SUB-2).
pub const DATASET_METADATA_FIELDS: &[FieldName] = &[
    FieldName::Mode,
    FieldName::Artifact,
    FieldName::HasNoData,
    FieldName::Label,
    FieldName::Class,
    FieldName::Subclass,
    FieldName::Structure,
    FieldName::Repeating,
    FieldName::ReferenceData,
    FieldName::Domain,
    FieldName::Standard,
    FieldName::NonStandard,
    FieldName::Comment,
];
pub const COLUMN_METADATA_FIELDS: &[FieldName] = &[
    FieldName::KeySequence,
    FieldName::HasNoData,
    FieldName::Core,
    FieldName::Mandatory,
    FieldName::Role,
    FieldName::DataType,
    FieldName::Length,
    FieldName::SignificantDigits,
    FieldName::DisplayFormat,
    FieldName::Codelist,
    FieldName::InventoryVocabulary,
    FieldName::Origin,
    FieldName::Method,
    FieldName::Comment,
    FieldName::ValueLevel,
];
/// The fields of one value-level entry: its identity, its where clause, its
/// description and every column metadata field except `value_level`.
pub const VALUE_ENTRY_FIELDS: &[FieldName] = &[
    FieldName::KeySequence,
    FieldName::HasNoData,
    FieldName::Id,
    FieldName::Where,
    FieldName::Label,
    FieldName::Core,
    FieldName::Mandatory,
    FieldName::Role,
    FieldName::DataType,
    FieldName::Length,
    FieldName::SignificantDigits,
    FieldName::DisplayFormat,
    FieldName::Codelist,
    FieldName::InventoryVocabulary,
    FieldName::Origin,
    FieldName::Method,
    FieldName::Comment,
];
vocabulary!(ColumnType { String => "str", Integer => "int", Float => "float", Date => "date", Datetime => "datetime" });
impl ColumnType {
    pub fn default_data_type(self) -> DataType {
        match self {
            Self::String => DataType::Text,
            Self::Integer => DataType::Integer,
            Self::Float => DataType::Float,
            Self::Date => DataType::Date,
            Self::Datetime => DataType::Datetime,
        }
    }
    /// Admission on the specification alone, before a document names the
    /// family: a date or datetime admits its own type and the `integer` an
    /// `adam` document submits it as (SUB-6, SUB-25).
    pub fn admits_data_type(self, data_type: DataType) -> bool {
        match self {
            Self::String => !matches!(data_type, DataType::Integer | DataType::Float),
            Self::Date | Self::Datetime => {
                data_type == self.default_data_type() || data_type == DataType::Integer
            }
            Self::Integer | Self::Float => data_type == self.default_data_type(),
        }
    }
    /// Whether the column type is `date` or `datetime`.
    pub fn temporal(self) -> bool {
        matches!(self, Self::Date | Self::Datetime)
    }
    /// The display format of a date or datetime an `adam` document submits as
    /// a SAS numeric date, or None when the column keeps its own form (SUB-25).
    pub fn numeric_date_format(self, adam: bool) -> Option<&'static str> {
        match self {
            Self::Date if adam => Some("DATE9."),
            Self::Datetime if adam => Some("DATETIME20."),
            _ => None,
        }
    }
    /// The default submission data type once the family is known.
    pub fn family_data_type(self, adam: bool) -> DataType {
        if self.numeric_date_format(adam).is_some() {
            DataType::Integer
        } else {
            self.default_data_type()
        }
    }
    /// Admission once the family is known: a date or datetime admits exactly
    /// the one type its family submits it as.
    pub fn admits_family_data_type(self, data_type: DataType, adam: bool) -> bool {
        if self.temporal() {
            data_type == self.family_data_type(adam)
        } else {
            self.admits_data_type(data_type)
        }
    }
}
/// Both standalone and selected-study admission apply the same type vocabulary.
pub fn admits_data_type_names(column: &str, data_type: &str) -> bool {
    let column = ColumnType::VALUES.iter().find(|(name, _)| *name == column);
    let data_type = DataType::VALUES.iter().find(|(name, _)| *name == data_type);
    matches!((column, data_type), (Some((_, column)), Some((_, data_type))) if column.admits_data_type(*data_type))
}
/// [`admits_data_type_names`] once a specification declares its model: a date
/// or datetime admits exactly the one type the model submits it as (SUB-6,
/// SUB-25).
pub fn admits_model_data_type_names(column: &str, data_type: &str, adam: bool) -> bool {
    let column = ColumnType::VALUES.iter().find(|(name, _)| *name == column);
    let data_type = DataType::VALUES.iter().find(|(name, _)| *name == data_type);
    matches!((column, data_type), (Some((_, column)), Some((_, data_type))) if column.admits_family_data_type(*data_type, adam))
}
/// The display format a column of type `column` is submitted with under
/// ADaM, a SAS numeric date's, or None when it keeps its own (SUB-25).
pub fn numeric_date_format_name(column: &str, adam: bool) -> Option<&'static str> {
    ColumnType::VALUES
        .iter()
        .find(|(name, _)| *name == column)
        .and_then(|(_, column)| column.numeric_date_format(adam))
}
impl DatasetClass {
    /// The classes the tabulation families (`sdtm`, `send`) publish. Every other
    /// class is an analysis class, where `core` never derives `mandatory`.
    pub fn tabulation(self) -> bool {
        matches!(
            self,
            Self::Events
                | Self::Findings
                | Self::FindingsAbout
                | Self::Interventions
                | Self::Relationship
                | Self::SpecialPurpose
                | Self::StudyReference
                | Self::TrialDesign
        )
    }
    /// Read a written class name; a name outside the vocabulary is not tabulation.
    pub fn tabulation_name(name: &str) -> bool {
        Self::VALUES
            .iter()
            .any(|(text, class)| *text == name && class.tabulation())
    }
}
impl Core {
    /// Whether `model` publishes this designation (SUB-10): `Req`, `Exp` and
    /// `Perm` for SDTM and SEND, and `Req`, `Cond` and `Perm` for ADaM.
    pub fn admitted_by(self, model: ModelKind) -> bool {
        match self {
            Self::Exp => model != ModelKind::ADaM,
            Self::Cond => model == ModelKind::ADaM,
            Self::Req | Self::Perm => true,
        }
    }
}
impl OriginType {
    /// Every origin type, in the vocabulary's published order.
    pub fn all() -> impl Iterator<Item = Self> {
        Self::VALUES.iter().map(|(_, kind)| *kind)
    }
    /// Whether `model` admits this type with `source` (SUB-15): SDTM by its
    /// table of pairs, which has no `Not Available` (D65), SEND without a
    /// source or with a pair SDTM admits, and ADaM four types whose source is
    /// derived, never declared.
    pub fn admits_source(self, model: ModelKind, source: Option<OriginSource>) -> bool {
        use ModelKind as M;
        use OriginSource as S;
        match model {
            M::Sdtm => match self {
                Self::Collected => matches!(source, Some(S::Subject | S::Investigator | S::Vendor)),
                Self::Assigned | Self::Derived => matches!(source, Some(S::Vendor | S::Sponsor)),
                Self::Protocol => source == Some(S::Sponsor),
                Self::Other => source.is_some(),
                Self::Predecessor => source.is_none(),
                Self::NotAvailable => false,
            },
            M::Send => source.is_none() || self.admits_source(M::Sdtm, source),
            M::ADaM => {
                source.is_none()
                    && matches!(
                        self,
                        Self::Derived | Self::Assigned | Self::Predecessor | Self::Other
                    )
            }
        }
    }
}
/// The one `mandatory` resolution shared by the domain build and composition: a
/// key column is mandatory; otherwise a declared value wins, and without one a
/// tabulation class gives true for `Req` and false for every other designation.
pub fn resolve_mandatory(
    key: bool,
    declared: Option<bool>,
    core: Option<Core>,
    tabulation: bool,
) -> bool {
    key || declared.unwrap_or(tabulation && core == Some(Core::Req))
}
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
    pub(super) steps: [Option<Step>; 11],
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
    pub fn rule(&self) -> Option<&'static str> {
        self.scalar_rule.map(ScalarSubmissionRule::rule)
    }
}
/// Every class keeps its original normalized mapping order as occurrence facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dataset {
    pub mode: Field<SubmissionMode>,
    pub artifact: Option<Field<String>>,
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
    /// The name of the section standard that binds the dataset; the
    /// section's first when omitted (P-STD-1, DEFINE-8). No 0.5 form.
    pub standard: Option<Field<String>>,
    /// No implementation guide defines the dataset, so it binds none
    /// (SUB-4, DEFINE-6).
    pub non_standard: Field<bool>,
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
    pub key_sequence: Option<Field<String>>,
    /// A historical ItemRef's declared `def:HasNoData`; validated metadata
    /// measures it from the published rows instead (SUB-28, DEFINE-30).
    pub has_no_data: Option<Field<bool>>,
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
    /// The declared origins, in declaration order: 0.5's one `origin`, or
    /// each entry of 1.0's `origins` (P-ORIGIN-1). Empty when none is
    /// declared.
    pub origins: Vec<OriginMetadata>,
    pub method: Option<Method>,
    pub comment: Option<Comment>,
    /// The column's value-level entries in declaration order; always empty for
    /// an entry's own metadata.
    pub value_level: Vec<ValueEntry>,
}
/// One value-level entry: its metadata overrides the column's on the rows its
/// where clause selects. Mapping order and written occurrences remain available.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueEntry {
    pub origin: Origin,
    pub fields: Vec<Origin>,
    pub id: Field<String>,
    /// The where clause: every listed column holds one of its values, or
    /// under `NE` and `NOTIN` none of them (SUB-12).
    pub clause: Vec<WhereTerm>,
    /// The description of the value-level definition, when declared.
    pub label: Option<Field<String>>,
    pub metadata: ColumnSubmission,
}
/// One column of a where clause, its comparator and the values it lists, in
/// written order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhereTerm {
    pub column: Field<String>,
    pub comparator: crate::ast::Comparator,
    pub values: Vec<Field<String>>,
}
impl ColumnSubmission {
    /// Missing entry fields inherit the complete column declaration. Nested
    /// class values replace their counterpart, and an entry without origins of
    /// its own takes the column's whole list (DEFINE-27); occurrence facts stay
    /// attached. A historical ItemRef's `key_sequence` and `has_no_data` are
    /// its own and never inherit (DEFINE-30). The merged metadata describes
    /// one entry, so it carries no entries.
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
            method,
            comment
        );
        if merged.origins.is_empty() {
            merged.origins = base.origins.clone();
        }
        merged.value_level = Vec::new();
        merged
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
}
