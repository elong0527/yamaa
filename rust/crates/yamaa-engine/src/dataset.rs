//! Closed typed dataset plan for bounded row construction and column derivation.
//!
//! This is an internal bridge target, not a specification parser or public backend.
//! Unsupported syntax must be rejected by a compiler before creating this plan.

#[path = "dataset_conversion.rs"]
mod conversion;
#[path = "dataset_intermediates.rs"]
mod intermediates;
pub use yamaa_core::dataset::*;
#[path = "dataset_functions.rs"]
mod functions;
pub use functions::FunctionBindings;
pub(crate) fn unavailable_functions<E>() -> impl FunctionBindings<Error = E> {
    functions::UnavailableFunctions(core::marker::PhantomData)
}
#[path = "dataset_keys.rs"]
mod key_grain;
#[path = "dataset_lookup.rs"]
mod lookup;
#[path = "dataset_numeric.rs"]
mod numeric;
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
pub struct CheckRecord {
    pub path: String,
    pub condition: &'static str,
    pub requirement: &'static str,
    pub evaluated_count: usize,
    pub failed_count: usize,
    pub output_rows: usize,
    pub offending_rows: Vec<RowIdentity>,
    /// Full offending values survive discarded output, under identity quotas.
    pub codelist: Option<CodelistObservation>,
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
        condition: &'static str,
        requirement: &'static str,
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

/// Keep source provenance per candidate for the later whole-column phase.
struct Candidate {
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
    functions: &'a mut dyn FunctionBindings<Error = E>,
    secondary: &'a [&'a dyn TableAccess<Error = E>],
    budget: &'a mut Budget,
    handlers: &'a mut HandlerCounter,
    intermediates: &'a mut intermediates::Run,
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
    if let Expression::Intermediate { index, column } = assignment.expression {
        state.budget.work(1, 1)?;
        let value = intermediates::read(index, column, assignment, candidate, plan, row, state)?;
        return finish(
            value,
            assignment,
            candidate,
            plan,
            row,
            state.budget,
            state.handlers,
        );
    }
    let EvaluationState {
        budget,
        handlers,
        secondary,
        functions,
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
        Expression::Function(function) => functions::evaluate(
            function,
            table,
            candidate,
            functions::Context {
                plan,
                row,
                assignment,
            },
            budget,
            handlers,
            *functions,
        )?,
        Expression::ProjectFunction(function) => functions::evaluate_project(
            function,
            table,
            candidate,
            functions::Context {
                plan,
                row,
                assignment,
            },
            budget,
            handlers,
            *functions,
        )?,
        Expression::Window(_) => unreachable!("window assignments execute by whole column"),
        Expression::Intermediate { .. } => unreachable!("intermediates execute through run state"),
        Expression::Literal(value) => {
            if let Value::Str(text) = value {
                budget.scalar_text(text.len())?;
            }
            value.clone()
        }
        Expression::Compute(expression) => {
            numeric::evaluate(expression, table, candidate, plan, row, budget)?
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
        Expression::FirstAvailable(selection) => {
            let mut selected = Value::Missing;
            for operand in selection.sources() {
                budget.work(1, 1)?;
                let value = match &operand.read {
                    yamaa_core::dataset::SelectionRead::Column(column) => {
                        let value = &candidate.values[*column];
                        if let Value::Str(text) = value {
                            budget.scalar_text(text.len())?;
                        }
                        value.clone()
                    }
                    yamaa_core::dataset::SelectionRead::Collect {
                        column,
                        identifier,
                        filter,
                    } => {
                        budget.work(candidate.members.len(), 1)?;
                        key_grain::collect_bound(
                            table,
                            key_grain::Collection {
                                column: *column,
                                identifier,
                                filter: filter.as_deref(),
                                selection: None,
                            },
                            key_grain::CollectionContext {
                                path: &assignment.path,
                                candidate,
                                plan,
                                row,
                            },
                            budget,
                            handlers,
                        )?
                    }
                };
                if value != Value::Missing {
                    selected = value;
                    break;
                }
            }
            if selected == Value::Missing {
                if let Value::Str(text) = selection.missing() {
                    budget.scalar_text(text.len())?;
                }
                selection.missing().clone()
            } else {
                selected
            }
        }
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
    finish(value, assignment, candidate, plan, row, budget, handlers)
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
            conversion::recover(error, assignment, candidate, plan, row, budget, handlers)?
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
        functions: &mut dyn FunctionBindings<Error = T::Error>,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error>;
    /// Observe stage transitions without repeating evaluation or exposing intermediate output.
    /// The trusted observer owns its clock and must not mutate sources or callback state.
    /// Finished follows every returned success/failure, but is not guaranteed after a panic.
    fn execute_with_phase_observer<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionBindings<Error = T::Error>,
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
        functions: &mut dyn FunctionBindings<Error = T::Error>,
        limits: Limits,
    ) -> ExecutionAttempt<T::Error> {
        Executor { plan: self }.execute_observed_functions(table, secondary, functions, limits)
    }
    fn execute_with_phase_observer<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionBindings<Error = T::Error>,
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
            &mut functions::UnavailableFunctions(core::marker::PhantomData),
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
        functions: &mut dyn FunctionBindings<Error = T::Error>,
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
        functions: &mut dyn FunctionBindings<Error = T::Error>,
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
        functions: &mut dyn FunctionBindings<Error = T::Error>,
        limits: Limits,
        observations: &mut AttemptObservations,
        observer: &mut dyn FnMut(ExecutionPhase),
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        let AttemptObservations {
            handlers,
            verifications: records,
        } = observations;
        for assignment in self
            .plan
            .templates()
            .iter()
            .flat_map(|template| &template.assignments)
            .chain(self.plan.columns())
        {
            if let Expression::Function(function) = &assignment.expression {
                if functions.signature(function.slot()) != Some(function.signature()) {
                    return Err(Box::new(ExecutionError::FunctionBinding {
                        slot: function.slot(),
                    }));
                }
            }
            if let Expression::ProjectFunction(function) = &assignment.expression {
                if functions.project_signature(function.slot()) != Some(function.signature()) {
                    return Err(Box::new(ExecutionError::FunctionBinding {
                        slot: function.slot(),
                    }));
                }
            }
        }
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
        let mut budget = Budget::new(limits);
        let mut intermediate_run = intermediates::Run::default();
        let mut candidates: Vec<Candidate> = Vec::new();
        if matches!(self.plan.templates()[0].mode, RowMode::Keys) {
            candidates = key_grain::construct(self, table, limits, &mut budget, handlers)?;
        } else {
            for template in self.plan.templates() {
                let groups = match &template.mode {
                    RowMode::Keys => unreachable!("key mode is admitted only as the sole template"),
                    RowMode::Records => {
                        // Admit the known cardinality before materializing record memberships.
                        self.check_capacity(candidates.len(), table.row_count(), limits)?;
                        (0..table.row_count()).map(|row| vec![row]).collect()
                    }
                    RowMode::Groups(keys) => {
                        budget.work(table.row_count(), keys.len())?;
                        partition(table, keys, limits.source_rows, limits.key_cells)
                            .map_err(|error| Box::new(ExecutionError::Grouping(error)))?
                    }
                };
                self.check_capacity(candidates.len(), groups.len(), limits)?;
                candidates
                    .try_reserve(groups.len())
                    .map_err(|_| Box::new(ExecutionError::Allocation))?;
                for members in groups {
                    let mut candidate = Candidate {
                        members,
                        values: vec![Value::Missing; self.plan.output().columns().len()],
                        completed: vec![false; self.plan.output().columns().len()],
                    };
                    for assignment in &template.assignments {
                        candidate.values[assignment.column] = evaluate(
                            table,
                            assignment,
                            &candidate,
                            self,
                            candidates.len(),
                            limits,
                            &mut EvaluationState {
                                functions,
                                secondary,
                                budget: &mut budget,
                                handlers,
                                intermediates: &mut intermediate_run,
                            },
                        )?;
                        candidate.completed[assignment.column] = true;
                    }
                    if let Some(filter) = &template.filter {
                        let source_row = candidate.members[0];
                        let truth = crate::dataset_predicate::evaluate(
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
                        })?;
                        if truth != yamaa_core::predicate::Truth::True {
                            budget.discard_candidate(&candidate.values);
                            continue;
                        }
                    }
                    candidates.push(candidate);
                }
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
            let mut numbers = if let Expression::Window(window) = &assignment.expression {
                Some(windows::Run::new(
                    window,
                    &candidates,
                    self,
                    limits,
                    &mut budget,
                )?)
            } else {
                None
            };
            for row in 0..candidates.len() {
                let number = if let (Some(run), Expression::Window(window)) =
                    (&mut numbers, &assignment.expression)
                {
                    Some(run.value(
                        row,
                        window,
                        &candidates,
                        table,
                        &mut budget,
                        &windows::Context {
                            assignment,
                            plan: self,
                        },
                    )?)
                } else {
                    None
                };
                let candidate = &mut candidates[row];
                candidate.values[assignment.column] = if let Some(value) = number {
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
                            functions,
                            secondary,
                            budget: &mut budget,
                            handlers,
                            intermediates: &mut intermediate_run,
                        },
                    )?
                };
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
        let dataset = Dataset {
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
                    condition: "missing_key",
                    requirement: "REQ-0240",
                    evaluated_count: dataset.rows.len(),
                    failed_count: missing.len(),
                    output_rows: dataset.rows.len(),
                    codelist: None,
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
                condition: "duplicate_key",
                requirement: "REQ-0240",
                evaluated_count: groups.len(),
                failed_count: duplicates.len(),
                output_rows: dataset.rows.len(),
                codelist: None,
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
                | Check::Range { .. }
                | Check::MaxLength(_)
                | Check::Matches(_) => unreachable!("column-only check is rejected at admission"),
                Check::InvalidDiagnostic(diagnostic) => {
                    return Err(Box::new(ExecutionError::VerificationDiagnostic {
                        diagnostic: diagnostic.clone(),
                        records: core::mem::take(records),
                    }));
                }
                Check::InvalidDeclaration {
                    condition,
                    requirement,
                    reason,
                } => {
                    return Err(Box::new(ExecutionError::VerificationDeclaration {
                        path: verification.path.clone(),
                        condition,
                        requirement,
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
                        condition: "unique_failed",
                        requirement: "REQ-0381",
                        evaluated_count: groups.len(),
                        failed_count: repeated.len(),
                        output_rows: dataset.rows.len(),
                        codelist: None,
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
                        condition: "all_or_none_failed",
                        requirement: "REQ-0382",
                        evaluated_count: dataset.rows.len(),
                        failed_count: offending_rows.len(),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        offending_rows,
                    }
                }
                Check::RowCount { min, max } => {
                    budget.work(1, 1)?;
                    let count = dataset.rows.len() as i128;
                    let failed = min.is_some_and(|min| count < i128::from(min))
                        || max.is_some_and(|max| count > i128::from(max));
                    CheckRecord {
                        path: verification.path.clone(),
                        condition: "row_count_failed",
                        requirement: "REQ-0385",
                        evaluated_count: 1,
                        failed_count: usize::from(failed),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        offending_rows: Vec::new(),
                    }
                }
            };
            records.push(record);
        }
        if records.iter().any(|record| record.failed_count != 0) {
            return Err(Box::new(ExecutionError::VerificationFailures(
                core::mem::take(records),
            )));
        }
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
                let start = records.len();
                for verification in &group.checks {
                    match &verification.check {
                        Check::InvalidDiagnostic(diagnostic) => {
                            return Err(Box::new(ExecutionError::VerificationDiagnostic {
                                diagnostic: diagnostic.clone(),
                                records: core::mem::take(records),
                            }))
                        }
                        Check::InvalidDeclaration {
                            condition,
                            requirement,
                            reason,
                        } => {
                            return Err(Box::new(ExecutionError::VerificationDeclaration {
                                path: verification.path.clone(),
                                condition,
                                requirement,
                                reason: reason.clone(),
                                records: core::mem::take(records),
                            }))
                        }
                        Check::NotMissing
                        | Check::AllowedValues(_)
                        | Check::Codelist { .. }
                        | Check::Range { .. }
                        | Check::MaxLength(_)
                        | Check::Matches(_) => {
                            let width = match &verification.check {
                                Check::AllowedValues(accepted) => accepted.len(),
                                Check::Codelist { values, .. } => values.len().max(1),
                                Check::Range { .. } => 2,
                                _ => 1,
                            };
                            budget.work(candidates.len(), width)?;
                            if matches!(verification.check, Check::MaxLength(_)) {
                                for row in candidates {
                                    if let Value::Str(text) = &row.values[group.column] {
                                        budget.scalar_text(text.len())?;
                                    }
                                }
                            }
                            let values = candidates.iter().map(|row| &row.values[group.column]);
                            let offending = if let Check::Matches(pattern) = &verification.check {
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
                            let definition =
                                yamaa_core::dataset_checks::column_definition(&verification.check)
                                    .expect("admitted column check");
                            let codelist =
                                if let Check::Codelist { id, .. } = &verification.check {
                                    budget.scalar_text(id.len())?;
                                    budget.identity(offending.iter().map(|&position| {
                                        &candidates[position].values[group.column]
                                    }))?;
                                    Some(CodelistObservation {
                                        id: id.clone(),
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
                                condition: definition.condition,
                                requirement: definition
                                    .requirement
                                    .expect("column presence has a requirement"),
                                evaluated_count: candidates.len(),
                                failed_count: offending_rows.len(),
                                output_rows: candidates.len(),
                                codelist,
                                offending_rows,
                            });
                        }
                        _ => unreachable!("column check vocabulary is admitted in core"),
                    }
                }
                if records[start..]
                    .iter()
                    .any(|record| record.failed_count != 0)
                {
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
