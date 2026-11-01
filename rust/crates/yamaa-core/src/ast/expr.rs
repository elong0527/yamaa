//! Expression nodes. Every expression of a spec lives in the spec's one
//! arena and is addressed by [`ExprId`]; side tables key on that id.

use super::{Constant, JoinKey, Meta, Named, Text};
use crate::expression::{CompileError, CompiledExpression, Context};
use crate::expression_table::{Closed, DeclaredDefault, Handlers, ScalarOp, Slot, WindowFunction};
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

impl From<DeclaredDefault> for Literal {
    /// The literal a declared default writes; an empty list or mapping is
    /// no scalar.
    fn from(default: DeclaredDefault) -> Self {
        match default {
            DeclaredDefault::Text(text) => Self::Text(text.into()),
            DeclaredDefault::Integer(text) => Self::Int(text.into()),
            DeclaredDefault::Boolean(value) => Self::Bool(value),
            DeclaredDefault::Empty => Self::Unsupported,
        }
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
    pub closed: Closed,
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

/// One operand of a verb: the field it is written in, and the expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub slot: Slot,
    pub expr: ExprId,
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
    /// A `baseline_flag`'s written `window.order_by` that does not start
    /// with the date ascending, which WINDOW-5 refuses; an order that does
    /// is folded into `order_by`. Empty for every other window.
    pub window_order_by: Vec<OrderTerm>,
}

impl WindowFunction {
    /// A `row_value`'s offset as the rows it crosses. Execution admits at
    /// most `i64::MAX` context rows, so a larger magnitude is an absent
    /// neighbour on the written side. `None` for every other verb.
    pub fn row_value_offset(&self) -> Option<i64> {
        let Self::RowValue { offset } = self else {
            return None;
        };
        Some(offset.parse().unwrap_or_else(|_| {
            if offset.starts_with('-') {
                i64::MIN
            } else {
                i64::MAX
            }
        }))
    }
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
    /// The order `STR_JOIN` joins the group's values in, each term a field
    /// of the relation or a binding (LOOKUP-10).
    pub order_by: Vec<OrderTerm>,
    /// The text `STR_JOIN` writes between two values (FORMULA-18).
    pub separator: Option<Text>,
    /// Whether `STR_JOIN` and `COUNT(x)` take each value once; `None` when
    /// omitted (FORMULA-18).
    pub distinct: Option<bool>,
    /// The probability at which `QUANTILE` reads its quantile, as written;
    /// `None` when omitted or null (FORMULA-18).
    pub quantile: Option<super::Constant>,
    pub expr: ExprId,
    /// Of the bindings and the reducer fields, the ones written:
    /// `derivations`, `order_by`, `separator`, `distinct` and `quantile`.
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
