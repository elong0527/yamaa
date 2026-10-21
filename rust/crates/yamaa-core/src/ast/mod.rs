//! The typed specification tree that every front end lowers into, and the
//! roots of a project's own files ([`project`]).
//!
//! Each surface has its own front end under [`crate::front_end`] (today's
//! in `v0_5`; the 1.0 front end comes with #3030 Phase 1). Each lowers into
//! these nodes, and every analysis reads them through the one traversal in
//! [`traversal`] and types them with the one result-type table,
//! [`result_type`]. There is no IO here and there are no YAML types:
//! positions arrive as plain numbers.
//!
//! Every node carries a [`Meta`]: its structured spec path, which one
//! renderer writes in either notation, and the position of the deepest
//! written node in the layer whose value took effect (P-ISSUE-1).

mod expr;
mod path;
pub mod project;
pub mod traversal;
mod types;

pub use expr::{
    Aggregate, Arm, Between, Binding, CallShape, Closed, DayRule, Dictionary, Direction, Expr,
    ExprId, ExprKind, FlagPart, Formula, Handlers, Keep, Literal, Name, Nulls, OdmRead, Operand,
    OrderTerm, Precision, RankMethod, ScalarOp, Slot, SourceRead, TemporalType, TextCase, TimeRule,
    TimeUnit, Window, WindowFunction,
};
pub use path::{Notation, PathId, Paths, Segment};
pub use project::{
    Codelist, CodelistDeclaration, CodelistItem, CodelistSource, CodelistType, Codelists,
    DefineContext, DocumentKind, ExternalDictionary, Function, FunctionDeclaration, FunctionSource,
    FunctionTest, GuideStatus, HostEnvironment, Implementation, ImplementationGuide,
    MetadataVersion, Parameter, Project, Section, Study, SubmissionDeclarations, SubmissionRefused,
    SupportingDocument, TerminologyStandard, Transport, STANDARD_NAMES,
};
pub use traversal::{Children, Filtered, Ordered, Owner, Read, Role, Scope, Site, Step};
pub use types::{result_type, Table, TypeEnv};

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
/// optional; `{literal: null}` today and YAML null in 1.0 are both
/// [`Literal::Missing`].
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

/// The surface a file is written in and the version it declares.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub surface: Surface,
    /// Today's `schema_version`; in 1.0 the chain's version (P-VER-1).
    pub version: Text,
    pub meta: Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    V0_5,
    V1 { minor: u32 },
}

impl Surface {
    /// Whether a source binding's or a lookup's `absent` also answers a
    /// variable that does not exist where it is read, as COLUMN-11 at the
    /// `yamaa-v0.5.0` tag says. In 1.0 `absent` answers only a lookup that
    /// finds no record, and a variable that does not exist fails under
    /// NAME-3 (D67).
    pub fn absent_answers_unknown_variable(self) -> bool {
        matches!(self, Self::V0_5)
    }
    /// Whether a lookup that declares no `key` infers the applicable output
    /// keys at binding, as LOOKUP-1 and LOOKUP-5 at the `yamaa-v0.5.0` tag
    /// say. In 1.0 an implicit join and a qualified aggregate without `key`
    /// match on the input's declared `key`, and every named intermediate but
    /// a row template's driver declares its own (D23, D63).
    pub fn infers_join_keys(self) -> bool {
        matches!(self, Self::V0_5)
    }
}

/// `model: SDTM | SEND | ADaM` (P-MODEL-1); no 0.5 form.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub kind: ModelKind,
    pub meta: Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    Sdtm,
    Send,
    Adam,
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
    /// Write a node's path in `notation`.
    pub fn render(&self, path: PathId, notation: Notation) -> String {
        self.paths.render(path, notation)
    }
    /// The notation this spec's diagnostics cite paths in: today's for a
    /// 0.5 file, P-CLR-2's for a 1.0 file.
    pub fn notation(&self) -> Notation {
        match self.header.surface {
            Surface::V0_5 => Notation::V0_5,
            Surface::V1 { .. } => Notation::V1,
        }
    }
    /// A node's path as this spec's diagnostics cite it.
    pub fn cite(&self, meta: Meta) -> String {
        self.render(meta.path, self.notation())
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
    /// Whether the intermediate `item` lacks the `key` it requires: on a 1.0
    /// tree, it declares none and drives no row template (LOOKUP-3, ROW-2;
    /// D23, D63). A 0.5 lookup without `key` infers it.
    pub fn lacks_key(&self, item: &Intermediate) -> bool {
        !self.header.surface.infers_join_keys()
            && item.key.is_none()
            && !self.drives_rows(&item.id.name)
    }
    /// The input `name`'s declared `key`, if it declares one (P-JOIN-1).
    pub fn input_key(&self, name: &str) -> Option<&[Named]> {
        self.inputs
            .iter()
            .rfind(|input| input.name.name == name)
            .and_then(|input| match &input.key {
                KeyDecl::Declared(columns) => Some(columns.as_slice()),
                KeyDecl::Inferred | KeyDecl::Undeclared => None,
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
/// has no 0.5 form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Csv,
    Parquet,
    Xport,
    Sas7bdat,
    OdmXml,
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
    /// 0.5 only: implicit joins infer the applicable keys at binding.
    Inferred,
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
    pub key: Option<JoinKey>,
    pub between: Option<Between>,
    pub filter: Option<ExprId>,
    pub order_by: Vec<OrderTerm>,
    pub keep: Option<Keep>,
    pub columns: Option<Vec<Named>>,
    /// The answer when the lookup selects no record: `no_match` in 0.5,
    /// `absent` in 1.0 (P-HND-1). Which absence it answers is the surface's
    /// ([`Surface::absent_answers_unknown_variable`], D67).
    pub absent: Option<Constant>,
    pub require_match: bool,
    pub bindings: Vec<Binding>,
    pub checks: Vec<UniqueCheck>,
    /// Of `key`, `between`, `filter`, `order_by`, `keep`, `columns`,
    /// `derivations`, `verifications`, the absence answer and
    /// `require_match`.
    pub written: Written,
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
    /// The input's declared key (P-KEY-1); no 0.5 form.
    FromInput,
    /// 0.5 only: the keys are inferred at binding.
    Applicable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UniqueCheck {
    pub columns: Vec<Named>,
    pub id: Option<Text>,
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
    NotMissing,
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

/// Today's list form is `In`, written EQ or IN by the number of values;
/// `NotIn`, written NE or NOTIN, is the 1.0 addition (P-VLM-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparator {
    In,
    NotIn,
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
