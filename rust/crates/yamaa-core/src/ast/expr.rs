//! Expression nodes. Every expression of a spec lives in the spec's one
//! arena and is addressed by [`ExprId`]; side tables key on that id.

use super::{Constant, JoinKey, Meta, Named, Text};
use crate::expression::{CompileError, CompiledExpression, Context};
use crate::value::{ColumnType, Value};

/// An expression: an index into its spec's expression arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExprId(pub(crate) u32);

impl ExprId {
    /// The arena position.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub meta: Meta,
}

/// The one closed set of expression shapes. Only
/// [`Spec::children`](super::Spec::children) matches on it to find
/// sub-expressions; every other walk derives from that.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// A written scalar: a YAML scalar, or text holding only a literal.
    Literal(Literal),
    /// A name read at the current position: a column, `OWNER.FIELD`, or
    /// `OWNER.*` under `COUNT`. A bare name in either surface lowers here,
    /// which keeps the canonical `source` binding and its paths.
    Ref(Name),
    /// Grammar text, compiled once; the names it reads are its children.
    Formula(Formula),
    /// A structured source binding.
    Source(SourceRead),
    /// Ordered branches; an arm without a condition is `otherwise`.
    Case(Vec<Arm>),
    Flag {
        condition: ExprId,
        true_value: Constant,
        false_value: Option<Constant>,
        missing: Option<Constant>,
    },
    FirstAvailable {
        sources: Vec<ExprId>,
        missing: Option<Constant>,
    },
    /// A row-wise verb over its operands.
    Scalar {
        op: ScalarOp,
        operands: Vec<Operand>,
        handlers: Handlers,
    },
    Window(Window),
    Aggregate(Aggregate),
    Odm(OdmRead),
    /// A project function call; each argument is a [`ExprKind::Ref`] or a
    /// [`ExprKind::Literal`].
    Call {
        function: Named,
        args: Vec<(Named, ExprId)>,
        shape: CallShape,
    },
    /// An operation, or an operation payload, outside the language. Only a
    /// document admitted without schema validation can hold one, and
    /// preparation refuses it; the text is the operation's key.
    Unsupported(String),
}

/// How a call's payload is written. The schema admits only `name` beside
/// `args`, a mapping; a document admitted without validation may write
/// another shape, which the call's admission refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallShape {
    /// Whether the payload writes `name` and `args` and no other field.
    pub closed: bool,
    /// Whether `args` is written as a mapping.
    pub args: bool,
}

/// A written scalar, kept as written so that lowering never fails: a value
/// that does not convert (an integer beyond 64 bits, an invalid date)
/// stays a finding of the check that converts it.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    /// YAML null, or `NULL` in text.
    Missing,
    Bool(bool),
    /// Canonical decimal text of any width.
    Int(String),
    /// A finite binary64 value.
    Float(f64),
    Text(String),
    /// A DATE literal in text, its value as written.
    Date(String),
    /// A DATETIME literal in text, its value as written.
    DateTime(String),
    /// A value outside the language; see [`ExprKind::Unsupported`].
    Unsupported,
}

impl Literal {
    /// The value the literal writes (EXPR-4): none for an integer beyond 64
    /// bits, temporal text that names no day or second, or a value outside
    /// the language, which the check that converts it refuses.
    pub fn value(&self) -> Option<Value> {
        Some(match self {
            Self::Missing => Value::Missing,
            Self::Text(value) => Value::Str(value.clone()),
            Self::Bool(value) => Value::Bool(*value),
            Self::Float(value) => Value::float(*value),
            Self::Int(value) => Value::Int(value.parse().ok()?),
            Self::Date(value) => Value::Date(value.parse().ok()?),
            Self::DateTime(value) => Value::DateTime(value.parse().ok()?),
            Self::Unsupported => return None,
        })
    }
}

/// A name as written: `COLUMN`, `OWNER.FIELD`, or `OWNER.*`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(pub String);

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// The relation a qualified name reads through.
    pub fn owner(&self) -> Option<&str> {
        self.0.split_once('.').map(|(owner, _)| owner)
    }
    /// The field part: the whole name when it is unqualified.
    pub fn field(&self) -> &str {
        self.0.split_once('.').map_or(&self.0, |(_, field)| field)
    }
    /// `OWNER.*`, the records of a relation under `COUNT`.
    pub fn is_records(&self) -> bool {
        self.0.ends_with(".*")
    }
}

/// Grammar text with its context and its one compilation. The expression's
/// own meta is its site and `text` carries the text field's, which differ
/// only where an operation holds the text in a field of its own, such as
/// `aggregate.expr`.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub context: Context,
    pub text: Text,
    pub compiled: Result<CompiledExpression, CompileError>,
    /// Each distinct name the text reads, as a [`ExprKind::Ref`] child:
    /// [`Role::Reducer`](super::Role::Reducer) inside a reduction,
    /// [`Role::Operand`](super::Role::Operand) outside one.
    pub names: Vec<(super::Role, ExprId)>,
}

/// A structured source binding: a variable with the record selection that
/// reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRead {
    pub variable: ExprId,
    pub between: Option<Between>,
    pub filter: Option<ExprId>,
    pub order_by: Vec<OrderTerm>,
    pub keep: Option<Keep>,
    /// The answer when the read finds no record, never when the variable
    /// does not exist (D67).
    pub absent: Option<Constant>,
    pub require_match: bool,
    /// Of `between`, `filter`, `order_by`, `keep`, `absent` and
    /// `require_match`.
    pub written: super::Written,
}

/// A record range: a current-row value between two donor-record fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Between {
    pub value: ExprId,
    pub lower: Option<ExprId>,
    pub upper: Option<ExprId>,
    pub meta: Meta,
}

/// One order term; a declared `nulls` is absolute and never flips.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderTerm {
    pub variable: ExprId,
    pub direction: Direction,
    pub nulls: Option<Nulls>,
    pub meta: Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nulls {
    First,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    First,
    Last,
}

/// One case branch; `condition` is `None` for `otherwise`.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub condition: Option<ExprId>,
    pub result: ExprId,
    pub meta: Meta,
}

/// The role an operand plays in its verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// The verb's input (P-OP-1, P-OP-2); every entry of `greatest`,
    /// `least` and a template's placeholders.
    Source,
    /// The date a source is compared with (`study_day`, `baseline_flag`).
    Reference,
    Start,
    End,
    /// `shift.days` or `shift.amount`.
    Amount,
    NotBefore,
    NotAfter,
    Fallback,
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub slot: Slot,
    pub expr: ExprId,
}

/// The answers a scalar verb declares; each verb has its own subset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handlers {
    pub missing: Option<Constant>,
    pub invalid: Option<Constant>,
    pub no_match: Option<Constant>,
    pub unmapped: Option<Constant>,
}

/// A scalar verb and its written parameters; its operands are separate.
/// Each verb is one of the executor's operation types, named on each
/// variant. The variant keeps parameters as written so that lowering stays
/// total: constructing the executor's operation validates them. What each
/// verb answers about itself, before and after its operation is built,
/// belongs to the executor (`crate::scalar_verb`), so no analysis names a
/// verb.
#[derive(Clone, Debug, PartialEq)]
pub enum ScalarOp {
    /// `dataset::Rounding`: `round_half_away_from_zero`, digits as written.
    Round {
        digits: String,
    },
    /// `dataset::Temporal`: `diff`, with endpoint inclusion (P-ORD-2).
    Diff {
        kind: TemporalType,
        unit: TimeUnit,
        closed: Closed,
    },
    /// `dataset::Temporal`.
    StudyDay,
    /// `dataset::Temporal`.
    ToEpochDay,
    /// `dataset::Temporal`: the amount is the `Amount` operand.
    Shift {
        kind: TemporalType,
        unit: Option<TimeUnit>,
    },
    /// `dataset::Temporal`.
    DurationValue {
        unit: TimeUnit,
    },
    /// `dataset::Temporal`: the inverse of `DurationValue`.
    DurationText {
        unit: TimeUnit,
    },
    /// `dataset::Temporal`: the parts are the operands, year first.
    Dtc,
    /// `dataset::Temporal`.
    TimeOfDay,
    /// `dataset::Imputation`.
    Impute {
        kind: TemporalType,
        month: Option<String>,
        day: Option<DayRule>,
        time: Option<TimeRule>,
    },
    /// `dataset::Imputation`.
    Precision {
        kind: TemporalType,
    },
    /// `dataset::Imputation`.
    ImputationFlag {
        part: FlagPart,
        seconds_collected: bool,
    },
    /// `dataset::Imputation`.
    ToDate,
    /// `text_case::StringCase`, with the `delimiters` list as written.
    StrCase {
        to: TextCase,
        delimiters: Option<String>,
    },
    /// `text_pad::StringPad`, width as written.
    StrPad {
        width: String,
    },
    /// `text_position::StringSubstr`, start and length as written.
    StrSubstr {
        start: String,
        length: Option<String>,
    },
    /// `text_position::StringLength`.
    StrLength,
    /// `text_extract::StringExtract`, group as written.
    StrExtract {
        pattern: String,
        group: String,
    },
    /// `text_replace::StringReplace`.
    StrReplace {
        pattern: String,
        replacement: String,
    },
    /// `text_template::StringTemplate`; its placeholders are the operands.
    StrTemplate {
        template: String,
    },
    Greatest,
    Least,
    /// `mapping_cut::MappingCut`.
    Mapping {
        dict: Dictionary,
        case_sensitive: bool,
    },
    /// `mapping_cut::MappingCut`.
    Cut {
        breaks: Vec<f64>,
        labels: Vec<String>,
        closed: Closed,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalType {
    Date,
    DateTime,
}

/// Calendar units for dates and elapsed units for datetimes, as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Day,
    Week,
    Month,
    Year,
    Second,
    Minute,
    Hour,
}

/// Which endpoints an interval includes, as `closed` writes it (P-ORD-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    Left,
    Right,
    Both,
    Neither,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DayRule {
    /// A day number as written.
    Day(String),
    First,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeRule {
    First,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagPart {
    Date,
    Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCase {
    Upper,
    Lower,
    Sentence,
    Title,
    Proper,
}

impl TextCase {
    /// The mode as `to` writes it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Sentence => "sentence",
            Self::Title => "title",
            Self::Proper => "proper",
        }
    }
}

/// A mapping dictionary, inline or the path of a file holding it.
#[derive(Clone, Debug, PartialEq)]
pub enum Dictionary {
    Inline(Vec<(Text, Constant)>),
    File(Text),
}

/// A window verb: its partitions, order (P-ORD-1) and row selection.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub function: WindowFunction,
    pub operands: Vec<Operand>,
    pub group_by: Vec<ExprId>,
    /// For `baseline_flag`, the date and then the tie-break terms.
    pub order_by: Vec<OrderTerm>,
    pub filter: Option<ExprId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowFunction {
    RowNumber,
    Rank {
        method: RankMethod,
    },
    /// The offset as written.
    RowValue {
        offset: String,
    },
    Locf {
        include_current: bool,
    },
    /// `window_order_by` holds a written `window.order_by` that does not
    /// start with the date ascending, which WINDOW-5 refuses; an order that
    /// does is folded into the window's `order_by`.
    BaselineFlag {
        inclusive: bool,
        window_order_by: Vec<OrderTerm>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankMethod {
    Competition,
    Dense,
}

/// A reduction over the records of one relation.
#[derive(Clone, Debug, PartialEq)]
pub struct Aggregate {
    /// `None` for a reduction of the output rows or of a grouped row's
    /// records; [`JoinKey::FromInput`] for a keyless qualified aggregate.
    pub key: Option<JoinKey>,
    pub between: Option<Between>,
    pub filter: Option<ExprId>,
    pub group_by: Vec<ExprId>,
    /// Per-record bindings evaluated before the reduction (P-BIND-1).
    pub bindings: Vec<Binding>,
    pub expr: ExprId,
    /// Of the bindings, written `derivations`.
    pub written: super::Written,
}

/// A named per-record value: an intermediate derivation or an aggregate
/// binding.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub name: Named,
    pub declared: Option<ColumnType>,
    pub derivation: super::Derivation,
    pub meta: Meta,
}

/// One collected ODM item read at the row's scope.
#[derive(Clone, Debug, PartialEq)]
pub struct OdmRead {
    /// `DATASET.ItemOID`, a [`ExprKind::Ref`].
    pub item: ExprId,
    /// Each selector is `None` when omitted; a written list, even an empty
    /// one, selects only the records carrying one of its values.
    pub event: Option<Vec<Text>>,
    pub form: Option<Vec<Text>>,
    pub item_group: Option<Vec<Text>>,
    pub filter: Option<ExprId>,
}
