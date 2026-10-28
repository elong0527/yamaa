//! The typed specification tree that the front end lowers into, and the
//! roots of a project's own files ([`project`]).
//!
//! A specification is read in 1.0 only: [`crate::front_end::v1::spec`]
//! lowers it into these nodes through the construct lowering in
//! [`crate::front_end::lowering`]. Every analysis reads them through the
//! one traversal in [`traversal`] and types them with the one result-type
//! table, [`result_type`]. There is no IO here and there are no YAML types:
//! positions arrive as plain numbers.
//!
//! Every node carries a [`Meta`]: its structured spec path, which one
//! renderer writes in the notation of P-CLR-2, and the position of the
//! deepest written node in the layer whose value took effect (P-ISSUE-1).

mod expr;
mod path;
pub mod project;
pub mod traversal;
mod types;

pub use crate::expression_table::{
    Closed, DayRule, FlagPart, Handlers, ImputeDay, RankMethod, ScalarOp, Slot, TemporalType,
    TextCase, TimeRule, TimeUnit, WindowFunction,
};
pub use expr::{
    Aggregate, Arm, Between, Binding, CallShape, Dictionary, Direction, Expr, ExprId, ExprKind,
    Formula, Keep, Literal, Name, Nulls, OdmRead, Operand, OrderTerm, SourceRead, Window,
};
pub use path::{PathId, Paths, Segment};
pub use project::{
    Codelist, CodelistDeclaration, CodelistItem, CodelistSource, CodelistType, Codelists,
    DocumentKind, ExternalDictionary, Function, FunctionDeclaration, FunctionTest, HostEnvironment,
    Implementation, ImplementationGuide, MetadataVersion, Parameter, Project, Section, Study,
    SubmissionDeclarations, SubmissionRefused, SupportingDocument, TerminologyStandard, Transport,
};
pub use traversal::{Children, Filtered, Ordered, Owner, Read, Role, Scope, Site, Step};
pub use types::{result_type, Table, TypeEnv};

use crate::cdisc_vocabulary::{ModelKind, WhereComparator};
use crate::value::ColumnType;

/// Where a node is, in the spec and in its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub path: PathId,
    pub span: Span,
}

/// A one-based position in one of the spec's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub line: u32,
    pub column: u32,
}

impl Span {
    /// No position was supplied. Lines and columns are one-based, so the
    /// zero position is never a written one.
    pub const UNKNOWN: Self = Self {
        file: FileId(0),
        line: 0,
        column: 0,
    };
    pub fn is_known(self) -> bool {
        self.line != 0
    }
}

/// An index into [`Spec::files`], the spec's layer list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// One file of a spec's layer list, by the identity its adapter captured;
/// the adapter renders the absolute path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub identity: String,
}

/// The optional fields a declaration writes with a value, in written order,
/// as its surface spells them. A field counts whatever the value: an empty
/// list or map, or `false`. The typed fields keep none of those apart from
/// an omitted field, and the checks that count a written field read this
/// (LOOKUP-4, LOOKUP-22, NAME-8, ROW-2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Written(pub Vec<&'static str>);

impl Written {
    /// Whether the declaration writes `field`.
    pub fn contains(&self, field: &str) -> bool {
        self.0.contains(&field)
    }
    /// The fields written, in written order.
    pub fn fields(&self) -> &[&'static str] {
        &self.0
    }
}

/// A declared identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub name: String,
    pub meta: Meta,
}

/// Written text: a path, a label, a description, a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub meta: Meta,
}

/// A written value outside an expression: a handler, a flag value, a
/// dictionary entry, a check bound. Omitted is `None` where the field is
/// optional; YAML null is [`Literal::Missing`].
#[derive(Clone, Debug, PartialEq)]
pub struct Constant {
    pub value: Literal,
    pub meta: Meta,
}

/// One root per file type; see [`project`] for the project, codelist and
/// function roots.
#[derive(Clone, Debug, PartialEq)]
pub enum File {
    Spec(Box<Spec>),
    Project(Box<Project>),
    Codelists(Box<Codelists>),
    Function(Box<Function>),
}

/// The version a file declares.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// The file's `yamaa`, which a resolved specification writes as its
    /// chain's version (P-VER-1).
    pub version: Text,
    pub meta: Meta,
}

/// `model: SDTM | SEND | ADaM` (P-MODEL-1); no 0.5 form.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub kind: ModelKind,
    pub meta: Meta,
}

/// One resolved specification: composed, pruned and admitted.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub header: Header,
    pub model: Option<Model>,
    pub domain: Named,
    pub keys: Vec<Named>,
    pub inputs: Vec<Input>,
    pub rows: Rows,
    pub intermediates: Vec<Intermediate>,
    /// Copy entries already expanded (P-COPY-1).
    pub columns: Vec<Column>,
    pub checks: Vec<DatasetCheck>,
    pub output: Output,
    pub submission: Option<DatasetSubmission>,
    pub metadata: Vec<(Named, Text)>,
    pub exprs: Vec<Expr>,
    pub paths: Paths,
    pub files: Vec<FileRef>,
    /// Of `parent`, which composition consumes: a spec that still writes
    /// one was not composed.
    pub written: Written,
    pub meta: Meta,
}

impl Spec {
    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id.index()]
    }
    /// Write a node's path in the notation of P-CLR-2.
    pub fn render(&self, path: PathId) -> String {
        self.paths.render(path)
    }
    /// A node's path as this spec's diagnostics cite it.
    pub fn cite(&self, meta: Meta) -> String {
        self.render(meta.path)
    }
    /// Whether the intermediate `name` drives a row template (ROW-2).
    pub fn drives_rows(&self, name: &str) -> bool {
        self.rows.templates.iter().any(|template| {
            template
                .driver
                .as_ref()
                .is_some_and(|driver| driver.name == name)
        })
    }
    /// Whether the intermediate `name` is a side of a pair relation, whose
    /// records it reads as a row template reads a driver's (LOOKUP-27).
    pub fn pairs_with(&self, name: &str) -> bool {
        self.intermediates.iter().any(|item| {
            item.inner_join
                .as_ref()
                .is_some_and(|right| right.name == name || item.dataset.name == name)
        })
    }
    /// Whether the intermediate `item` lacks the `key` it requires: it
    /// declares none and neither drives a row template nor is a side of a
    /// pair relation (LOOKUP-3, ROW-2, LOOKUP-27; D23, D63), or it is a pair
    /// relation, whose `key` pairs its two sides whatever reads it.
    pub fn lacks_key(&self, item: &Intermediate) -> bool {
        let name = &item.id.name;
        item.key.is_none() && (item.pairs() || !(self.drives_rows(name) || self.pairs_with(name)))
    }
    /// The pair relation `name` declares, if it is one (LOOKUP-27).
    pub fn pair_relation(&self, name: &str) -> Option<&Intermediate> {
        self.intermediates
            .iter()
            .find(|item| item.id.name == name && item.pairs())
    }
    /// The input `name`'s declared `key`, if it declares one (P-JOIN-1).
    pub fn input_key(&self, name: &str) -> Option<&[Named]> {
        self.inputs
            .iter()
            .rfind(|input| input.name.name == name)
            .and_then(|input| match &input.key {
                KeyDecl::Declared(columns) => Some(columns.as_slice()),
                KeyDecl::Undeclared => None,
            })
    }
    /// Whether the intermediate `item` matches its input on exactly the
    /// input's declared `key`: its own `key` lists the same columns, in any
    /// order, so it matches as the input's implicit join does (LOOKUP-4,
    /// D71).
    pub fn keys_as_input(&self, item: &Intermediate) -> bool {
        fn names(columns: &[Named]) -> std::collections::BTreeSet<&str> {
            columns.iter().map(|column| column.name.as_str()).collect()
        }
        let Some(JoinKey::Columns { columns, .. }) = &item.key else {
            return false;
        };
        self.input_key(&item.dataset.name)
            .is_some_and(|declared| names(declared) == names(columns))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub name: Named,
    pub source: InputSource,
    pub key: KeyDecl,
    pub require_match: bool,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputSource {
    pub path: Text,
    pub relative_to: PathBase,
    /// Selected by the path's extension; `None` when no profile matches.
    pub profile: Option<Profile>,
    pub types: Vec<(Named, ColumnType)>,
    pub encoding: Option<Text>,
    /// A 1.0 input's `byte_order_mark: true`: the csv source begins with one
    /// UTF-8 byte-order mark, which is skipped (CSV-10). 0.5 has no such
    /// field.
    pub byte_order_mark: bool,
    pub schema: Option<Text>,
    pub empty_string: EmptyString,
    pub ordinal: Option<Named>,
    /// Of `types`, written even when it declares no field (NAME-8), and of
    /// `byte_order_mark`, written whatever its value (CSV-10).
    pub written: Written,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathBase {
    Declaring,
    Entry,
}

/// The storage profile, including the ODM XML profile of P-ODMIN-1, which
/// has no 0.5 form, and the two Dataset-JSON profiles (DSJSON-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Csv,
    Parquet,
    Xport,
    Sas7bdat,
    OdmXml,
    DatasetJson,
    DatasetNdjson,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyString {
    Missing,
    Present,
}

/// An input's join key (P-JOIN-1).
#[derive(Clone, Debug, PartialEq)]
pub enum KeyDecl {
    Declared(Vec<Named>),
    Undeclared,
}

/// How output rows are built. A spec writes either `base` and `filter` or
/// `rows`; a spec that writes `filter` beside `rows` keeps both here, so
/// that the refusal can name them.
#[derive(Clone, Debug, PartialEq)]
pub struct Rows {
    pub base: Option<Named>,
    pub filter: Option<ExprId>,
    pub templates: Vec<Template>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub id: Named,
    pub driver: Option<Named>,
    pub group_by: Option<Vec<ExprId>>,
    pub filter: Option<ExprId>,
    pub overrides: Vec<(Named, Derivation)>,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Intermediate {
    pub id: Named,
    pub dataset: Named,
    /// The right side of a pair relation, whose `dataset` is the left side
    /// (LOOKUP-27).
    pub inner_join: Option<Named>,
    pub key: Option<JoinKey>,
    pub between: Option<Between>,
    pub filter: Option<ExprId>,
    pub order_by: Vec<OrderTerm>,
    pub keep: Option<Keep>,
    /// The selection by distance that replaces `order_by` and `keep`
    /// (LOOKUP-26).
    pub nearest: Option<Nearest>,
    pub columns: Option<Vec<Named>>,
    /// `absent`, the answer when the lookup selects no record (P-HND-1); it
    /// answers no variable that does not exist where it is read (D67).
    pub absent: Option<Constant>,
    pub require_match: bool,
    pub bindings: Vec<Binding>,
    pub checks: Vec<UniqueCheck>,
    /// Of `inner_join`, `key`, `between`, `filter`, `order_by`, `keep`,
    /// `nearest`, `columns`, `derivations`, `verifications`, the absence
    /// answer and `require_match`.
    pub written: Written,
    pub meta: Meta,
}

impl Intermediate {
    /// Whether the intermediate is a pair relation: it writes `inner_join`,
    /// so its records are the matching pairs of its two sides (LOOKUP-27).
    pub fn pairs(&self) -> bool {
        self.inner_join.is_some()
    }
    /// The current-row values the lookup reads before any record: each key
    /// pair's match value, then the `to` of its nearest selection, which is
    /// read as a key match value is (LOOKUP-5, LOOKUP-26).
    pub fn match_values(&self) -> impl Iterator<Item = ExprId> + '_ {
        let pairs = match &self.key {
            Some(JoinKey::Pairs { pairs, .. }) => pairs.as_slice(),
            _ => &[],
        };
        pairs
            .iter()
            .map(|&(_, value)| value)
            .chain(self.nearest.as_ref().map(|nearest| nearest.to))
    }
}

/// `nearest: {source, to, unit}` (LOOKUP-26): of the candidates, the one
/// whose date or datetime `source` lies nearest the current row's `to`,
/// counted in `unit` as `diff` counts it (TIME-4).
#[derive(Clone, Debug, PartialEq)]
pub struct Nearest {
    /// The donor field, stored or derived, written bare.
    pub source: Named,
    /// The current-row value, read as a key match value is (LOOKUP-5).
    pub to: ExprId,
    /// The `to` field itself, where a name written alone is read, as a key
    /// pair's meta is for its value.
    pub to_field: Meta,
    /// The unit as written; `None` when the schema refused it.
    pub unit: Option<TimeUnit>,
    pub meta: Meta,
}

/// A lookup's key.
#[derive(Clone, Debug, PartialEq)]
pub enum JoinKey {
    /// Donor columns, each paired with the current row's same-named value.
    Columns { columns: Vec<Named>, meta: Meta },
    /// Donor columns, each paired with a current-row expression.
    Pairs {
        pairs: Vec<(Named, ExprId)>,
        meta: Meta,
    },
    /// The input's declared key (P-KEY-1).
    FromInput,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UniqueCheck {
    pub columns: Vec<Named>,
    pub id: Option<Text>,
    /// `warning` is permitted only beside `keep` and `order_by` (LOOKUP-17).
    pub severity: Severity,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: Named,
    pub kind: ColumnType,
    pub label: Option<Text>,
    pub derivation: Option<Derivation>,
    pub origin: ColumnOrigin,
    pub checks: Vec<ColumnCheck>,
    pub submission: Option<ColumnSubmission>,
    pub metadata: Vec<(Named, Text)>,
    pub meta: Meta,
}

/// Where a column's declaration comes from (P-COPY-1).
#[derive(Clone, Debug, PartialEq)]
pub enum ColumnOrigin {
    Declared,
    /// No 0.5 form.
    Copied {
        dataset: Named,
        field: Named,
    },
}

/// How a value is produced, with its conversion answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Derivation {
    pub expr: ExprId,
    pub unconvertible: Option<Constant>,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnCheck {
    pub kind: ColumnCheckKind,
    pub id: Option<Text>,
    pub severity: Severity,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColumnCheckKind {
    /// `when` selects the rows it checks, and `max_fraction` the share of
    /// them that may be missing (CHECK-5).
    NotMissing {
        when: Option<ExprId>,
        max_fraction: Option<Constant>,
    },
    AllowedValues(Vec<Constant>),
    Range {
        min: Option<Constant>,
        max: Option<Constant>,
    },
    MaxLength(Constant),
    Matches(Text),
    /// A check outside the language; see [`ExprKind::Unsupported`].
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DatasetCheck {
    pub kind: DatasetCheckKind,
    pub id: Option<Text>,
    pub severity: Severity,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DatasetCheckKind {
    Unique {
        columns: Vec<Named>,
    },
    AllOrNone {
        columns: Vec<Named>,
    },
    Assert {
        when: Option<ExprId>,
        require: ExprId,
    },
    RowCount(Box<RowCount>),
    /// A check outside the language; see [`ExprKind::Unsupported`].
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowCount {
    /// `None` when omitted; a written empty list is refused.
    pub group_by: Option<Vec<Named>>,
    pub filter: Option<ExprId>,
    pub when: Option<ExprId>,
    pub min: Option<Constant>,
    pub max: Option<Constant>,
    pub min_fraction: Option<Constant>,
    pub max_fraction: Option<Constant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub path: Text,
    pub relative_to: PathBase,
    pub decimals: Option<Constant>,
    pub columns: Vec<Named>,
    pub report: Option<Text>,
    pub order_by: Vec<OrderTerm>,
    pub meta: Meta,
}

/// Governed dataset metadata. Closed CDISC vocabularies stay text: their
/// values are admitted by the schema (P-VOC-1).
#[derive(Clone, Debug, PartialEq)]
pub struct DatasetSubmission {
    pub mode: crate::specification_submission_document::SubmissionMode,
    pub artifact: Option<Text>,
    pub label: Text,
    pub class: Text,
    pub subclass: Option<Text>,
    pub structure: Text,
    pub repeating: bool,
    pub reference_data: bool,
    pub has_no_data: bool,
    pub domain: Option<Named>,
    pub comment: Option<Comment>,
    /// The section standard the spec selects (P-STD-1); no 0.5 form.
    pub standard: Option<Named>,
    /// No implementation guide defines the dataset (SUB-4, DEFINE-6).
    pub non_standard: bool,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnSubmission {
    pub metadata: SubmissionMetadata,
    pub value_level: Vec<ValueLevel>,
    pub meta: Meta,
}

/// The metadata a column and each of its value-level entries share.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubmissionMetadata {
    pub key_sequence: Option<Constant>,
    /// A historical ItemRef's declared `def:HasNoData` (DEFINE-30).
    pub has_no_data: Option<bool>,
    pub core: Option<Text>,
    pub mandatory: Option<bool>,
    pub role: Option<Text>,
    pub data_type: Option<Text>,
    pub length: Option<Constant>,
    pub significant_digits: Option<Constant>,
    pub display_format: Option<Text>,
    pub codelist: Option<Named>,
    pub inventory_vocabulary: Option<bool>,
    /// Always a list (P-ORIGIN-1); today's single `origin` is one entry.
    pub origins: Vec<Origin>,
    pub method: Option<Method>,
    pub comment: Option<Comment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueLevel {
    pub id: Named,
    pub conditions: Vec<WhereClause>,
    pub label: Option<Text>,
    pub metadata: SubmissionMetadata,
    pub meta: Meta,
}

/// One column of a value-level `where`, selecting rows by its values.
#[derive(Clone, Debug, PartialEq)]
pub struct WhereClause {
    pub column: Named,
    pub comparator: Comparator,
    pub values: Vec<Text>,
    pub meta: Meta,
}

/// How a value-level where column compares with its values (SUB-12). A bare
/// list is `In`, written EQ for one value and IN for several (D17); `Ne` and
/// `NotIn` are written `comparator: NE` and `comparator: NOTIN` (D66,
/// P-VLM-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparator {
    In,
    Ne,
    NotIn,
}

impl Comparator {
    /// Whether the comparison selects the rows that hold none of the values,
    /// a missing value included, instead of one of them.
    pub fn excludes(self) -> bool {
        !matches!(self, Self::In)
    }
    /// Whether the comparison takes `count` values: `NE` exactly one and
    /// `NOTIN` two or more; a bare list takes any nonempty list.
    pub fn takes(self, count: usize) -> bool {
        match self {
            Self::In => true,
            Self::Ne => count == 1,
            Self::NotIn => count >= 2,
        }
    }
    /// The comparison a written `comparator` states: `NE`, or else `NOTIN`,
    /// the one other spelling admission leaves.
    pub fn written_as(spelling: &str) -> Self {
        match WhereComparator::from_name(spelling) {
            Some(WhereComparator::Ne) => Self::Ne,
            Some(WhereComparator::Notin) | None => Self::NotIn,
        }
    }
    /// The spelling a written comparator has (`NE` or `NOTIN`), or none for
    /// a bare list.
    pub fn written(self) -> Option<&'static str> {
        match self {
            Self::In => None,
            Self::Ne => Some(WhereComparator::Ne.name()),
            Self::NotIn => Some(WhereComparator::Notin.name()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Origin {
    pub kind: Text,
    pub source: Option<Text>,
    pub description: Option<Text>,
    pub documents: Vec<DocumentRef>,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    pub name: Option<Text>,
    pub kind: Option<Text>,
    pub description: Text,
    pub expression: Option<FormalExpression>,
    pub documents: Vec<DocumentRef>,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormalExpression {
    pub context: Text,
    pub code: Text,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub text: Text,
    pub documents: Vec<DocumentRef>,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRef {
    pub document: Named,
    pub pages: Option<PageRef>,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageRef {
    pub kind: Text,
    pub refs: Text,
    pub title: Option<Text>,
    pub meta: Meta,
}
