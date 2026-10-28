//! Bound dataset plans and their execution over explicit source and function
//! ports.
pub use crate::bound_expression::BoundNumeric;
use crate::bound_expression::Read;
use crate::{
    bound_expression::{BindingError, BoundPredicate},
    table::TableSchema,
    value::Value,
};
use std::{collections::BTreeMap, vec};
mod conversion;
pub use conversion::ConversionHandler;
mod count;
pub use count::{share, CountBoundError, RowCount, RowCountBound};
mod flags;
pub use flags::{flag_condition_diagnostic, Flag, FlagBinding, FlagError, FlagRead};
mod functions;
pub use functions::{BoundProjectFunction, FunctionArgument, FunctionInput};
mod aggregate;
pub use aggregate::{
    AggregateBinding, AggregateBindingError, AggregateDerivation, AggregatePhase,
    AggregateRelation, AggregateRowContext, AggregateSource, BoundAggregate,
};
pub(crate) mod odm;
pub use odm::{BoundOdm, OdmRead, OdmScope, OdmTemplateScope};
mod intermediates;
pub(crate) use intermediates::{read_handlers, ReadHandlers};
pub use intermediates::{
    visit_output_dependencies, Intermediate, IntermediateDerivations, IntermediateUnique, Nearest,
    RecordMatchKey, SourceSchemas,
};
pub(crate) mod relation_predicates;
pub use relation_predicates::{CurrentField, CurrentRead, RelationPredicate};
mod relation_shape;
pub use relation_shape::{RelationField, RelationFieldOrigin, RelationShape, RelationShapeError};
mod relation_range;
pub use relation_range::{
    RelationRange, RelationRangeBound, RelationRangeError, RelationRangeFailure, RelationRangeTypes,
};
mod self_relations;
pub use self_relations::IntermediateSource;
mod pairs;
pub use pairs::{PairJoin, PairKey, PairRecords, PairSide, PairValue};
mod match_expression;
mod match_values;
pub use match_expression::MatchExpression;
pub use match_values::{ContextualMatch, IntermediateKey, MatchRead, MatchValue};
mod joined;
pub use joined::JoinedRead;
mod rounding;
pub use rounding::{InvalidDigits, Rounding};
mod imputation;
pub(crate) use imputation::completion_policy_findings;
pub use imputation::{
    completion_policy, Completion, DatePolicyError, DayDeclaration, DayRule, FlagPart, Imputation,
    ImputationKind, ImputationStep, Pending, TemporalType, TimeRule,
};
mod duration;
mod temporal;
mod temporal_arithmetic;
pub use crate::text_case::StringCase;
pub use crate::text_decimal::StringDecimal;
pub use crate::text_extract::StringExtract;
pub use crate::text_pad::StringPad;
pub use crate::text_position::{StringLength, StringSubstr};
pub use crate::text_replace::StringReplace;
pub use crate::text_template::StringTemplate;
pub use temporal::{
    CalendarUnit, DateBounds, ElapsedUnit, InvalidTemporal, Temporal, TemporalKind, DATE_PARTS,
};
pub use temporal_arithmetic::TemporalStep;
mod selection;
pub use selection::{FirstAvailable, SelectionRead, SelectionSource};
mod row_stages;
pub use row_stages::{read_requires_window, window_stages};
mod row_drivers;
mod windows;
pub use windows::{ContextWindow, OrderTerm, Window, WindowKind};
mod cut;
pub use cut::PartitionCut;
pub mod resolved;
pub use resolved::{ResolutionError, ResolvedPlan};
mod columnar;
pub use columnar::PartitionSource;

/// Already bound expressions; no host joins or lookup fallback are implicit.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Joined(std::boxed::Box<JoinedRead>),
    Odm(std::boxed::Box<BoundOdm>),
    Aggregate(std::boxed::Box<BoundAggregate>),
    MappingCut {
        source: std::boxed::Box<Assignment>,
        operation: crate::mapping_cut::MappingCut,
    },
    Extreme {
        operands: Vec<Assignment>,
        largest: bool,
    },
    StrTemplate {
        operands: Vec<Assignment>,
        operation: StringTemplate,
    },
    StrExtract {
        source: std::boxed::Box<Assignment>,
        operation: StringExtract,
    },
    StrReplace {
        source: std::boxed::Box<Assignment>,
        operation: StringReplace,
    },
    StrCase {
        source: std::boxed::Box<Assignment>,
        operation: StringCase,
    },
    StrPad {
        source: std::boxed::Box<Assignment>,
        operation: StringPad,
    },
    StrSubstr {
        source: std::boxed::Box<Assignment>,
        operation: StringSubstr,
    },
    StrLength {
        source: std::boxed::Box<Assignment>,
        operation: StringLength,
    },
    StrDecimal {
        source: std::boxed::Box<Assignment>,
        operation: StringDecimal,
    },
    /// Only the first TRUE branch is evaluated; FALSE and UNKNOWN continue.
    Case(Vec<CaseBranch>),
    Literal(Value),
    /// Binding handler ownership is retained even when the variable exists.
    SourceBinding {
        source: std::boxed::Box<Assignment>,
        absent: Option<Value>,
        /// A source keep declaration still has an auditable zero count when its
        /// operand is an already selected scalar and needs no new record choice.
        selection_declared: bool,
        variable: String,
        path: String,
    },
    /// An implicit join on completed output keys (LOOKUP-1): a source
    /// binding's range, filter, selection and answer for a row without a
    /// record, or an operand's.
    SourceLookup {
        lookup: Lookup,
        range: Option<RelationRange>,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
        /// The read's answer for a row without a record: a source binding's,
        /// or an operand's.
        absence: Option<JoinAbsence>,
    },
    /// A source binding's implicit join on its declared port, with its range,
    /// filter, selection and answer for a row without a record.
    JoinedSource {
        read: std::boxed::Box<JoinedRead>,
        range: Option<RelationRange>,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
        /// The source binding's answer for a row without a record.
        absence: Option<JoinAbsence>,
    },
    /// One condition selects a raw true/false/UNKNOWN literal.
    Flag(Flag),
    /// Compiled scalar arithmetic over statically bound source/completed output reads.
    Compute(BoundNumeric),
    /// Declarative rounding over one bound operand; no formula text is invented.
    Round {
        source: std::boxed::Box<Assignment>,
        rounding: Rounding,
    },
    Imputation {
        operands: Vec<Assignment>,
        operation: Imputation,
    },
    Temporal {
        operands: Vec<Assignment>,
        operation: Temporal,
    },
    /// Versionless package invocation, activated before any study read.
    ProjectFunction(BoundProjectFunction),
    /// Windows over completed output rows in their column or row-template context.
    Window(Window),
    ContextWindow(ContextWindow),
    /// First present raw operand in authored order, followed by one output conversion.
    FirstAvailable(std::boxed::Box<FirstAvailable>),
    Source(usize),
    /// Read a field from the run-local cached named record selection.
    Intermediate {
        index: usize,
        column: usize,
    },
    /// Distinct present raw readings across a key combination, before conversion.
    Collect {
        column: usize,
        identifier: String,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
    },
    Column(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaseBranch {
    pub when: Option<BoundPredicate>,
    pub assignment: std::boxed::Box<Assignment>,
}

/// Choose the first or last record in declared stable source order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    First,
    Last,
}

/// Stable record order; each caller determines when its cardinality requires a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSelection {
    pub order_by: Vec<OrderTerm>,
    pub keep: Keep,
}

/// How a read through an implicit join answers a row whose match finds no
/// record, or whose match value is missing (LOOKUP-24): a `source` binding
/// with its declared `absent` value and `require_match`, and an operand bound
/// to the join with neither. A formula's reads carry none; their owner gives
/// the read's site when it reads.
#[derive(Clone, Debug, PartialEq)]
pub struct JoinAbsence {
    /// The read's path, owning the `absent` count and failure.
    pub path: String,
    /// The declared `absent` literal; `None` gives missing by default.
    pub absent: Option<Value>,
    /// `require_match: true` fails the row as `unmatched_key` instead.
    pub require_match: bool,
}
impl JoinAbsence {
    /// An operand's answer: missing by default, counted at its path.
    pub fn operand(path: &str) -> Self {
        Self {
            path: path.into(),
            absent: None,
            require_match: false,
        }
    }
    /// Whether the read fails instead of answering, given whether its input
    /// declares `require_match: true`: a declared `absent` always answers.
    pub fn requires_match(&self, input_requires_match: bool) -> bool {
        self.absent.is_none() && (self.require_match || input_requires_match)
    }
}

/// One named secondary relation with a stable schema and source-list position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecondarySource {
    pub name: String,
    pub schema: TableSchema,
}

/// Equality pairs compare a secondary source field to a completed output value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchKey {
    pub source_column: usize,
    pub output_column: usize,
}

/// A many-to-one read; absence is missing and duplicate records remain an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lookup {
    pub source: usize,
    pub column: usize,
    pub keys: Vec<MatchKey>,
}

/// One completed-value assignment, with original specification provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub column: usize,
    pub expression: Expression,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowMode {
    Records,
    Groups(Vec<usize>),
    Keys,
    /// A named input relation retains the backing donor coordinates.
    IntermediateDriver {
        index: usize,
        source: usize,
        mode: std::boxed::Box<RowMode>,
    },
    /// A stored secondary source with one bounded record/group construction mode.
    Driver {
        source: usize,
        mode: std::boxed::Box<RowMode>,
    },
}

/// Assignments are supplied in resolved dependency order, not map iteration order.
#[derive(Clone, Debug, PartialEq)]
pub struct RowTemplate {
    pub mode: RowMode,
    pub assignments: Vec<Assignment>,
    pub filter: Option<BoundPredicate>,
}

/// One bound column-phase derivation and the row templates whose rows it
/// completes (#2829). A column whose templates read different drivers has one
/// binding per driver. A column's bindings are adjacent in
/// [`DatasetPlan::column_bindings`], in the order of the first template each
/// serves, and its step runs them in that order.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnBinding {
    pub assignment: Assignment,
    /// The row templates whose rows this binding completes, ascending. A
    /// template that derives the column itself is never listed.
    pub templates: Vec<usize>,
    pub scope: ColumnScope,
}

/// The records a column binding's driver reads address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnScope {
    /// Each row's own driver: a driver read addresses the record that built
    /// the row, through the port of the row's template (LOOKUP-1).
    Template,
    /// The whole dataset, through the primary port: the binding reads no
    /// driver record of a row another input builds, and a qualified aggregate
    /// reduces its declared input (LOOKUP-18).
    Dataset,
}

impl ColumnBinding {
    /// A whole-column assignment for every template that does not derive the
    /// column itself, each row read through its own template's port.
    pub fn every_template(assignment: Assignment, templates: &[RowTemplate]) -> Self {
        let templates = templates
            .iter()
            .enumerate()
            .filter(|(_, template)| {
                !template
                    .assignments
                    .iter()
                    .any(|row| row.column == assignment.column)
            })
            .map(|(index, _)| index)
            .collect();
        Self {
            assignment,
            templates,
            scope: ColumnScope::Template,
        }
    }
    /// Whether this binding completes the rows of `template`. A key-grain
    /// candidate belongs to the sole key template.
    pub fn serves(&self, template: Option<usize>) -> bool {
        self.templates.contains(&template.unwrap_or(0))
    }
}

/// Error-severity dataset checks supported by this closed application slice.
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    /// Presence of the owning completed column, before later derivations, on
    /// the rows `when` selects, every row without it; with `max_fraction`, at
    /// most that share of those rows may be missing (CHECK-5).
    NotMissing {
        when: Option<BoundPredicate>,
        max_fraction: Option<f64>,
    },
    /// Presence required by the column's resolved `mandatory`. Without a scope
    /// every row is checked; a scope selects rows by value-level where clauses.
    Mandatory(Option<RowScope>),
    /// The ISO 8601 form a `str` column's date or time submission data type
    /// names (SUB-26). Without a scope every row is checked; a scope selects
    /// rows by value-level where clauses.
    DataType {
        form: crate::iso8601::Form,
        scope: Option<RowScope>,
    },
    /// Already converted, present permitted values for the owning column.
    AllowedValues(Vec<Value>),
    /// Fixed environment terminology at the owning column's checkpoint.
    /// Numeric codes retain their original integer/binary64 representation.
    Codelist {
        id: String,
        values: Vec<Value>,
    },
    /// The column's and its value-level entries' bindings share one
    /// declaration path. Selection reads completed where columns, never a
    /// donor row or template: each binding checks the rows its clauses select,
    /// and the column's binding every row no `overrides` clause selects.
    ValueCodelists {
        overrides: Vec<WhereClause>,
        bindings: Vec<ValueCodelist>,
    },
    Inventory {
        values: Vec<Value>,
        scope: Option<InventoryScope>,
    },
    /// A value-level length bound over the rows its scope selects.
    ValueMaxLength {
        max: RowCountBound,
        scope: RowScope,
    },
    /// Each row matches at most one of a column's value-level entries, named
    /// by their identifiers.
    ValueEntries(Vec<(String, WhereClause)>),
    /// A value-level entry, named by its identifier, matches at least one
    /// row; checked at `warning`.
    ValueEntryMatch {
        id: String,
        clause: WhereClause,
    },
    Range {
        min: Option<Value>,
        max: Option<Value>,
    },
    /// Unicode scalar count, rather than encoded bytes or grapheme clusters.
    MaxLength(usize),
    /// An exact authored maximum that is wider than the machine count.
    WideMaxLength(RowCountBound),
    /// Search the portable pattern as written over each present string.
    Matches(crate::portable_regex::Pattern),
    /// A core-owned declaration finding deferred until dataset verification.
    InvalidDiagnostic(crate::diagnostic::Diagnostic),
    /// A compiler finding evaluated in declaration order after output keys.
    InvalidDeclaration {
        code: crate::diagnostic::ConditionCode,
        reason: String,
    },
    /// `require` must hold on every row that `when`, if present, binds (CHECK-12).
    Assert {
        when: Option<BoundPredicate>,
        require: BoundPredicate,
    },
    /// Compiler checkpoint before a later deferred declaration error; emits no record.
    PredicateDeclaration(BoundPredicate),
    Unique(Vec<usize>),
    /// Every named column is present together or missing together on each row.
    AllOrNone(Vec<usize>),
    RowCount {
        min: Option<i64>,
        max: Option<i64>,
    },
    /// Exact authored count bounds retained as canonical decimal integers.
    WideRowCount {
        min: Option<RowCountBound>,
        max: Option<RowCountBound>,
    },
    /// Count filtered rows in each constructed-output group.
    ScopedRowCount(RowCount),
}

/// One value-level where clause over completed `str` columns: a row is
/// selected when every term selects it.
#[derive(Clone, Debug, PartialEq)]
pub struct WhereClause {
    pub terms: Vec<WhereTerm>,
}
impl WhereClause {
    pub fn selects(&self, row: &[Value]) -> bool {
        self.terms.iter().all(|term| {
            term.selects(match &row[term.column] {
                Value::Str(value) => Some(value),
                _ => None,
            })
        })
    }
    /// The columns the clause reads.
    pub fn columns(&self) -> impl Iterator<Item = usize> + '_ {
        self.terms.iter().map(|term| term.column)
    }
}

/// One where column of a clause and the values it lists (SUB-12). A term
/// selects a row whose column holds one of the values, or, when it
/// `excludes` (NE and NOTIN), a row whose column holds none of them, a
/// missing value included.
#[derive(Clone, Debug, PartialEq)]
pub struct WhereTerm {
    pub column: usize,
    pub values: Vec<String>,
    pub excludes: bool,
}
impl WhereTerm {
    /// A term over `column` listing `values`, as a bare list writes it.
    pub fn listed(column: usize, values: Vec<String>) -> Self {
        Self {
            column,
            values,
            excludes: false,
        }
    }
    /// Whether the term selects a row whose column holds `value`, `None`
    /// when the value is missing.
    pub fn selects(&self, value: Option<&str>) -> bool {
        value.is_some_and(|value| self.values.iter().any(|listed| listed == value)) != self.excludes
    }
}

/// Rows selected by where clauses: the rows any clause selects when
/// `include`, and every other row otherwise.
#[derive(Clone, Debug, PartialEq)]
pub struct RowScope {
    pub clauses: Vec<WhereClause>,
    pub include: bool,
}
impl RowScope {
    pub fn selects(&self, row: &[Value]) -> bool {
        self.clauses.iter().any(|clause| clause.selects(row)) == self.include
    }
}

/// A column's inventory vocabulary resolved per row: the first override whose
/// clause selects the row decides, and `default` decides every other row.
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryScope {
    pub default: bool,
    pub overrides: Vec<(WhereClause, bool)>,
}
impl InventoryScope {
    pub fn selects(&self, row: &[Value]) -> bool {
        self.overrides
            .iter()
            .find(|(clause, _)| clause.selects(row))
            .map_or(self.default, |(_, enabled)| *enabled)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ValueCodelist {
    pub id: String,
    pub values: Vec<Value>,
    /// The clauses of the entries binding this codelist.
    pub clauses: Vec<WhereClause>,
    /// Whether the column itself binds this codelist.
    pub default: bool,
    /// The authored path of the first declaration binding this codelist: the
    /// column's own `submission.codelist` or a value-level entry's.
    pub path: String,
}

impl Check {
    /// The plain `not_missing`: every row, no share allowed.
    pub const NOT_MISSING: Self = Self::NotMissing {
        when: None,
        max_fraction: None,
    };
    /// The completed columns a column check's `when` reads, each once, in
    /// first-read order (CHECK-5).
    pub fn when_columns(&self) -> Vec<usize> {
        let mut columns = Vec::new();
        if let Self::NotMissing {
            when: Some(when), ..
        } = self
        {
            for binding in when.bindings() {
                if let Read::Column(column) = binding.read {
                    if !columns.contains(&column) {
                        columns.push(column);
                    }
                }
            }
        }
        columns
    }
    /// The where clauses a value-level check selects its rows by.
    pub fn clauses(&self) -> Vec<&WhereClause> {
        match self {
            Self::Mandatory(Some(scope))
            | Self::DataType {
                scope: Some(scope), ..
            }
            | Self::ValueMaxLength { scope, .. } => scope.clauses.iter().collect(),
            Self::ValueCodelists {
                overrides,
                bindings,
            } => overrides
                .iter()
                .chain(bindings.iter().flat_map(|binding| &binding.clauses))
                .collect(),
            Self::Inventory {
                scope: Some(scope), ..
            } => scope.overrides.iter().map(|(clause, _)| clause).collect(),
            Self::ValueEntries(entries) => entries.iter().map(|(_, clause)| clause).collect(),
            Self::ValueEntryMatch { clause, .. } => vec![clause],
            _ => Vec::new(),
        }
    }
    /// Resolve a retained terminology observation against its admitted binding.
    pub fn codelist(&self, identifier: &str) -> Option<(&str, &[Value])> {
        match self {
            Self::Codelist { id, values } if id == identifier => Some((id, values)),
            Self::ValueCodelists { bindings, .. } => bindings
                .iter()
                .find(|binding| binding.id == identifier)
                .map(|binding| (binding.id.as_str(), binding.values.as_slice())),
            _ => None,
        }
    }
    /// Shared mathematical count comparison for legacy probes and specifications.
    pub fn row_count_accepts(&self, count: usize) -> Option<bool> {
        use std::cmp::Ordering;
        Some(match self {
            Self::RowCount { min, max } => {
                let count = count as i128;
                !min.is_some_and(|min| count < i128::from(min))
                    && !max.is_some_and(|max| count > i128::from(max))
            }
            Self::WideRowCount { min, max } => {
                min.as_ref()
                    .is_none_or(|min| min.compare_count(count) != Ordering::Greater)
                    && max
                        .as_ref()
                        .is_none_or(|max| max.compare_count(count) != Ordering::Less)
            }
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verification {
    pub path: String,
    pub check: Check,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerificationSeverity {
    #[default]
    Error,
    Warning,
}
/// The spelling of each [`VerificationSeverity`], in declaration order.
pub const SEVERITIES: [&str; 2] = ["error", "warning"];
impl VerificationSeverity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => SEVERITIES[0],
            Self::Warning => SEVERITIES[1],
        }
    }
}

/// Checks for one declared column, in authored order and its own ID namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnVerifications {
    pub column: usize,
    pub checks: Vec<Verification>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("dataset invalid columns")]
    InvalidColumns,
    #[error("dataset invalid source")]
    InvalidSource,
    #[error("dataset invalid window")]
    InvalidWindow,
    #[error("dataset invalid intermediate")]
    InvalidIntermediate,
    #[error("dataset invalid function")]
    InvalidFunction,
}

/// An immutable bound plan. The specification compiler checks every reference
/// and phase dependency before it builds one.
#[derive(Clone, Debug, PartialEq)]
pub struct DatasetPlan {
    source: TableSchema,
    secondary: Vec<SecondarySource>,
    intermediates: Vec<Intermediate>,
    output: TableSchema,
    output_types: Vec<crate::result_type::ResultType>,
    templates: Vec<RowTemplate>,
    row_contexts: Vec<Option<AggregateRowContext>>,
    columns: Vec<ColumnBinding>,
    keys: Vec<usize>,
    verifications: Vec<Verification>,
    column_verifications: Vec<ColumnVerifications>,
    warning_verifications: std::collections::BTreeSet<String>,
    conversion_handlers: Vec<ConversionHandler>,
    conversion_sites: BTreeMap<String, usize>,
    output_order: Vec<OrderTerm>,
    /// Inputs whose every implicit join must find a record (LOOKUP-24).
    required_matches: std::collections::BTreeSet<String>,
}
impl DatasetPlan {
    /// A single-source plan whose column bindings serve every template.
    #[cfg(test)]
    pub(crate) fn new(
        source: TableSchema,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Self {
        Self::new_with_intermediates(
            SourceSchemas {
                primary: source,
                secondary: Vec::new(),
            },
            Vec::new(),
            output,
            templates,
            columns,
            keys,
            verifications,
        )
    }

    /// A plan whose column-phase assignments each serve every template.
    pub(crate) fn new_with_intermediates(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Self {
        let columns = columns
            .into_iter()
            .map(|assignment| ColumnBinding::every_template(assignment, &templates))
            .collect();
        Self::new_with_bindings(
            sources,
            intermediates,
            output,
            templates,
            columns,
            keys,
            verifications,
        )
    }

    /// Each template completes its own row-phase columns; the column-phase
    /// bindings complete the rest, each for its own templates (#2829). Only
    /// the specification compiler builds a plan, after it found nothing, so
    /// construction checks nothing: the compiler's findings are the only
    /// validation (#2843).
    pub(crate) fn new_with_bindings(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<ColumnBinding>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Self {
        Self::assemble(
            sources,
            intermediates,
            output,
            templates,
            columns,
            keys,
            verifications,
        )
    }

    /// A donor plan has no publication key gate, and a derived column of its
    /// output may have a type decided per value (`output_types`, EXPR-14).
    pub(crate) fn new_donor(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        output_types: Vec<crate::result_type::ResultType>,
    ) -> Self {
        let columns = columns
            .into_iter()
            .map(|assignment| ColumnBinding::every_template(assignment, &templates))
            .collect();
        Self {
            output_types,
            ..Self::assemble(
                sources,
                intermediates,
                output,
                templates,
                columns,
                Vec::new(),
                Vec::new(),
            )
        }
    }

    /// A plan whose output columns each have their declared type.
    fn assemble(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<ColumnBinding>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Self {
        let output_types = output
            .columns()
            .iter()
            .map(|column| crate::result_type::ResultType::column(column.kind))
            .collect();
        Self {
            source: sources.primary,
            secondary: sources.secondary,
            intermediates,
            output,
            output_types,
            templates,
            columns,
            keys,
            verifications,
            row_contexts: Vec::new(),
            column_verifications: Vec::new(),
            warning_verifications: Default::default(),
            conversion_handlers: Vec::new(),
            conversion_sites: BTreeMap::new(),
            output_order: Vec::new(),
            required_matches: Default::default(),
        }
    }
}

impl DatasetPlan {
    /// Presentation terms, separate from derivation and window order.
    pub(crate) fn with_output_order(mut self, terms: Vec<OrderTerm>) -> Self {
        self.output_order = terms;
        self
    }
    /// Presentation occurs only after all keys and verifications succeed.
    pub fn output_order(&self) -> &[OrderTerm] {
        &self.output_order
    }
    /// Name the inputs declaring `require_match: true`: a read through an
    /// implicit join to one of them that finds no record fails, unless the
    /// read declares its own `absent` answer (LOOKUP-24).
    pub(crate) fn with_required_matches(
        mut self,
        inputs: impl IntoIterator<Item = String>,
    ) -> Self {
        self.required_matches = inputs.into_iter().collect();
        self
    }
    /// Whether every implicit join through this input must find a record.
    pub fn requires_match(&self, input: &str) -> bool {
        self.required_matches.contains(input)
    }
    /// The inputs declaring `require_match: true`.
    pub(crate) fn required_matches(&self) -> &std::collections::BTreeSet<String> {
        &self.required_matches
    }
    /// The checks declared at warning severity, by path; every other check
    /// is an error.
    pub(crate) fn with_warning_verifications(mut self, paths: Vec<String>) -> Self {
        self.warning_verifications = paths.into_iter().collect();
        self
    }
    pub fn verification_severity(&self, path: &str) -> VerificationSeverity {
        if self.warning_verifications.contains(path) {
            VerificationSeverity::Warning
        } else {
            VerificationSeverity::Error
        }
    }
    /// Column checks, in declared column order.
    pub(crate) fn with_column_verifications(mut self, groups: Vec<ColumnVerifications>) -> Self {
        self.column_verifications = groups;
        self
    }

    /// Borrow checks in declared column order, independently of dependency order.
    pub fn column_verifications(&self) -> &[ColumnVerifications] {
        &self.column_verifications
    }
    /// Borrow admitted source without exposing mutation.
    pub fn source(&self) -> &TableSchema {
        &self.source
    }
    /// Borrow admitted secondary without exposing mutation.
    pub fn secondary(&self) -> &[SecondarySource] {
        &self.secondary
    }
    /// Borrow admitted intermediates without exposing mutation.
    pub fn intermediates(&self) -> &[Intermediate] {
        &self.intermediates
    }
    /// Borrow admitted output without exposing mutation.
    pub fn output(&self) -> &TableSchema {
        &self.output
    }
    /// Physical layout slots do not promise a type for untyped donor results.
    pub(crate) fn output_kind(&self, column: usize) -> Option<crate::value::ColumnType> {
        self.output_type(column).column_type()
    }
    /// An output column's result type (EXPR-14): its declared type, or an
    /// untyped donor result's, which may be decided per value.
    pub(crate) fn output_type(&self, column: usize) -> crate::result_type::ResultType {
        self.output_types
            .get(column)
            .copied()
            .unwrap_or(crate::result_type::ResultType::UNKNOWN)
    }
    /// Borrow admitted templates without exposing mutation.
    pub fn templates(&self) -> &[RowTemplate] {
        &self.templates
    }
    /// The admitted column-phase assignments, one per column binding, in
    /// column order.
    pub fn columns(&self) -> impl Iterator<Item = &Assignment> + '_ {
        self.columns.iter().map(|binding| &binding.assignment)
    }
    /// Borrow the admitted column bindings, in column order, without exposing
    /// mutation.
    pub fn column_bindings(&self) -> &[ColumnBinding] {
        &self.columns
    }
    /// Borrow admitted keys without exposing mutation.
    pub fn keys(&self) -> &[usize] {
        &self.keys
    }
    /// Borrow admitted verifications without exposing mutation.
    pub fn verifications(&self) -> &[Verification] {
        &self.verifications
    }
    /// Borrow admitted conversion handlers without exposing mutation.
    pub fn conversion_handlers(&self) -> &[ConversionHandler] {
        &self.conversion_handlers
    }
    /// Resolve the already-admitted replacement for one assignment path.
    pub fn conversion_handler(&self, path: &str) -> Option<&crate::conversion::LiteralHandler> {
        self.conversion_sites
            .get(path)
            .map(|index| &self.conversion_handlers[*index].handler)
    }
}

#[cfg(test)]
mod admission_boundaries;
#[cfg(test)]
mod partitioned_execution;

mod execution;
pub use execution::{
    CheckRecord, CodelistObservation, Dataset, Division, Execution, ExecutionAttempt,
    ExecutionError, ExecutionEvent, ExecutionPhase, Partitioning, Resource, RowCountObservation,
    RowIdentity, TemplateRows,
};
