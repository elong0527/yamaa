//! The executor's scalar verbs (#2844, owner decision 6).
//!
//! [`ScalarOp`] belongs to the executor: each written verb is one of the
//! executor's operation types, built here from its written parameters, and
//! each verb describes itself. From its written parameters alone,
//! [`ScalarOp`] answers its result type (EXPR-14) and how preparation scans
//! and checks it before any operation is built. The built [`Operation`]
//! answers the rest: the operands it reads and where a read of each is
//! reported, which operand types it accepts, its declaration findings and
//! its plan expression.
//!
//! So preparation, result typing, binding and `check()` each have one arm
//! for every scalar verb, and [`crate::specification`] names none of them.
//! A verb's schema module generates its variant of [`ScalarOp`] and how the
//! lowering reads it ([`crate::expression_table`]); a new verb then changes
//! only the executor: this module's arms, its operation type and the
//! operation's evaluation.
//!
//! Several answers keep how preparation has always checked one verb or
//! another, so that no result and no diagnostic changes; each says which
//! verbs it singles out.

mod extremes;
mod imputation;
mod mapping_cut;
mod rounding;
mod str_case;
mod str_decimal;
mod str_extract;
mod str_pad;
mod str_position;
mod str_template;
mod temporal;

pub(crate) use temporal::difference;

use crate::ast::{self, ExprId, ExprKind, ScalarOp, Slot, Spec};
use crate::dataset::{
    Assignment, ContextualMatch, Expression, Imputation, MatchRead, MatchValue, Rounding, Temporal,
};
use crate::diagnostic::Diagnostic;
use crate::mapping_cut::MappingCut;
use crate::result_type::ResultType;
use crate::text_case::StringCase;
use crate::text_decimal::StringDecimal;
use crate::text_extract::StringExtract;
use crate::text_pad::StringPad;
use crate::text_position::{StringLength, StringSubstr};
use crate::text_replace::StringReplace;
use crate::text_template::StringTemplate;
use crate::value::{ColumnType, Value};

/// What a verb answers from its written parameters, before any operation
/// is built.
impl ScalarOp {
    /// The verb's result type (EXPR-14), fixed by the verb or set by its
    /// written `type`, unit or dictionary: differences, study days, epoch
    /// days, times of day and text lengths are counts; a difference of
    /// datetimes in seconds is a whole count and in a longer unit a
    /// fraction; a shift and `impute` keep their declared type; a duration
    /// is a fraction in every unit; `to_date` is a date; assembled date
    /// parts, dates read from their declared forms, duration text, the other
    /// text verbs, precisions, imputation flags, decodes and cuts are text; a
    /// mapping returns the types its dictionary values take; `greatest` and
    /// `least` take every type their `operands` take.
    pub(crate) fn result_type(&self, operands: &mut dyn Iterator<Item = ResultType>) -> ResultType {
        let temporal = |kind: &ast::TemporalType| match kind {
            ast::TemporalType::Date => ResultType::DATE,
            ast::TemporalType::DateTime => ResultType::DATETIME,
        };
        match self {
            Self::RoundHalfAwayFromZero { .. } | Self::DurationValue { .. } => ResultType::FLOAT,
            Self::Diff {
                kind: ast::TemporalType::DateTime,
                unit,
                ..
            } if *unit != ast::TimeUnit::Second => ResultType::FLOAT,
            Self::Diff { .. }
            | Self::StudyDay
            | Self::ToEpochDay
            | Self::TimeOfDay
            | Self::StrLength => ResultType::INT,
            Self::Shift { kind, .. } | Self::Impute { kind, .. } => temporal(kind),
            Self::ToDate => ResultType::DATE,
            Self::Dtc
            | Self::DurationText { .. }
            | Self::DateText { .. }
            | Self::Precision { .. }
            | Self::ImputationFlag { .. }
            | Self::StrCase { .. }
            | Self::StrPad { .. }
            | Self::StrSubstr { .. }
            | Self::StrDecimal { .. }
            | Self::StrExtract { .. }
            | Self::StrReplace { .. }
            | Self::StrTemplate { .. }
            | Self::Decode { .. }
            | Self::Cut { .. } => ResultType::STR,
            Self::Mapping {
                dict: ast::Dictionary::Inline(entries),
                ..
            } => entries
                .iter()
                .fold(ResultType::MISSING, |kind, (_, value)| {
                    kind.or(value.value.result_type())
                }),
            // A dictionary resource is refused before typing.
            Self::Mapping { .. } => ResultType::UNKNOWN,
            Self::Greatest | Self::Least => operands.fold(ResultType::MISSING, ResultType::or),
        }
    }

    /// Whether the verb reads its operand in `slot` only when its source
    /// needs it: an `impute`'s bounds and fallback. Preparation's dependency
    /// scan leaves such an operand out, a row driver's reads take it in, and
    /// a lookup key's match value cannot hold it.
    pub(crate) fn conditional(&self, slot: Slot) -> bool {
        matches!(self, Self::Impute { .. })
            && matches!(slot, Slot::NotBefore | Slot::NotAfter | Slot::Fallback)
    }

    /// How preparation's dependency scan reads the verb's operands.
    pub(crate) fn scan(&self) -> OperandScan {
        match self {
            Self::Diff { .. } | Self::StudyDay => OperandScan::Twice,
            Self::StrTemplate { .. } => OperandScan::Placeholders,
            _ => OperandScan::Operands,
        }
    }

    /// Whether a lookup key's match value holding the verb matches on one
    /// read of its source, transformed, rather than as an expression:
    /// `str_pad` ([`Operation::match_value`]).
    pub(crate) fn key_read(&self) -> bool {
        matches!(self, Self::StrPad { .. })
    }

    /// Whether a column default holding the verb stays in the column phase
    /// when it reads a grouped template's driver, as a window or an
    /// aggregate does: a `mapping`.
    pub(crate) fn stays_in_column_phase(&self) -> bool {
        matches!(self, Self::Mapping { .. })
    }

    /// The semantic depth the verb adds for its own operands: `greatest` and
    /// `least` count one more level against the compiler's limit.
    pub(crate) fn operand_depth(&self) -> usize {
        usize::from(matches!(self, Self::Greatest | Self::Least))
    }

    /// The preflight check that refuses the verb's declaration and its
    /// declared operand types before any input is read ([`preflight`]);
    /// `str_pad`, `mapping`, `decode` and `cut` have none.
    pub(crate) fn preflight(&self) -> Option<Preflight> {
        Some(match self {
            Self::RoundHalfAwayFromZero { .. } => Preflight::Rounding,
            Self::Diff { .. }
            | Self::StudyDay
            | Self::ToEpochDay
            | Self::Shift { .. }
            | Self::DurationValue { .. }
            | Self::DurationText { .. }
            | Self::Dtc
            | Self::TimeOfDay => Preflight::Temporal,
            Self::Impute { .. }
            | Self::Precision { .. }
            | Self::ImputationFlag { .. }
            | Self::ToDate
            | Self::DateText { .. } => Preflight::Imputation,
            Self::StrCase { .. } => Preflight::Case,
            Self::Greatest | Self::Least => Preflight::Extremes,
            Self::StrExtract { .. } | Self::StrReplace { .. } => Preflight::Extraction,
            Self::StrTemplate { .. } => Preflight::Template,
            Self::StrSubstr { .. } | Self::StrLength => Preflight::Position,
            Self::StrDecimal { .. } => Preflight::Decimal,
            Self::StrPad { .. } | Self::Mapping { .. } | Self::Decode { .. } | Self::Cut { .. } => {
                return None
            }
        })
    }

    /// Whether a first row template's SELF read through one of the verb's
    /// operands is reported at that operand, as `greatest` and `least`
    /// report it, or at the verb (LOOKUP-7).
    pub(crate) fn cites_operands(&self) -> bool {
        matches!(self, Self::Greatest | Self::Least)
    }

    /// The row of SUB-19's table a derivation holding the verb falls in.
    pub(crate) fn recode(&self) -> Recode {
        match self {
            Self::Mapping { .. }
            | Self::Decode { .. }
            | Self::StrCase { .. }
            | Self::StrPad { .. }
            | Self::StrSubstr { .. }
            | Self::StrReplace { .. }
            | Self::StrExtract { .. }
            | Self::ToDate
            | Self::DurationText { .. }
            | Self::DateText { .. } => Recode::Source,
            Self::Dtc => Recode::Parts,
            Self::StrTemplate { template }
                if StringTemplate::new(template.clone(), "origin".into(), None)
                    .is_ok_and(|parsed| parsed.declaration_diagnostic().is_none()) =>
            {
                Recode::Template
            }
            _ => Recode::Computed,
        }
    }
}

/// How preparation's dependency scan reads a verb's operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperandScan {
    /// Each operand's variable in written order, but the conditional ones
    /// ([`ScalarOp::conditional`]).
    Operands,
    /// As `Operands`, twice: `diff` and `study_day` are scanned as operands
    /// and again as their two dates.
    Twice,
    /// None in place: a template's placeholders follow the whole scan.
    Placeholders,
}

/// The verbs' preflight checks, in the order preparation reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Preflight {
    Rounding,
    Temporal,
    Imputation,
    Case,
    Extremes,
    Extraction,
    Template,
    /// The positional text verbs, `str_substr` and `str_length`.
    Position,
    /// Fixed-decimal text, `str_decimal`.
    Decimal,
}

impl Preflight {
    /// Every check, in the order preparation reports them.
    pub(crate) const ORDER: [Self; 9] = [
        Self::Rounding,
        Self::Temporal,
        Self::Imputation,
        Self::Case,
        Self::Extremes,
        Self::Extraction,
        Self::Template,
        Self::Position,
        Self::Decimal,
    ];

    /// The derivations the check visits.
    pub(crate) fn sites(self) -> Sites {
        match self {
            Self::Rounding | Self::Temporal => Sites::Defaults,
            Self::Imputation | Self::Case | Self::Extremes => Sites::Derivations,
            Self::Extraction | Self::Template | Self::Position | Self::Decimal => Sites::Everywhere,
        }
    }

    /// Whether the check visits a spec with row templates: rounding, which
    /// types only bare names, checks nothing there, since a bare name's
    /// phase is binding's to decide.
    pub(crate) fn with_row_templates(self) -> bool {
        self != Self::Rounding
    }
}

/// The derivations a preflight check visits, each position at and below
/// them where a verb nests (EXPR-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sites {
    /// Every column default and every key value of an aggregate or an
    /// intermediate.
    Defaults,
    /// As `Defaults`, and every row template's own derivations.
    Derivations,
    /// Every column default, row template derivation, intermediate key value
    /// and intermediate derivation, and below each the case results,
    /// aggregate key values and aggregate derive bindings.
    Everywhere,
}

/// SUB-19's row for a derivation holding a verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Recode {
    /// The verb computes its value.
    Computed,
    /// It recodes or reformats its source: a recode when the source reads a
    /// variable.
    Source,
    /// It assembles its operands: a recode when it has some and each reads
    /// a variable (`dtc`'s date parts).
    Parts,
    /// It fills a valid template: a recode when each placeholder reads a
    /// variable.
    Template,
}

/// The preflight findings of the scalar verb at `e`, which its
/// [`ScalarOp::preflight`] check reports: its declaration findings and each
/// declared operand type it does not accept. `declared` types a name before
/// any schema is captured; with `rows` a bare name's phase is binding's to
/// decide, so a check that types bare names leaves them untyped.
pub(crate) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let ExprKind::Scalar { op, .. } = &spec.expr(e).kind else {
        return Err(Refusal::Internal);
    };
    match op.preflight() {
        Some(Preflight::Rounding) => Ok(rounding::preflight(spec, e)),
        Some(Preflight::Temporal) => temporal::preflight(spec, e, declared, rows),
        Some(Preflight::Imputation) => imputation::preflight(spec, e, declared, rows),
        Some(Preflight::Case) => str_case::preflight(spec, e, declared, rows),
        Some(Preflight::Extremes) => Ok(extremes::preflight(spec, e, declared)),
        Some(Preflight::Extraction) => str_extract::preflight(spec, e, declared, rows),
        Some(Preflight::Template) => str_template::preflight(spec, e, declared, rows),
        Some(Preflight::Position) => str_position::preflight(spec, e, declared, rows),
        Some(Preflight::Decimal) => str_decimal::preflight(spec, e, declared, rows),
        None => Ok(Vec::new()),
    }
}

/// What refuses a written verb before any source is read.
#[derive(Debug)]
pub(crate) enum Refusal {
    /// Declaration findings, in the order they are written.
    Findings(Vec<Diagnostic>),
    /// A written value the language cannot hold, at `path`.
    Literal { literal: ast::Literal, path: String },
    /// A pattern beyond a regular-expression limit, at `path`.
    Pattern {
        resource: crate::portable_regex::Resource,
        limit: usize,
        path: String,
    },
    /// An operation outside the language, at `path`.
    Unsupported {
        operation: &'static str,
        path: String,
    },
    /// A shape the tree's admission refuses before any verb is built.
    Internal,
}

/// One verb's executor operation, built from the tree, and the operands it
/// reads.
#[derive(Clone, Debug)]
pub(crate) struct Operation {
    kind: Kind,
    operands: Vec<Operand>,
}

/// The executor's operation types.
#[derive(Clone, Debug)]
enum Kind {
    Round(Rounding),
    Temporal(Temporal),
    Imputation(Imputation),
    StrCase(StringCase),
    /// A `str_pad` whose width is not a positive whole number has no
    /// operation, only its declaration defect (TEXT-2).
    StrPad(Result<StringPad, Diagnostic>),
    /// A `str_substr` whose fields TEXT-13 or TEXT-16 refuses has no
    /// operation, only its declaration defect.
    StrSubstr(Result<StringSubstr, Diagnostic>),
    StrLength(StringLength),
    StrDecimal(StringDecimal),
    StrExtract(StringExtract),
    StrReplace(StringReplace),
    StrTemplate(StringTemplate),
    Extreme {
        largest: bool,
        sources: Vec<String>,
        path: String,
    },
    MappingCut(MappingCut),
}

/// One operand of a built verb: what binding reads, and where binding and
/// `check()` report the read.
#[derive(Clone, Debug)]
pub(crate) struct Operand {
    pub(crate) read: Read,
    pub(crate) path: String,
}

/// How an operand is read.
#[derive(Clone, Debug)]
pub(crate) enum Read {
    /// A name, read as the reading scope decides.
    Name(String),
    /// A written value.
    Literal(Value),
    /// A source binding, at this node: a name, or a variable with its
    /// record selection.
    Source(ExprId),
}

impl Operand {
    fn name(name: &str, path: String) -> Self {
        Self {
            read: Read::Name(name.into()),
            path,
        }
    }
}

/// Which type authority binding consults for a verb's operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Typing {
    /// None: preflight types them, or the executor does (rounding,
    /// imputation, `str_pad`).
    None,
    /// Each operand once it binds, by the reading scope's catalog alone,
    /// which does not type a lookup's field (`str_extract`, `str_replace`,
    /// `str_template`, `str_substr`, `str_length`).
    Catalog,
    /// As `Catalog`, but not in a spec without row templates, whose names
    /// preflight typed (`str_case`).
    RowTemplates,
    /// Each operand once it binds, by what it binds to: a lookup's field by
    /// its donor's captured type, anything else by the catalog (the temporal
    /// verbs, `str_decimal`).
    Bound,
    /// Each operand once it binds, by its captured type: a name a lookup
    /// selects by its donor's, anything else by the catalog (`mapping`,
    /// `cut`).
    Captured,
    /// Every operand by its captured type, bound or not, compared together
    /// once all are read ([`Operation::incomparable`]; `greatest`, `least`).
    Together,
}

/// What a verb's plan needs once binding has read its operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Completion {
    /// Every operand bound and accepted.
    Operands,
    /// No finding since binding began (`str_template`, `greatest`, `least`).
    Findings,
}

/// A verb's own refusal of an operand whose name is unknown, in place of
/// the reference refusal binding reports.
#[derive(Clone, Debug)]
pub(crate) struct Unknown {
    pub(crate) diagnostic: Diagnostic,
    /// Whether it replaces the refusal of a qualified name too, or only of
    /// a bare one.
    pub(crate) qualified: bool,
}

impl Operation {
    /// The operation of the verb at `e`, or what refuses it before any source
    /// is read.
    pub(crate) fn from_tree(spec: &Spec, e: ExprId) -> Result<Self, Refusal> {
        let ExprKind::Scalar { op, .. } = &spec.expr(e).kind else {
            return Err(Refusal::Internal);
        };
        match op {
            ScalarOp::RoundHalfAwayFromZero { .. } => rounding::from_tree(spec, e),
            ScalarOp::Diff { .. }
            | ScalarOp::StudyDay
            | ScalarOp::ToEpochDay
            | ScalarOp::Shift { .. }
            | ScalarOp::DurationValue { .. }
            | ScalarOp::DurationText { .. }
            | ScalarOp::Dtc
            | ScalarOp::TimeOfDay => temporal::from_tree(spec, e),
            ScalarOp::Impute { .. }
            | ScalarOp::Precision { .. }
            | ScalarOp::ImputationFlag { .. }
            | ScalarOp::ToDate
            | ScalarOp::DateText { .. } => imputation::from_tree(spec, e),
            ScalarOp::StrCase { .. } => str_case::from_tree(spec, e),
            ScalarOp::StrPad { .. } => str_pad::from_tree(spec, e),
            ScalarOp::StrSubstr { .. } | ScalarOp::StrLength => str_position::from_tree(spec, e),
            ScalarOp::StrDecimal { .. } => str_decimal::from_tree(spec, e),
            ScalarOp::StrExtract { .. } | ScalarOp::StrReplace { .. } => {
                str_extract::from_tree(spec, e)
            }
            ScalarOp::StrTemplate { .. } => str_template::from_tree(spec, e),
            ScalarOp::Greatest | ScalarOp::Least => extremes::from_tree(spec, e),
            ScalarOp::Mapping { .. } | ScalarOp::Cut { .. } => mapping_cut::from_tree(spec, e),
            // The executor reads no project codelist yet, so a `decode`
            // (EXPR-15) is refused before any source is read.
            ScalarOp::Decode { .. } => Err(Refusal::Unsupported {
                operation: crate::expression_table::Verb::Decode.name(),
                path: path(spec, e),
            }),
        }
    }

    /// The operands, in the order binding reads them.
    pub(crate) fn operands(&self) -> &[Operand] {
        &self.operands
    }

    /// The operand-type table: the refusal of a value of `kind` at the
    /// operand `index`, when the verb does not accept it.
    pub(crate) fn check(&self, index: usize, kind: ColumnType) -> Option<Diagnostic> {
        let actual = kind.name();
        let text = kind == ColumnType::Str;
        match &self.kind {
            Kind::Round(rounding) => (!matches!(kind, ColumnType::Int | ColumnType::Float))
                .then(|| Rounding::input_diagnostic(rounding.source(), rounding.path(), actual)),
            Kind::Temporal(operation) => (!operation.accepts(index, kind))
                .then(|| operation.operand_diagnostic(index, actual)),
            Kind::Imputation(operation) if index == 0 => {
                (!operation.accepts(kind)).then(|| operation.input_diagnostic(actual))
            }
            Kind::Imputation(operation) => {
                let (field, name) = operation.operands().nth(index)?;
                (!operation.accepts_operand(kind))
                    .then(|| operation.operand_input_diagnostic(field, name, actual))
            }
            Kind::StrCase(operation) => (!text).then(|| operation.input_diagnostic(actual)),
            Kind::StrSubstr(Ok(operation)) => (!text).then(|| operation.input_diagnostic(actual)),
            Kind::StrLength(operation) => (!text).then(|| operation.input_diagnostic(actual)),
            Kind::StrDecimal(operation) => (!matches!(kind, ColumnType::Int | ColumnType::Float))
                .then(|| {
                    StringDecimal::input_diagnostic(operation.source(), operation.path(), actual)
                }),
            Kind::StrExtract(operation) => (!text).then(|| operation.input_diagnostic(actual)),
            Kind::StrReplace(operation) => {
                (!text).then(|| operation.search().input_diagnostic(actual))
            }
            Kind::StrTemplate(operation) => (!text)
                .then(|| operation.input_diagnostic(index, actual))
                .flatten(),
            Kind::MappingCut(operation) => {
                (!operation.accepts(kind)).then(|| operation.input_diagnostic(actual))
            }
            // A defective `str_substr` is refused at its declaration, before
            // binding reads its source.
            Kind::StrPad(_) | Kind::StrSubstr(Err(_)) | Kind::Extreme { .. } => None,
        }
    }

    /// Which type authority binding consults for the operands.
    pub(crate) fn typing(&self) -> Typing {
        match &self.kind {
            Kind::Round(_) | Kind::Imputation(_) | Kind::StrPad(_) => Typing::None,
            Kind::StrExtract(_)
            | Kind::StrReplace(_)
            | Kind::StrTemplate(_)
            | Kind::StrSubstr(_)
            | Kind::StrLength(_) => Typing::Catalog,
            Kind::StrCase(_) => Typing::RowTemplates,
            Kind::Temporal(_) | Kind::StrDecimal(_) => Typing::Bound,
            Kind::MappingCut(_) => Typing::Captured,
            Kind::Extreme { .. } => Typing::Together,
        }
    }

    /// VALUE-19: operands of the captured types `types`, every one known,
    /// that do not all compare with the first (`greatest`, `least`).
    pub(crate) fn incomparable(&self, types: &[Option<ColumnType>]) -> Option<Diagnostic> {
        let Kind::Extreme { sources, path, .. } = &self.kind else {
            return None;
        };
        let types = types.iter().copied().collect::<Option<Vec<_>>>()?;
        extremes::incomparable(sources, &types, path)
    }

    /// The verb's own refusal of the operand `index`, unbound because its
    /// name is unknown: an extraction's source, a template's placeholder.
    pub(crate) fn unknown(&self, index: usize) -> Option<Unknown> {
        match &self.kind {
            Kind::StrExtract(operation) => Some(Unknown {
                diagnostic: operation.unknown_diagnostic(),
                qualified: true,
            }),
            Kind::StrReplace(operation) => Some(Unknown {
                diagnostic: operation.search().unknown_diagnostic(),
                qualified: true,
            }),
            Kind::StrTemplate(operation) => {
                operation
                    .unknown_diagnostic(index)
                    .map(|diagnostic| Unknown {
                        diagnostic,
                        qualified: false,
                    })
            }
            _ => None,
        }
    }

    /// What the plan needs once binding has read the operands.
    pub(crate) fn completion(&self) -> Completion {
        match &self.kind {
            Kind::StrTemplate(_) | Kind::Extreme { .. } => Completion::Findings,
            _ => Completion::Operands,
        }
    }

    /// A declaration finding binding reports in place of every operand: a
    /// template's syntax (TEXT-3).
    pub(crate) fn refusal(&self) -> Option<&Diagnostic> {
        match &self.kind {
            Kind::StrTemplate(operation) => operation.declaration_diagnostic(),
            _ => None,
        }
    }

    /// A declaration finding binding reports once every operand is bound and
    /// accepted: an extraction's pattern or group.
    pub(crate) fn late_refusal(&self) -> Option<&Diagnostic> {
        match &self.kind {
            Kind::StrExtract(operation) => operation.declaration_diagnostic(),
            Kind::StrReplace(operation) => operation.search().declaration_diagnostic(),
            _ => None,
        }
    }

    /// The declaration defect that leaves the verb without an operation,
    /// which binding reports at its derivation's checkpoint, before any
    /// reference binds: a `str_pad` width (TEXT-2), a `str_substr`'s fields
    /// (TEXT-13, TEXT-16).
    pub(crate) fn defect(&self) -> Option<&Diagnostic> {
        match &self.kind {
            Kind::StrPad(Err(defect)) | Kind::StrSubstr(Err(defect)) => Some(defect),
            _ => None,
        }
    }

    /// Each written operand whose value the operand does not take: a whole
    /// number a temporal verb counts, written as a fraction.
    pub(crate) fn literal_findings(&self) -> Vec<Diagnostic> {
        match &self.kind {
            Kind::Temporal(operation) => temporal::literal_findings(operation),
            _ => Vec::new(),
        }
    }

    /// The plan expression over the bound operands, one for each operand in
    /// order; none for a verb with a declaration defect.
    pub(crate) fn plan(&self, mut operands: Vec<Assignment>) -> Option<Expression> {
        let mut source = || {
            (operands.len() == 1)
                .then(|| operands.pop())
                .flatten()
                .map(std::boxed::Box::new)
        };
        Some(match &self.kind {
            Kind::Round(rounding) => Expression::Round {
                source: source()?,
                rounding: rounding.clone(),
            },
            Kind::StrCase(operation) => Expression::StrCase {
                source: source()?,
                operation: operation.clone(),
            },
            Kind::StrPad(Ok(operation)) => Expression::StrPad {
                source: source()?,
                operation: operation.clone(),
            },
            Kind::StrPad(Err(_)) | Kind::StrSubstr(Err(_)) => return None,
            Kind::StrSubstr(Ok(operation)) => Expression::StrSubstr {
                source: source()?,
                operation: operation.clone(),
            },
            Kind::StrLength(operation) => Expression::StrLength {
                source: source()?,
                operation: operation.clone(),
            },
            Kind::StrDecimal(operation) => Expression::StrDecimal {
                source: source()?,
                operation: operation.clone(),
            },
            Kind::StrExtract(operation) => Expression::StrExtract {
                source: source()?,
                operation: operation.clone(),
            },
            Kind::StrReplace(operation) => Expression::StrReplace {
                source: source()?,
                operation: operation.clone(),
            },
            // Collection multiplicity belongs to the mapping or cut; its
            // source's own reads keep the authored `.source` leaf.
            Kind::MappingCut(operation) => Expression::MappingCut {
                source: source().map(|source| {
                    std::boxed::Box::new(Assignment {
                        path: operation.path().into(),
                        ..*source
                    })
                })?,
                operation: operation.clone(),
            },
            Kind::Temporal(operation) => Expression::Temporal {
                operands,
                operation: operation.clone(),
            },
            Kind::Imputation(operation) => Expression::Imputation {
                operands,
                operation: operation.clone(),
            },
            Kind::StrTemplate(operation) => Expression::StrTemplate {
                operands,
                operation: operation.clone(),
            },
            Kind::Extreme { largest, .. } => Expression::Extreme {
                operands,
                largest: *largest,
            },
        })
    }

    /// As a lookup key's match value that reads `read` ([`ScalarOp::key_read`]):
    /// the executor's match value.
    pub(crate) fn match_value(&self, read: MatchRead) -> Option<MatchValue> {
        match &self.kind {
            Kind::StrPad(operation) => Some(MatchValue::StrPad {
                read,
                operation: operation.as_ref().ok().cloned(),
            }),
            _ => None,
        }
    }

    /// As a lookup key's match value whose read of `identifier` binds once
    /// the context that reads it is known.
    pub(crate) fn contextual_match(&self, identifier: String) -> Option<ContextualMatch> {
        match &self.kind {
            Kind::StrPad(operation) => Some(ContextualMatch::StrPad {
                identifier,
                operation: operation.as_ref().ok().cloned(),
            }),
            _ => None,
        }
    }
}

/// A node's path, as the spec's diagnostics cite it.
fn path(spec: &Spec, e: ExprId) -> String {
    spec.cite(spec.expr(e).meta)
}

/// The operand of a verb in `slot`.
fn operand(operands: &[ast::Operand], slot: Slot) -> Option<ExprId> {
    operands
        .iter()
        .find(|operand| operand.slot == slot)
        .map(|operand| operand.expr)
}

/// The name a reference reads.
fn name(spec: &Spec, e: ExprId) -> Option<&str> {
    match &spec.expr(e).kind {
        ExprKind::Ref(name) => Some(name.as_str()),
        _ => None,
    }
}

/// The name a verb's operand in `slot` reads.
fn operand_name<'s>(spec: &'s Spec, operands: &[ast::Operand], slot: Slot) -> Option<&'s str> {
    operand(operands, slot).and_then(|e| name(spec, e))
}

/// A written value, refused at `path` when the language cannot hold it.
fn literal(literal: &ast::Literal, path: String) -> Result<Value, Refusal> {
    literal.value().ok_or_else(|| Refusal::Literal {
        literal: literal.clone(),
        path,
    })
}

/// A written handler's value, refused at its own path.
fn handler(spec: &Spec, handler: Option<&ast::Constant>) -> Result<Option<Value>, Refusal> {
    handler
        .map(|handler| literal(&handler.value, spec.cite(handler.meta)))
        .transpose()
}
