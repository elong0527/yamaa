//! Closed typed dataset plan for bounded row construction and column derivation.
//!
//! This is an internal bridge target, not a specification parser or public backend.
//! Unsupported syntax must be rejected by a compiler before creating this plan.

#[path = "dataset_diagnostics.rs"]
mod diagnostics;

#[path = "dataset_conversion.rs"]
mod conversion;
#[path = "dataset_intermediates.rs"]
mod intermediates;
#[path = "dataset_relation_predicate.rs"]
mod relation_predicates;
pub use yamaa_core::dataset::{
    AggregateBinding, AggregateBindingError, AggregateDerivation, AggregatePhase,
    AggregateRelation, AggregateRowContext, AggregateSource, Assignment, BoundAggregate,
    BoundFunction, BoundNumeric, BoundOdm, BoundProjectFunction, CalendarUnit, CaseBranch, Check,
    ColumnVerifications, ContextWindow, ContextualMatch, ConversionHandler, CountBoundError,
    CurrentField, CurrentRead, DatasetPlan, DateBounds, DayDeclaration, DayRule, Expression,
    FirstAvailable, Flag, FlagBinding, FlagError, FlagRead, FunctionArgument, FunctionInput,
    Imputation, ImputationKind, ImputationStep, Intermediate, IntermediateDerivations,
    IntermediateKey, IntermediateSource, IntermediateUnique, InvalidDigits, InvalidTemporal,
    InventoryScope, JoinedRead, Keep, Lookup, MatchKey, MatchRead, MatchValue, OdmRead, OdmScope,
    OdmTemplateScope, OrderTerm, PartialDate, PlanError, RecordMatchKey, RelationField,
    RelationFieldOrigin, RelationPredicate, RelationRange, RelationRangeBound, RelationRangeError,
    RelationRangeFailure, RelationRangeTypes, RelationShape, RelationShapeError,
    RelationShapeLimits, Rounding, RowCount, RowCountBound, RowLookup, RowMatchKey, RowMode,
    RowTemplate, SecondarySource, SelectionRead, SelectionSource, SourceSchemas, SourceSelection,
    StringCase, StringExtract, StringPad, StringTemplate, Temporal, TemporalKind, TimeRule,
    ValueCodelist, Verification, VerificationSeverity, Window, WindowKind,
};
#[path = "dataset_case_predicate.rs"]
mod case_predicates;
#[path = "dataset_flag.rs"]
mod flags;
#[path = "dataset_functions.rs"]
mod functions;
pub use crate::function_invocation::FunctionPort;
pub(crate) fn unavailable_functions<E>() -> impl FunctionPort<Error = E> {
    crate::function_invocation::NoFunctions::new()
}
#[path = "dataset_aggregate.rs"]
mod aggregates;
#[path = "dataset_temporal_imputation.rs"]
mod imputation;
#[path = "dataset_joined.rs"]
mod joined;
#[path = "dataset_keys.rs"]
mod key_grain;
#[path = "dataset_lookup.rs"]
mod lookup;
#[path = "dataset_mapping.rs"]
mod mapping_cut;
#[path = "dataset_numeric.rs"]
mod numeric;
#[path = "dataset_odm.rs"]
mod odm;
#[path = "dataset_ordering.rs"]
mod ordering;
#[path = "dataset_row_drivers.rs"]
mod row_drivers;
#[path = "dataset_row_windows.rs"]
mod row_windows;
#[path = "dataset_scalar.rs"]
mod scalar;
#[path = "dataset_self.rs"]
mod self_relations;
#[path = "dataset_text_case.rs"]
mod str_case;
#[path = "dataset_text_extract.rs"]
mod str_extract;
#[path = "dataset_text_pad.rs"]
mod str_pad;
#[path = "dataset_text_template.rs"]
mod str_template;
#[path = "dataset_windows.rs"]
mod windows;

use crate::{
    dataset_budget::Budget,
    numeric_lifecycle::{HandlerCount, HandlerCountOverflow, HandlerCounter, HandlerKind},
    table_grouping::{partition, GroupingError},
    table_reduction::{count_selected, reduce_column, TableReductionError},
};
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec, vec::Vec};
use core::convert::Infallible;
use yamaa_core::{
    bound_expression::BoundPredicate,
    conversion::{convert, ConversionError},
    reduction::NumericReducer,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::{Value, ValueType},
};

/// Contiguous stages of one attempt; clocks and measurement storage belong to adapters.
/// Derivation includes source reads, callbacks and per-value result conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionPhase {
    Admission,
    Derivation,
    OutputKeys,
    Verification,
    Finished,
}

/// Caller-selected capacity limits, not normative language limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub source_rows: usize,
    pub output_rows: usize,
    pub output_cells: usize,
    pub key_cells: usize,
    pub work_cells: usize,
    pub scalar_text_bytes: usize,
    pub output_text_bytes: usize,
    pub identity_cells: usize,
    pub identity_text_bytes: usize,
}

/// Cumulative resource failures are distinct from normative language conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    WorkCells,
    ScalarTextBytes,
    OutputTextBytes,
    IdentityCells,
    IdentityTextBytes,
    PredicateWork,
    PredicateResolutions,
    PredicateTextBytes,
    PredicateLikeWork,
    PredicateRegexSubjectBytes,
    PredicateRegexWork,
    PredicateRegexStateCells,
}

/// Complete owned rows are exposed only after output identity and checks succeed.
#[derive(Debug, PartialEq)]
pub struct Dataset {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}

impl Dataset {
    /// Inspect accepted rows in template/group/source order and declared column order.
    pub fn rows(&self) -> &[Vec<Value>] {
        &self.rows
    }
}

impl TableAccess for Dataset {
    type Error = Infallible;
    /// Retain declared schema even when every template produces zero rows.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    /// Count completed rows independently of missing cells.
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    /// Borrow a checked owned cell; missing is never an out-of-bounds substitute.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

/// Output identity remains available after a failed artifact has been discarded.
#[derive(Clone, Debug, PartialEq)]
pub struct RowIdentity {
    pub position: usize,
    pub values: Vec<Value>,
}

/// Portable observations retain full identities; a host may format bounded samples.
#[derive(Clone, Debug, PartialEq)]
pub struct RowCountObservation {
    pub counts: Vec<usize>,
    pub denominators: Option<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckRecord {
    pub path: String,
    pub code: yamaa_core::diagnostic::ConditionCode,
    pub evaluated_count: usize,
    pub failed_count: usize,
    pub output_rows: usize,
    pub offending_rows: Vec<RowIdentity>,
    /// Full offending values survive discarded output, under identity quotas.
    pub codelist: Option<CodelistObservation>,
    pub inventory: Option<Vec<Value>>,
    pub row_count: Option<RowCountObservation>,
}

impl CheckRecord {
    pub fn definition(&self) -> yamaa_core::diagnostic::Definition {
        self.code.definition()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodelistObservation {
    pub id: String,
    pub values: Vec<Value>,
}

#[derive(Debug, PartialEq)]
pub struct Execution {
    pub dataset: Dataset,
    pub verifications: Vec<CheckRecord>,
}

/// An attempted run retains handler evidence even when no dataset can be accepted.
#[derive(Debug, PartialEq)]
pub struct ExecutionAttempt<E> {
    pub result: Result<Execution, Box<ExecutionError<E>>>,
    pub handler_counts: Vec<HandlerCount>,
    /// Completed checks preceding an unrelated failure, outside its diagnostic.
    pub retained_verifications: Vec<CheckRecord>,
}

#[derive(Default)]
struct AttemptObservations {
    handlers: HandlerCounter,
    verifications: Vec<CheckRecord>,
}

#[derive(Debug, PartialEq)]
pub enum ExecutionError<E> {
    /// A pure scalar operation's already classified cause and original field path.
    ScalarDiagnostic(yamaa_core::diagnostic::Diagnostic),
    IntermediateVerifications(Vec<yamaa_core::diagnostic::Diagnostic>),
    SemanticDiagnostic {
        diagnostic: yamaa_core::diagnostic::Diagnostic,
        identity: Option<RowIdentity>,
    },
    VerificationDiagnostic {
        diagnostic: yamaa_core::diagnostic::Diagnostic,
        records: Vec<CheckRecord>,
    },
    /// Callback registry does not match a declared signature; detected before table access.
    FunctionBinding {
        slot: usize,
    },
    /// Fatal invocation failure, before result conversion or its recovery handler.
    Function {
        path: String,
        error: crate::function_invocation::InvocationFailure<E>,
        identity: Option<RowIdentity>,
    },
    /// Versionless package failure retains the original host payload and name/call.
    ProjectFunction {
        path: String,
        error: crate::function_invocation::InvocationFailure<
            E,
            yamaa_core::function_signature::ProjectFunctionIdentity,
        >,
        identity: Option<RowIdentity>,
    },
    Numeric {
        error: yamaa_core::numeric_compiler::CompiledEvaluationError<Infallible>,
        identity: Option<RowIdentity>,
    },
    HandlerAccounting(HandlerCountOverflow),
    MultipleMatches {
        path: String,
        dataset: String,
        intermediate: String,
        match_count: usize,
        matched_key: Vec<(String, Value)>,
        identity: Option<RowIdentity>,
    },
    UnmatchedKey {
        path: String,
        dataset: String,
        intermediate: String,
        matched_key: Vec<(String, Value)>,
        identity: Option<RowIdentity>,
    },
    SecondaryCell {
        path: String,
        source: usize,
        source_row: usize,
        error: CellError<E>,
    },
    BaselineAmbiguity {
        path: String,
        column: String,
        date: Value,
        match_count: usize,
        partition: Vec<(String, Value)>,
    },
    MultipleValues {
        path: String,
        identifier: String,
        value_count: usize,
        identity: Option<RowIdentity>,
    },
    VerificationDeclaration {
        path: String,
        code: yamaa_core::diagnostic::ConditionCode,
        reason: String,
        records: Vec<CheckRecord>,
    },
    VerificationPredicate {
        error: yamaa_core::predicate::EvaluationError<CellError<Infallible>>,
        records: Vec<CheckRecord>,
    },
    Predicate {
        source_row: usize,
        error: yamaa_core::predicate::EvaluationError<CellError<E>>,
    },
    Limit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
    },
    SchemaMismatch,
    Capacity,
    Allocation,
    Grouping(GroupingError<E>),
    Cell {
        path: String,
        source_row: usize,
        error: CellError<E>,
    },
    ReductionType {
        path: String,
        expression: String,
        reducer: NumericReducer,
        source: String,
        actual: ValueType,
    },
    Reduction {
        path: String,
        error: TableReductionError<E>,
        identity: Option<RowIdentity>,
    },
    Conversion {
        path: String,
        output_row: usize,
        error: ConversionError,
        identity: Option<RowIdentity>,
    },
    KeyFailures(Vec<CheckRecord>),
    VerificationFailures(Vec<CheckRecord>),
    OutputGrouping(GroupingError<Infallible>),
}

/// A selection identity is distinct from source coordinates and diagnostic row position.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SelectionIdentity {
    /// Key probes cannot read named selections in an admitted plan.
    Probe,
    Candidate(usize),
}
/// Keep source provenance per candidate for the later whole-column phase.
struct Candidate {
    template: Option<usize>,
    selection_identity: SelectionIdentity,
    members: Vec<usize>,
    values: Vec<Value>,
    completed: Vec<bool>,
}

/// Copy the selected normalized value, retaining all temporal precision metadata.
pub(crate) fn own(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Missing,
        ValueRef::Str(value) => Value::Str(value.into()),
        ValueRef::Int(value) => Value::Int(value),
        ValueRef::Float(value) => Value::Float(value),
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(value) => Value::Date(value),
        ValueRef::DateTime(value) => Value::DateTime(value),
    }
}

/// Keep predicate resource policies separate from portable language conditions.
pub(crate) fn predicate_limit<E>(limit: yamaa_core::predicate::LimitError) -> ExecutionError<E> {
    use yamaa_core::predicate::Resource as P;
    let resource = match limit.resource {
        P::Work => Resource::PredicateWork,
        P::Resolutions => Resource::PredicateResolutions,
        P::TextBytes => Resource::PredicateTextBytes,
        P::LikeWork => Resource::PredicateLikeWork,
        P::RegexSubjectBytes => Resource::PredicateRegexSubjectBytes,
        P::RegexWork => Resource::PredicateRegexWork,
        P::RegexStateCells => Resource::PredicateRegexStateCells,
        P::Nodes | P::Depth => unreachable!("predicate structure was admitted before execution"),
    };
    ExecutionError::Limit {
        resource,
        limit: limit.limit,
        required: None,
    }
}

/// Resources and semantic observations belong to the same attempted run.
struct EvaluationState<'a, E> {
    primary: &'a dyn TableAccess<Error = E>,
    functions: &'a mut dyn FunctionPort<Error = E>,
    secondary: &'a [&'a dyn TableAccess<Error = E>],
    self_rows: &'a [Candidate],
    budget: &'a mut Budget,
    handlers: &'a mut HandlerCounter,
    intermediates: &'a mut intermediates::Run,
    output_rows: &'a [Candidate],
    aggregate_row: Option<Candidate>,
    output_offset: usize,
    windows: &'a mut windows::Cache,
}

/// Evaluate one admitted assignment and finish conversion before publishing it.
fn evaluate<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let value =
        evaluate_raw_retained(table, assignment, candidate, plan, row, limits, state, true)?;
    finish(
        value,
        assignment,
        candidate,
        plan,
        row,
        state.budget,
        state.handlers,
    )
}

fn evaluate_raw<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    evaluate_raw_retained(
        table, assignment, candidate, plan, row, limits, state, false,
    )
}

#[allow(clippy::too_many_arguments)]
fn evaluate_raw_retained<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    limits: Limits,
    state: &mut EvaluationState<'_, T::Error>,
    retained: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    if let Expression::Function(function) = &assignment.expression {
        state.budget.work(1, 1)?;
        return functions::evaluate(
            function,
            table,
            candidate,
            functions::Context {
                plan,
                row,
                assignment,
            },
            state,
        );
    }
    if let Expression::ProjectFunction(function) = &assignment.expression {
        state.budget.work(1, 1)?;
        return functions::evaluate_project(
            function,
            table,
            candidate,
            functions::Context {
                plan,
                row,
                assignment,
            },
            state,
        );
    }
    if matches!(
        assignment.expression,
        Expression::Window(_) | Expression::ContextWindow(_)
    ) {
        return state.windows.value(
            assignment,
            state.output_rows,
            plan,
            table,
            row - state.output_offset,
            limits,
            state.budget,
            state.handlers,
        );
    }
    if let Expression::Joined(read) = &assignment.expression {
        return joined::read(read, assignment, candidate, plan, row, state);
    }
    if let Expression::JoinedSource {
        read,
        filter,
        selection,
    } = &assignment.expression
    {
        return joined::read_options(
            read,
            filter.as_ref(),
            selection.as_ref(),
            assignment,
            candidate,
            plan,
            row,
            state,
        );
    }
    if let Expression::Intermediate { index, column } = assignment.expression {
        state.budget.work(1, 1)?;
        let value = intermediates::read(
            index, column, assignment, candidate, plan, row, table, state,
        )?;
        return Ok(value);
    }
    if let Expression::SourceBinding {
        source,
        absent,
        variable_absent,
        path,
        ..
    } = &assignment.expression
    {
        if *variable_absent {
            let absent = absent.as_ref().expect("admitted absence handler");
            state.budget.work(1, 1)?;
            if let Value::Str(text) = absent {
                state.budget.scalar_text(text.len())?;
            }
            state
                .handlers
                .record(&alloc::format!("{path}.absent"), HandlerKind::Absent)
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            return Ok(absent.clone());
        }
        return evaluate_raw_retained(table, source, candidate, plan, row, limits, state, retained);
    }
    if let Expression::FirstAvailable(selection) = &assignment.expression {
        for operand in selection.sources() {
            state.budget.work(1, 1)?;
            let value = match &operand.read {
                yamaa_core::dataset::SelectionRead::Expression(source) => evaluate_raw_retained(
                    table, source, candidate, plan, row, limits, state, retained,
                )
                .map_err(|mut error| {
                    // Collection cardinality belongs to the enclosing selection.
                    // Keep operand metadata for handlers and every other failure.
                    if let ExecutionError::MultipleValues { path, .. } = error.as_mut() {
                        path.clone_from(&assignment.path);
                    }
                    error
                })?,
                yamaa_core::dataset::SelectionRead::Source(column) => {
                    let source_row = candidate.members[0];
                    let value = table.cell(source_row, *column).map_err(|error| {
                        Box::new(ExecutionError::Cell {
                            path: operand.path.clone(),
                            source_row,
                            error,
                        })
                    })?;
                    if let ValueRef::Str(text) = value {
                        state.budget.scalar_text(text.len())?;
                    }
                    own(value)
                }
                yamaa_core::dataset::SelectionRead::Column(column) => {
                    let value = &candidate.values[*column];
                    if let Value::Str(text) = value {
                        state.budget.scalar_text(text.len())?;
                    }
                    value.clone()
                }
                yamaa_core::dataset::SelectionRead::Collect {
                    column,
                    identifier,
                    filter,
                } => {
                    state.budget.work(candidate.members.len(), 1)?;
                    key_grain::collect_bound(
                        table,
                        key_grain::Collection {
                            column: *column,
                            identifier,
                            filter: filter.as_deref(),
                            selection: None,
                        },
                        key_grain::CollectionContext {
                            path: &operand.path,
                            candidate,
                            plan,
                            row,
                        },
                        state.budget,
                        state.handlers,
                    )
                    .map_err(|mut error| {
                        if let ExecutionError::MultipleValues { path, .. } = error.as_mut() {
                            *path = assignment.path.clone();
                        }
                        error
                    })?
                }
            };
            if value != Value::Missing {
                return Ok(value);
            }
        }
        if let Value::Str(text) = selection.missing() {
            state.budget.scalar_text(text.len())?;
        }
        return Ok(selection.missing().clone());
    }
    if let Expression::Flag(flag) = &assignment.expression {
        return flags::evaluate(flag, table, candidate, plan, row, assignment, state);
    }
    if let Expression::Case(branches) = &assignment.expression {
        for branch in branches {
            state.budget.work(1, 1)?;
            let selected = match &branch.when {
                None => true,
                Some(when) => {
                    let truth = case_predicates::evaluate(
                        when, table, candidate, plan, row, assignment, state,
                    )?;
                    truth == yamaa_core::predicate::Truth::True
                }
            };
            if selected {
                return evaluate_raw_retained(
                    table,
                    &branch.assignment,
                    candidate,
                    plan,
                    row,
                    limits,
                    state,
                    retained,
                );
            }
        }
        return Ok(Value::Missing);
    }
    if let Expression::MappingCut { source, operation } = &assignment.expression {
        return mapping_cut::evaluate(
            assignment, source, operation, table, candidate, plan, row, limits, state, retained,
        );
    }
    if let Expression::Round { source, rounding } = &assignment.expression {
        state.budget.work(1, 1)?;
        let value = evaluate_raw(table, source, candidate, plan, row, limits, state)?;
        return rounding
            .apply(ValueRef::from(&value))
            .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)));
    }
    if let Expression::StrTemplate {
        operands,
        operation,
    } = &assignment.expression
    {
        return str_template::evaluate(
            operands,
            operation,
            table,
            candidate,
            plan,
            row,
            limits,
            state,
            retained
                && plan.output().columns()[assignment.column].kind
                    == yamaa_core::value::ColumnType::Str,
        );
    }
    if matches!(
        &assignment.expression,
        Expression::Extreme { .. } | Expression::StrConcat { .. } | Expression::StrContains { .. }
    ) {
        return scalar::evaluate(assignment, table, candidate, plan, row, limits, state);
    }
    if let Expression::StrExtract { source, operation } = &assignment.expression {
        return str_extract::evaluate(
            source, operation, table, candidate, plan, row, limits, state,
        );
    }
    if let Expression::StrCase { source, operation } = &assignment.expression {
        return str_case::evaluate(
            source, operation, table, candidate, plan, row, limits, state,
        );
    }
    if let Expression::StrPad { source, operation } = &assignment.expression {
        return str_pad::evaluate(
            source, operation, table, candidate, plan, row, limits, state,
        );
    }
    if let Expression::Imputation {
        operands,
        operation,
    } = &assignment.expression
    {
        return imputation::evaluate(
            operands, operation, table, candidate, plan, row, limits, state,
        );
    }
    if let Expression::Temporal {
        operands,
        operation,
    } = &assignment.expression
    {
        state.budget.work(1, 1)?;
        let mut dates = Vec::with_capacity(operands.len());
        for (index, operand) in operands.iter().enumerate() {
            let value = evaluate_raw(table, operand, candidate, plan, row, limits, state)?;
            dates.push(
                operation
                    .operand(index, ValueRef::from(&value))
                    .expect("dataset admission fixes operand index")
                    .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)))?,
            );
        }
        return Ok(operation
            .apply(&dates)
            .expect("dataset admission fixes temporal arity"));
    }
    if let Expression::Compute(expression) = &assignment.expression {
        return numeric::evaluate(expression, table, candidate, plan, row, assignment, state);
    }
    if let Expression::Aggregate(expression) = &assignment.expression {
        return aggregates::evaluate(expression, table, candidate, plan, row, state);
    }
    if let Expression::Odm(expression) = &assignment.expression {
        return odm::evaluate(expression, table, candidate, plan, row, state);
    }
    let EvaluationState {
        budget,
        handlers,
        secondary,
        ..
    } = state;
    budget.work(1, 1)?;
    let reads = match &assignment.expression {
        Expression::Source(_) => 1,
        Expression::Reduce { .. } | Expression::Count { .. } | Expression::Collect { .. } => {
            candidate.members.len()
        }
        _ => 0,
    };
    budget.work(reads, 1)?;
    let value = match &assignment.expression {
        Expression::Function(_) | Expression::ProjectFunction(_) => {
            unreachable!("callback arguments resolve through complete run state")
        }
        Expression::Window(_) | Expression::ContextWindow(_) => {
            unreachable!("window assignments execute by whole column")
        }
        Expression::Intermediate { .. } => unreachable!("intermediates execute through run state"),
        Expression::Joined(_) | Expression::JoinedSource { .. } => {
            unreachable!("joined reads execute through run state")
        }
        Expression::Literal(value) => {
            if let Value::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            value.clone()
        }
        Expression::Extreme { .. }
        | Expression::StrConcat { .. }
        | Expression::StrContains { .. }
        | Expression::Flag(_)
        | Expression::Compute(_)
        | Expression::Aggregate(_)
        | Expression::Odm(_)
        | Expression::Case(_)
        | Expression::MappingCut { .. }
        | Expression::Round { .. }
        | Expression::Temporal { .. }
        | Expression::Imputation { .. }
        | Expression::StrCase { .. }
        | Expression::StrPad { .. }
        | Expression::StrExtract { .. }
        | Expression::StrTemplate { .. } => {
            unreachable!("recursive evaluation handled above")
        }
        Expression::Column(column) => {
            let value = &candidate.values[*column];
            if let Value::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            value.clone()
        }
        Expression::Source(column) => {
            let source_row = candidate.members[0];
            let value = table.cell(source_row, *column).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: assignment.path.clone(),
                    source_row,
                    error,
                })
            })?;
            if let ValueRef::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            own(value)
        }
        Expression::RowLookup(lookup) => lookup::read_row(
            table,
            secondary[lookup.source],
            lookup,
            &lookup::Context {
                assignment,
                candidate,
                plan,
                row,
            },
            budget,
        )?,
        Expression::Lookup(lookup) => lookup::read(
            secondary[lookup.source],
            lookup,
            assignment,
            candidate,
            plan,
            row,
            budget,
        )?,
        Expression::FirstAvailable(_) | Expression::SourceBinding { .. } => {
            unreachable!("source recursion is handled above")
        }
        Expression::SourceLookup {
            lookup: bound,
            filter,
            selection,
        } => lookup::read_source(
            secondary[bound.source],
            bound,
            filter.as_ref(),
            selection.as_ref(),
            &lookup::Context {
                assignment,
                candidate,
                plan,
                row,
            },
            budget,
            handlers,
        )?,
        Expression::Collect { .. } => {
            key_grain::collect(table, assignment, candidate, plan, row, budget, handlers)?
        }
        Expression::Count { column, .. } => Value::from(
            count_selected(table, *column, &candidate.members, limits.source_rows).map_err(
                |error| {
                    Box::new(ExecutionError::Reduction {
                        path: assignment.path.clone(),
                        error,
                        identity: None,
                    })
                },
            )?,
        ),
        Expression::Reduce {
            identifier,
            column,
            reducer,
            text,
        } => match reduce_column(
            table,
            *column,
            &candidate.members,
            limits.source_rows,
            *reducer,
            text,
        ) {
            Ok(value) => Value::from(value),
            Err(TableReductionError::Reduction(
                yamaa_core::reduction::ReductionError::IncompatibleInputType { actual, .. },
            )) => {
                return Err(Box::new(ExecutionError::ReductionType {
                    path: assignment.path.clone(),
                    expression: text.clone(),
                    reducer: *reducer,
                    source: identifier
                        .clone()
                        .unwrap_or_else(|| table.schema().columns()[*column].name.clone()),
                    actual,
                }));
            }
            Err(error) => {
                let identity = if matches!(&error, TableReductionError::Reduction(yamaa_core::reduction::ReductionError::Arithmetic { error, .. }) if error.phase() == "derivation")
                {
                    failure_identity(candidate, plan.keys(), row, budget)?
                } else {
                    None
                };
                return Err(Box::new(ExecutionError::Reduction {
                    path: assignment.path.clone(),
                    error,
                    identity,
                }));
            }
        },
    };
    Ok(value)
}

/// Convert and account one completed result before publishing its column slot.
fn finish<E>(
    value: Value,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let converted = match convert(&value, plan.output().columns()[assignment.column].kind) {
        Ok(value) => value,
        Err(error) => {
            match conversion::recover(error, assignment, candidate, plan, row, budget, handlers) {
                Ok(value) => value,
                Err(failure) => {
                    if let Expression::SourceBinding {
                        variable_absent: true,
                        variable,
                        path,
                        ..
                    } = &assignment.expression
                    {
                        if let ExecutionError::Conversion {
                            error,
                            path: result_path,
                            identity,
                            ..
                        } = &*failure
                        {
                            // REQ-0364 retains the reached binding handler and completed-result context.
                            budget.scalar_text(
                                variable.len().saturating_add(path.len()).saturating_add(64),
                            )?;
                            let mut diagnostic = error.clone().into_diagnostic(result_path.clone());
                            diagnostic.spec_paths.push(alloc::format!("{path}.absent"));
                            diagnostic.context.insert(
                                "variable".into(),
                                yamaa_core::diagnostic::ContextValue::Scalar(Value::Str(
                                    variable.clone(),
                                )),
                            );
                            diagnostic.context.insert(
                                "handler".into(),
                                yamaa_core::diagnostic::ContextValue::Scalar(Value::Str(
                                    "absent".into(),
                                )),
                            );
                            return Err(Box::new(ExecutionError::SemanticDiagnostic {
                                diagnostic,
                                identity: identity.clone(),
                            }));
                        }
                    }
                    return Err(failure);
                }
            }
        }
    };
    budget.value(&converted)?;
    Ok(converted)
}

/// Application execution for an immutable core plan. Each call owns fresh run state.
/// The core model itself contains no ports, callbacks, counters or execution methods.
pub trait DatasetExecution {
    /// Execute only the admitted scope, returning no accepted table on any failure.
    /// Source access errors remain errors. No undeclared handlers, callbacks, joins,
    /// file publication or fallback are implicit.
    fn execute<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> Result<Execution, Box<ExecutionError<T::Error>>>;
    /// Retain ordered handler counts across successful values and every later failure.
    fn execute_observed<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error>;
    /// Execute immutable source snapshots in the same order as the admitted secondary schemas.
    fn execute_observed_sources<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        limits: Limits,
    ) -> ExecutionAttempt<T::Error>;
    /// Execute with explicitly activated, signature-matched callbacks on the caller's thread.
    /// All referenced signatures must be present before any table method is called,
    /// even for empty input or filtered-out rows. No callback is retried or rolled back.
    fn execute_observed_functions<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error>;
    /// Observe stage transitions without repeating evaluation or exposing intermediate output.
    /// The trusted observer owns its clock and must not mutate sources or callback state.
    /// Finished follows every returned success/failure, but is not guaranteed after a panic.
    fn execute_with_phase_observer<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
        observer: &mut dyn FnMut(ExecutionPhase),
    ) -> ExecutionAttempt<T::Error>;
}
impl DatasetExecution for DatasetPlan {
    fn execute<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        Executor { plan: self }.execute(table, limits)
    }
    fn execute_observed<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        Executor { plan: self }.execute_observed(table, limits)
    }
    fn execute_observed_sources<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        Executor { plan: self }.execute_observed_sources(table, secondary, limits)
    }
    fn execute_observed_functions<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        Executor { plan: self }.execute_observed_functions(table, secondary, functions, limits)
    }
    fn execute_with_phase_observer<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
        observer: &mut dyn FnMut(ExecutionPhase),
    ) -> ExecutionAttempt<T::Error> {
        Executor { plan: self }
            .execute_with_phase_observer(table, secondary, functions, limits, observer)
    }
}
// A service borrows the one admitted model; it owns no second plan representation.
struct Executor<'a> {
    plan: &'a DatasetPlan,
}
impl core::ops::Deref for Executor<'_> {
    type Target = DatasetPlan;
    fn deref(&self) -> &Self::Target {
        self.plan
    }
}
impl Executor<'_> {
    /// Execute only the admitted scope, returning no accepted table on any failure.
    /// Source access errors remain errors. No undeclared handlers, callbacks, joins,
    /// file publication or fallback are implicit.
    pub fn execute<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        self.execute_observed(table, limits).result
    }

    /// Retain ordered handler counts across successful values and every later failure.
    pub fn execute_observed<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        self.execute_observed_sources(table, &[], limits)
    }

    /// Execute immutable source snapshots in the same order as the admitted secondary schemas.
    pub fn execute_observed_sources<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        self.execute_observed_functions(
            table,
            secondary,
            &mut crate::function_invocation::NoFunctions::new(),
            limits,
        )
    }

    /// Execute with explicitly activated, signature-matched callbacks on the caller's thread.
    /// All referenced signatures must be present before any table method is called,
    /// even for empty input or filtered-out rows. No callback is retried or rolled back.
    pub fn execute_observed_functions<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        self.execute_with_phase_observer(table, secondary, functions, limits, &mut |_| {})
    }

    /// Observe stage transitions without repeating evaluation or exposing intermediate output.
    /// The trusted observer owns its clock and must not mutate sources or callback state.
    /// Finished follows every returned success/failure, but is not guaranteed after a panic.
    pub fn execute_with_phase_observer<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
        observer: &mut dyn FnMut(ExecutionPhase),
    ) -> ExecutionAttempt<T::Error> {
        observer(ExecutionPhase::Admission);
        let mut observations = AttemptObservations::default();
        for declaration in self.plan.conversion_handlers() {
            observations
                .handlers
                .register(&declaration.handler.spec_path, HandlerKind::Unconvertible);
        }
        for assignment in self
            .plan
            .templates()
            .iter()
            .flat_map(|template| &template.assignments)
            .chain(self.plan.columns())
        {
            scalar::register(assignment, &mut observations.handlers);
            imputation::register(&assignment.expression, &mut observations.handlers);
            str_case::register(&assignment.expression, &mut observations.handlers);
            str_extract::register(&assignment.expression, &mut observations.handlers);
            str_template::register(&assignment.expression, &mut observations.handlers);
        }
        let result = self.execute_inner(
            table,
            secondary,
            functions,
            limits,
            &mut observations,
            observer,
        );
        let attempt = ExecutionAttempt {
            result,
            handler_counts: observations.handlers.snapshot().to_vec(),
            retained_verifications: observations.verifications,
        };
        observer(ExecutionPhase::Finished);
        attempt
    }

    /// Share one handler ledger and resource budget for this immutable plan attempt.
    fn execute_inner<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        limits: Limits,
        observations: &mut AttemptObservations,
        observer: &mut dyn FnMut(ExecutionPhase),
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        let AttemptObservations {
            handlers,
            verifications: records,
        } = observations;
        functions::admit_plan(self.plan, functions)?;
        if table.schema() != self.plan.source()
            || secondary.len() != self.plan.secondary().len()
            || secondary
                .iter()
                .zip(self.plan.secondary())
                .any(|(table, expected)| table.schema() != &expected.schema)
        {
            return Err(Box::new(ExecutionError::SchemaMismatch));
        }
        let rows = secondary
            .iter()
            .try_fold(table.row_count(), |total, source| {
                total.checked_add(source.row_count())
            });
        if rows.is_none_or(|rows| rows > limits.source_rows) {
            return Err(Box::new(ExecutionError::Capacity));
        }
        observer(ExecutionPhase::Derivation);
        let primary = row_drivers::Driver::Primary(table);
        let mut budget = Budget::new(limits);
        let mut intermediate_run = intermediates::Run::default();
        let mut candidates: Vec<Candidate> = Vec::new();
        intermediates::verify(
            self,
            false,
            limits,
            &mut EvaluationState {
                primary: &primary,
                functions,
                secondary,
                self_rows: &[],
                budget: &mut budget,
                handlers,
                intermediates: &mut intermediate_run,
                output_rows: &[],
                output_offset: 0,
                windows: &mut windows::Cache::default(),
                aggregate_row: None,
            },
        )?;
        let mut next_selection_identity = 0usize;
        if matches!(self.plan.templates()[0].mode, RowMode::Keys) {
            candidates = key_grain::construct(self, table, limits, &mut budget, handlers)?;
        } else {
            for (template_index, template) in self.plan.templates().iter().enumerate() {
                let named = if let RowMode::IntermediateDriver { index, .. } = &template.mode {
                    Some(intermediates::driver(
                        *index,
                        self,
                        limits,
                        &mut EvaluationState {
                            primary: &primary,
                            functions,
                            secondary,
                            self_rows: &candidates,
                            budget: &mut budget,
                            handlers,
                            intermediates: &mut intermediate_run,
                            output_rows: &[],
                            output_offset: 0,
                            windows: &mut windows::Cache::default(),
                            aggregate_row: None,
                        },
                    )?)
                } else {
                    None
                };
                let data = named
                    .as_ref()
                    .and_then(|relation| relation.materialized.as_ref())
                    .map(|data| intermediates::Data::<T::Error>::new(data));
                let stored =
                    row_drivers::Driver::new(table, secondary, template.mode.source_index());
                let table = if let Some(data) = &data {
                    row_drivers::Driver::Secondary(data)
                } else {
                    stored
                };
                let table = &table;
                let first = candidates.len();
                let deferred = yamaa_core::dataset::window_stages(
                    &template.assignments,
                    self.plan.output().columns().len(),
                    self.plan.intermediates(),
                );
                let groups = match template.mode.local_mode() {
                    RowMode::Driver { .. } | RowMode::IntermediateDriver { .. } => {
                        unreachable!("plan admission rejects nested drivers")
                    }
                    RowMode::Keys => unreachable!("key mode is admitted only as the sole template"),
                    RowMode::Records => {
                        // Admit the known cardinality before materializing record memberships.
                        let rows = named
                            .as_ref()
                            .map_or(table.row_count(), |relation| relation.eligible.len());
                        self.check_capacity(candidates.len(), rows, limits)?;
                        if let Some(relation) = &named {
                            relation.eligible.iter().map(|&row| vec![row]).collect()
                        } else {
                            (0..table.row_count()).map(|row| vec![row]).collect()
                        }
                    }
                    RowMode::Groups(keys) => {
                        budget.work(
                            named
                                .as_ref()
                                .map_or(table.row_count(), |relation| relation.eligible.len()),
                            keys.len(),
                        )?;
                        if let Some(relation) = &named {
                            intermediates::driver_groups(table, &relation.eligible, keys, limits)?
                        } else {
                            partition(table, keys, limits.source_rows, limits.key_cells)
                                .map_err(|error| Box::new(ExecutionError::Grouping(error)))?
                        }
                    }
                };
                self.check_capacity(candidates.len(), groups.len(), limits)?;
                candidates
                    .try_reserve(groups.len())
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                for members in groups {
                    // Discarding a row never refunds its token or evaluated work.
                    if !self.plan.intermediates().is_empty() {
                        budget.work(1, 1)?;
                    }
                    let selection_identity = SelectionIdentity::Candidate(next_selection_identity);
                    next_selection_identity = next_selection_identity
                        .checked_add(1)
                        .ok_or_else(|| Box::new(ExecutionError::Capacity))?;
                    let mut candidate = Candidate {
                        template: Some(template_index),
                        selection_identity,
                        members,
                        values: vec![Value::Missing; self.plan.output().columns().len()],
                        completed: vec![false; self.plan.output().columns().len()],
                    };
                    for assignment in template
                        .assignments
                        .iter()
                        .filter(|assignment| !deferred[assignment.column])
                    {
                        candidate.values[assignment.column] = evaluate(
                            table,
                            assignment,
                            &candidate,
                            self,
                            candidates.len(),
                            limits,
                            &mut EvaluationState {
                                primary: &primary,
                                functions,
                                secondary,
                                self_rows: &candidates[..first],
                                budget: &mut budget,
                                handlers,
                                intermediates: &mut intermediate_run,
                                output_rows: &[],
                                output_offset: 0,
                                windows: &mut windows::Cache::default(),
                                aggregate_row: None,
                            },
                        )?;
                        candidate.completed[assignment.column] = true;
                    }
                    if let Some(filter) = template.filter.as_ref() {
                        let source_row = candidate.members[0];
                        let truth = if filter.bindings().iter().any(|binding| {
                            matches!(
                                binding.read,
                                yamaa_core::bound_expression::Read::Intermediate { .. }
                                    | yamaa_core::bound_expression::Read::Joined(_)
                            )
                        }) {
                            let assignment = Assignment {
                                column: 0,
                                path: filter.plan().spec_path().into(),
                                expression: Expression::Literal(Value::Missing),
                            };
                            case_predicates::evaluate(
                                filter,
                                table,
                                &candidate,
                                self,
                                candidates.len(),
                                &assignment,
                                &mut EvaluationState {
                                    primary: &primary,
                                    functions,
                                    secondary,
                                    self_rows: &candidates[..first],
                                    budget: &mut budget,
                                    handlers,
                                    intermediates: &mut intermediate_run,
                                    output_rows: &[],
                                    output_offset: 0,
                                    windows: &mut windows::Cache::default(),
                                    aggregate_row: None,
                                },
                            )?
                        } else {
                            crate::dataset_predicate::evaluate(
                                filter,
                                table,
                                source_row,
                                &candidate.values,
                                budget.predicate(),
                            )
                            .map_err(|error| match error.kind {
                                yamaa_core::predicate::ErrorKind::Limit(limit) => {
                                    Box::new(predicate_limit(limit))
                                }
                                _ => Box::new(ExecutionError::Predicate { source_row, error }),
                            })?
                        };
                        if truth != yamaa_core::predicate::Truth::True {
                            budget.discard_candidate(&candidate.values);
                            continue;
                        }
                    }
                    candidates.push(candidate);
                }
                let (prior, current) = candidates.split_at_mut(first);
                row_windows::complete(
                    table,
                    template,
                    &deferred,
                    current,
                    self,
                    first,
                    limits,
                    &mut EvaluationState {
                        primary: &primary,
                        functions,
                        secondary,
                        self_rows: prior,
                        budget: &mut budget,
                        handlers,
                        intermediates: &mut intermediate_run,
                        output_rows: &[],
                        output_offset: 0,
                        windows: &mut windows::Cache::default(),
                        aggregate_row: None,
                    },
                )?;
                intermediate_run.publish_self(self, &mut budget)?;
                intermediates::verify(
                    self,
                    true,
                    limits,
                    &mut EvaluationState {
                        primary: &primary,
                        functions,
                        secondary,
                        self_rows: &candidates,
                        budget: &mut budget,
                        handlers,
                        intermediates: &mut intermediate_run,
                        output_rows: &[],
                        output_offset: 0,
                        windows: &mut windows::Cache::default(),
                        aggregate_row: None,
                    },
                )?;
            }
        }
        let mut completed = vec![false; self.plan.output().columns().len()];
        for assignment in &self.plan.templates()[0].assignments {
            completed[assignment.column] = true;
        }
        let mut next_column = 0;
        self.check_columns(
            &candidates,
            &completed,
            &mut next_column,
            records,
            &mut budget,
        )?;
        for assignment in self.plan.columns() {
            let mut nested_windows = windows::Cache::default();
            let mut numbers = windows::Evaluation::new(
                assignment,
                &candidates,
                self,
                table,
                limits,
                &mut budget,
                handlers,
            )?;
            for row in 0..candidates.len() {
                let number = if let Some(run) = &mut numbers {
                    Some(run.value(row, &candidates, table, &mut budget, assignment, self)?)
                } else {
                    None
                };
                let candidate = &candidates[row];
                let value = if let Some(value) = number {
                    finish(
                        value,
                        assignment,
                        candidate,
                        self,
                        row,
                        &mut budget,
                        handlers,
                    )?
                } else {
                    evaluate(
                        table,
                        assignment,
                        candidate,
                        self,
                        row,
                        limits,
                        &mut EvaluationState {
                            primary: &primary,
                            functions,
                            secondary,
                            self_rows: &candidates,
                            budget: &mut budget,
                            handlers,
                            intermediates: &mut intermediate_run,
                            output_rows: &candidates,
                            aggregate_row: None,
                            output_offset: 0,
                            windows: &mut nested_windows,
                        },
                    )?
                };
                let candidate = &mut candidates[row];
                candidate.values[assignment.column] = value;
                candidate.completed[assignment.column] = true;
            }
            completed[assignment.column] = true;
            self.check_columns(
                &candidates,
                &completed,
                &mut next_column,
                records,
                &mut budget,
            )?;
        }
        observer(ExecutionPhase::OutputKeys);
        let mut dataset = Dataset {
            schema: self.plan.output().clone(),
            rows: candidates.into_iter().map(|row| row.values).collect(),
        };
        let mut failures = Vec::new();
        budget.work(dataset.rows.len(), self.plan.keys().len())?;
        for (position, &column) in self.plan.keys().iter().enumerate() {
            let missing: Vec<_> = dataset
                .rows
                .iter()
                .enumerate()
                .filter_map(|(row, values)| matches!(values[column], Value::Missing).then_some(row))
                .collect();
            if !missing.is_empty() {
                failures.push(CheckRecord {
                    path: alloc::format!("keys[{position}]"),
                    code: yamaa_core::diagnostic::ConditionCode::OutputMissingKeyFailed,
                    evaluated_count: dataset.rows.len(),
                    failed_count: missing.len(),
                    output_rows: dataset.rows.len(),
                    codelist: None,
                    inventory: None,

                    row_count: None,
                    offending_rows: identities(&dataset, self.plan.keys(), missing, &mut budget)?,
                });
            }
        }
        budget.work(dataset.rows.len(), self.plan.keys().len())?;
        let groups = partition(
            &dataset,
            self.plan.keys(),
            limits.output_rows,
            limits.key_cells,
        )
        .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
        let duplicates: Vec<_> = groups
            .iter()
            .filter(|members| members.len() > 1)
            .map(|members| members[0])
            .collect();
        if !duplicates.is_empty() {
            failures.push(CheckRecord {
                path: "keys".into(),
                code: yamaa_core::diagnostic::ConditionCode::OutputDuplicateKeyFailed,
                evaluated_count: groups.len(),
                failed_count: duplicates.len(),
                output_rows: dataset.rows.len(),
                codelist: None,
                inventory: None,

                row_count: None,
                offending_rows: identities(&dataset, self.plan.keys(), duplicates, &mut budget)?,
            });
        }
        if !failures.is_empty() {
            return Err(Box::new(ExecutionError::KeyFailures(failures)));
        }
        observer(ExecutionPhase::Verification);
        for verification in self.plan.verifications() {
            let record = match &verification.check {
                Check::NotMissing
                | Check::AllowedValues(_)
                | Check::Codelist { .. }
                | Check::ValueCodelists { .. }
                | Check::Inventory { .. }
                | Check::ValueMaxLength { .. }
                | Check::Range { .. }
                | Check::MaxLength(_)
                | Check::WideMaxLength(_)
                | Check::Matches(_) => unreachable!("column-only check is rejected at admission"),
                Check::InvalidDiagnostic(diagnostic) => {
                    return Err(Box::new(ExecutionError::VerificationDiagnostic {
                        diagnostic: diagnostic.clone(),
                        records: core::mem::take(records),
                    }));
                }
                Check::InvalidDeclaration { code, reason } => {
                    return Err(Box::new(ExecutionError::VerificationDeclaration {
                        path: verification.path.clone(),
                        code: *code,
                        reason: reason.clone(),
                        records: core::mem::take(records),
                    }));
                }
                Check::Assert { .. } | Check::PredicateDeclaration(_) => {
                    let Some(record) = crate::dataset_verification::predicate_check(
                        verification,
                        &dataset,
                        self.plan.keys(),
                        &mut budget,
                        records,
                    )?
                    else {
                        continue;
                    };
                    record
                }
                Check::Unique(columns) => {
                    // Repeated references do not change tuple equality. Keep first
                    // declaration order while using the strict grouping primitive.
                    let mut distinct = Vec::new();
                    for &column in columns {
                        if !distinct.contains(&column) {
                            distinct.push(column);
                        }
                    }
                    budget.work(dataset.rows.len(), distinct.len())?;
                    let groups =
                        partition(&dataset, &distinct, limits.output_rows, limits.key_cells)
                            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
                    let repeated: Vec<_> =
                        groups.iter().filter(|members| members.len() > 1).collect();
                    CheckRecord {
                        path: verification.path.clone(),
                        code: yamaa_core::diagnostic::ConditionCode::VerificationUniqueFailed,
                        evaluated_count: groups.len(),
                        failed_count: repeated.len(),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        inventory: None,

                        row_count: None,
                        offending_rows: identities(
                            &dataset,
                            self.plan.keys(),
                            repeated
                                .into_iter()
                                .flat_map(|members| members.iter().copied()),
                            &mut budget,
                        )?,
                    }
                }
                Check::AllOrNone(columns) => {
                    budget.work(dataset.rows.len(), columns.len())?;
                    let offending = dataset.rows.iter().enumerate().filter_map(|(index, row)| {
                        let missing = matches!(row[columns[0]], Value::Missing);
                        columns
                            .iter()
                            .any(|&column| matches!(row[column], Value::Missing) != missing)
                            .then_some(index)
                    });
                    let offending_rows =
                        identities(&dataset, self.plan.keys(), offending, &mut budget)?;
                    CheckRecord {
                        path: verification.path.clone(),
                        code: yamaa_core::diagnostic::ConditionCode::VerificationAllOrNoneFailed,
                        evaluated_count: dataset.rows.len(),
                        failed_count: offending_rows.len(),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        inventory: None,

                        row_count: None,
                        offending_rows,
                    }
                }
                Check::ScopedRowCount(check) => crate::dataset_verification::row_count_check(
                    verification,
                    check,
                    &dataset,
                    limits,
                    &mut budget,
                    records,
                )?,
                Check::RowCount { .. } | Check::WideRowCount { .. } => {
                    budget.work(1, verification.check.row_count_work())?;
                    let failed = !verification
                        .check
                        .row_count_accepts(dataset.rows.len())
                        .unwrap_or(false);
                    CheckRecord {
                        path: verification.path.clone(),
                        code: yamaa_core::diagnostic::ConditionCode::VerificationRowCountFailed,
                        evaluated_count: 1,
                        failed_count: usize::from(failed),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        inventory: None,

                        row_count: None,
                        offending_rows: Vec::new(),
                    }
                }
            };
            records.push(record);
        }
        if records.iter().any(|record| {
            record.failed_count != 0
                && self.plan.verification_severity(&record.path)
                    == yamaa_core::dataset::VerificationSeverity::Error
        }) {
            return Err(Box::new(ExecutionError::VerificationFailures(
                core::mem::take(records),
            )));
        }
        ordering::apply(self.plan.output_order(), &mut dataset, &mut budget)?;
        Ok(Execution {
            dataset,
            verifications: core::mem::take(records),
        })
    }

    /// Check only the available declared prefix, after every key is complete.
    fn check_columns<E>(
        &self,
        candidates: &[Candidate],
        completed: &[bool],
        next: &mut usize,
        records: &mut Vec<CheckRecord>,
        budget: &mut Budget,
    ) -> Result<(), Box<ExecutionError<E>>> {
        if self.plan.keys().iter().any(|&column| !completed[column]) {
            return Ok(());
        }
        while *next < completed.len() && completed[*next] {
            if let Some(group) = self
                .plan
                .column_verifications()
                .iter()
                .find(|group| group.column == *next)
            {
                // A value-level binding reads the final discriminator. A later
                // discriminator delays the declared check prefix until its value
                // is complete; it never silently selects from a partial row.
                if group.checks.iter().any(|verification| {
                    matches!(&verification.check, Check::ValueCodelists { discriminator, .. }
                        if !completed[*discriminator]) || matches!(&verification.check, Check::Inventory { scope: Some(scope), .. } if !completed[scope.discriminator]) || matches!(&verification.check, Check::ValueMaxLength { discriminator, .. } if !completed[*discriminator])
                }) {
                    return Ok(());
                }
                let start = records.len();
                for verification in &group.checks {
                    match &verification.check {
                        Check::InvalidDiagnostic(diagnostic) => {
                            return Err(Box::new(ExecutionError::VerificationDiagnostic {
                                diagnostic: diagnostic.clone(),
                                records: core::mem::take(records),
                            }));
                        }
                        Check::InvalidDeclaration { code, reason } => {
                            return Err(Box::new(ExecutionError::VerificationDeclaration {
                                path: verification.path.clone(),
                                code: *code,
                                reason: reason.clone(),
                                records: core::mem::take(records),
                            }));
                        }
                        Check::NotMissing
                        | Check::AllowedValues(_)
                        | Check::Codelist { .. }
                        | Check::ValueCodelists { .. }
                        | Check::Inventory { .. }
                        | Check::ValueMaxLength { .. }
                        | Check::Range { .. }
                        | Check::MaxLength(_)
                        | Check::WideMaxLength(_)
                        | Check::Matches(_) => {
                            let bindings = match &verification.check {
                                Check::ValueCodelists { bindings, .. } => {
                                    bindings.iter().map(Some).collect::<Vec<_>>()
                                }
                                _ => vec![None],
                            };
                            for binding in bindings {
                                if let (Check::ValueCodelists { overrides, .. }, Some(binding)) =
                                    (&verification.check, binding)
                                {
                                    let width = overrides
                                        .iter()
                                        .chain(&binding.codes)
                                        .try_fold(1usize, |width, code| {
                                            width.checked_add(code.len().max(1))
                                        })
                                        .ok_or_else(|| {
                                            Box::new(ExecutionError::Limit {
                                                resource: Resource::WorkCells,
                                                limit: budget.work_cell_limit(),
                                                required: None,
                                            })
                                        })?;
                                    budget.work(candidates.len(), width)?;
                                }
                                if let Check::Inventory {
                                    scope: Some(scope), ..
                                } = &verification.check
                                {
                                    let width = scope
                                        .overrides
                                        .iter()
                                        .try_fold(1usize, |width, (code, _)| {
                                            width.checked_add(code.len().max(1))
                                        })
                                        .ok_or_else(|| {
                                            Box::new(ExecutionError::Limit {
                                                resource: Resource::WorkCells,
                                                limit: budget.work_cell_limit(),
                                                required: None,
                                            })
                                        })?;
                                    budget.work(candidates.len(), width)?;
                                }
                                if let Check::ValueMaxLength { codes, .. } = &verification.check {
                                    let width = codes
                                        .iter()
                                        .try_fold(1usize, |width, code| {
                                            width.checked_add(code.len().max(1))
                                        })
                                        .ok_or_else(|| {
                                            Box::new(ExecutionError::Limit {
                                                resource: Resource::WorkCells,
                                                limit: budget.work_cell_limit(),
                                                required: None,
                                            })
                                        })?;
                                    budget.work(candidates.len(), width)?;
                                }
                                let positions = candidates
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(index, candidate)| {
                                        let selected = match (&verification.check, binding) {
                                            (
                                                Check::ValueCodelists {
                                                    discriminator,
                                                    overrides,
                                                    ..
                                                },
                                                Some(binding),
                                            ) => {
                                                let code = match &candidate.values[*discriminator] {
                                                    Value::Str(code) => Some(code.as_str()),
                                                    _ => None,
                                                };
                                                code.is_some_and(|code| {
                                                    binding.codes.iter().any(|c| c == code)
                                                }) || binding.default
                                                    && !code.is_some_and(|code| {
                                                        overrides.iter().any(|c| c == code)
                                                    })
                                            }
                                            (
                                                Check::Inventory {
                                                    scope: Some(scope), ..
                                                },
                                                _,
                                            ) => match &candidate.values[scope.discriminator] {
                                                Value::Str(code) => scope
                                                    .overrides
                                                    .iter()
                                                    .find(|(value, _)| value == code)
                                                    .map_or(scope.default, |(_, enabled)| *enabled),
                                                _ => scope.default,
                                            },
                                            (Check::ValueMaxLength { discriminator, codes, include, .. }, _) => {
                                                let matches = matches!(&candidate.values[*discriminator], Value::Str(code) if codes.contains(code));
                                                matches == *include
                                            },
                                            _ => true,
                                        };
                                        selected.then_some(index)
                                    })
                                    .collect::<Vec<_>>();
                                let width = if let Some(binding) = binding {
                                    binding.values.len().max(1)
                                } else {
                                    match &verification.check {
                                        Check::AllowedValues(accepted)
                                        | Check::Inventory {
                                            values: accepted, ..
                                        } => accepted.len().max(1),
                                        Check::Codelist { values, .. } => values.len().max(1),
                                        Check::Range { .. } => 2,
                                        Check::WideMaxLength(bound)
                                        | Check::ValueMaxLength { max: bound, .. } => {
                                            bound.decimal().len().min(usize::BITS as usize).max(1)
                                        }
                                        _ => 1,
                                    }
                                };
                                budget.work(candidates.len(), width)?;
                                if matches!(
                                    verification.check,
                                    Check::MaxLength(_)
                                        | Check::WideMaxLength(_)
                                        | Check::ValueMaxLength { .. }
                                ) {
                                    for row in candidates {
                                        if let Value::Str(text) = &row.values[group.column] {
                                            budget.scalar_text(text.len())?;
                                        }
                                    }
                                }
                                let values = positions
                                    .iter()
                                    .map(|&position| &candidates[position].values[group.column]);
                                let offending = if let Some(binding) = binding {
                                    values
                                        .enumerate()
                                        .filter_map(|(position, value)| {
                                            (!yamaa_core::dataset_checks::codelist_accepts(
                                                &binding.values,
                                                value,
                                            ))
                                            .then_some(position)
                                        })
                                        .collect()
                                } else if let Check::Matches(pattern) = &verification.check {
                                    yamaa_core::dataset_checks::matches_offenders(
                                        pattern,
                                        values,
                                        Default::default(),
                                        budget.predicate(),
                                    )
                                    .map_err(|error| Box::new(predicate_limit(error)))?
                                } else {
                                    yamaa_core::dataset_checks::column_offenders(
                                        &verification.check,
                                        values,
                                    )
                                };
                                let offending = offending
                                    .into_iter()
                                    .map(|index| positions[index])
                                    .collect::<Vec<_>>();
                                let code =
                                    yamaa_core::dataset_checks::column_code(&verification.check)
                                        .expect("admitted column check");
                                let identifier = binding.map(|binding| binding.id.as_str()).or({
                                    if let Check::Codelist { id, .. } = &verification.check {
                                        Some(id.as_str())
                                    } else {
                                        None
                                    }
                                });
                                let codelist = if let Some(id) = identifier {
                                    budget.scalar_text(id.len())?;
                                    budget.identity(offending.iter().map(|&position| {
                                        &candidates[position].values[group.column]
                                    }))?;
                                    Some(CodelistObservation {
                                        id: id.into(),
                                        values: offending
                                            .iter()
                                            .map(|&position| {
                                                candidates[position].values[group.column].clone()
                                            })
                                            .collect(),
                                    })
                                } else {
                                    None
                                };
                                let inventory =
                                    if matches!(verification.check, Check::Inventory { .. }) {
                                        budget.identity(offending.iter().map(|&position| {
                                            &candidates[position].values[group.column]
                                        }))?;
                                        Some(
                                            offending
                                                .iter()
                                                .map(|&position| {
                                                    candidates[position].values[group.column]
                                                        .clone()
                                                })
                                                .collect(),
                                        )
                                    } else {
                                        None
                                    };
                                let mut offending_rows = Vec::new();
                                for position in offending {
                                    offending_rows.push(
                                        failure_identity(
                                            &candidates[position],
                                            self.plan.keys(),
                                            position,
                                            budget,
                                        )?
                                        .expect("all keys are complete before column checks"),
                                    );
                                }
                                records.push(CheckRecord {
                                    path: verification.path.clone(),
                                    code,
                                    evaluated_count: positions.len(),
                                    failed_count: offending_rows.len(),
                                    output_rows: candidates.len(),
                                    codelist,
                                    inventory,
                                    row_count: None,
                                    offending_rows,
                                });
                            }
                        }
                        _ => unreachable!("column check vocabulary is admitted in core"),
                    }
                }
                if records[start..].iter().any(|record| {
                    record.failed_count != 0
                        && self.plan.verification_severity(&record.path)
                            == yamaa_core::dataset::VerificationSeverity::Error
                }) {
                    return Err(Box::new(ExecutionError::VerificationFailures(
                        core::mem::take(records),
                    )));
                }
            }
            *next += 1;
        }
        Ok(())
    }

    /// Reject cardinality overflow and excessive output slots before deriving rows.
    fn check_capacity<E>(
        &self,
        current: usize,
        additional: usize,
        limits: Limits,
    ) -> Result<(), Box<ExecutionError<E>>> {
        let required = current
            .checked_add(additional)
            .ok_or_else(|| Box::new(ExecutionError::Capacity))?;
        if required > limits.output_rows
            || required
                .checked_mul(self.plan.output().columns().len())
                .is_none_or(|cells| cells > limits.output_cells)
        {
            return Err(Box::new(ExecutionError::Capacity));
        }
        Ok(())
    }
}

/// Retain original output keys without substituting projected or row-number identity.
pub(crate) fn identities<E>(
    dataset: &Dataset,
    keys: &[usize],
    rows: impl IntoIterator<Item = usize>,
    budget: &mut Budget,
) -> Result<Vec<RowIdentity>, Box<ExecutionError<E>>> {
    let mut identities = Vec::new();
    for position in rows {
        budget.work(1, keys.len())?;
        budget.identity(keys.iter().map(|&column| &dataset.rows[position][column]))?;
        identities.push(RowIdentity {
            position,
            values: keys
                .iter()
                .map(|&column| dataset.rows[position][column].clone())
                .collect(),
        });
    }
    Ok(identities)
}

/// Retain known current keys only when the owning semantic phase requests them.
fn retained_diagnostic<E>(
    diagnostic: yamaa_core::diagnostic::Diagnostic,
    retain_keys: bool,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    budget: &mut Budget,
) -> Box<ExecutionError<E>> {
    let identity = if retain_keys {
        match failure_identity(candidate, plan.keys(), row, budget) {
            Ok(identity) => identity,
            Err(error) => return error,
        }
    } else {
        None
    };
    Box::new(ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    })
}

/// A partial key is unavailable, whereas a completed missing key is a known value.
fn failure_identity<E>(
    candidate: &Candidate,
    keys: &[usize],
    position: usize,
    budget: &mut Budget,
) -> Result<Option<RowIdentity>, Box<ExecutionError<E>>> {
    if keys.iter().any(|&key| !candidate.completed[key]) {
        return Ok(None);
    }
    budget.work(1, keys.len())?;
    budget.identity(keys.iter().map(|&column| &candidate.values[column]))?;
    Ok(Some(RowIdentity {
        position,
        values: keys
            .iter()
            .map(|&column| candidate.values[column].clone())
            .collect(),
    }))
}
