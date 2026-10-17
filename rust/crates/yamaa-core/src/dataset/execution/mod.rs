//! Execute admitted dataset plans over typed source snapshots and explicit ports.

mod diagnostics;

mod conversion;
mod intermediates;
mod relation_predicates;
use super::*;
mod case_predicates;
mod flags;
mod functions;
use crate::function_invocation::FunctionPort;

mod aggregates;
mod imputation;
mod joined;
mod key_grain;
mod lookup;
mod mapping_cut;
mod numeric;
mod odm;
mod ordering;
mod predicates;
mod row_drivers;
mod row_windows;
mod scalar;
mod self_relations;
mod str_case;
mod str_extract;
mod str_pad;
mod str_replace;
mod str_template;
mod windows;

use crate::limits::Limits;
use crate::{
    bound_expression::BoundPredicate,
    conversion::{convert, ConversionError},
    reduction::NumericReducer,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::{Value, ValueType},
};
use crate::{
    numeric_lifecycle::{HandlerCount, HandlerCountOverflow, HandlerCounter, HandlerKind},
    table_grouping::{partition, GroupingError},
    table_reduction::{count_selected, reduce_column, TableReductionError},
};
use std::convert::Infallible;
use std::{collections::BTreeMap, vec};

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

/// Completion is emitted at the actual admitted checkpoint or returned result,
/// before Finished. A panic retains entry without inventing a returned outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionEvent {
    Phase(ExecutionPhase),
    Exit {
        phase: ExecutionPhase,
        outcome: crate::application_observer::Outcome,
    },
    Refusal {
        phase: ExecutionPhase,
        refusal: crate::application_observer::Refusal,
    },
}

/// Fixed boundary failures remain distinct from normative language conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    SourceRows,
    SourceCells,
    SourceColumns,
    SourceCount,
    OutputRows,
    OutputCells,
    OutputColumns,
    RegexWork,
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
    pub code: crate::diagnostic::ConditionCode,
    pub evaluated_count: usize,
    pub failed_count: usize,
    pub output_rows: usize,
    pub offending_rows: Vec<RowIdentity>,
    /// Full offending values survive discarded output.
    pub codelist: Option<CodelistObservation>,
    pub inventory: Option<Vec<Value>>,
    pub row_count: Option<RowCountObservation>,
}

impl CheckRecord {
    pub fn definition(&self) -> crate::diagnostic::Definition {
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
    ScalarDiagnostic(crate::diagnostic::Diagnostic),
    IntermediateVerifications(Vec<crate::diagnostic::Diagnostic>),
    IntermediateFilterTypes(Vec<crate::diagnostic::Diagnostic>),
    SemanticDiagnostic {
        diagnostic: crate::diagnostic::Diagnostic,
        identity: Option<RowIdentity>,
    },
    VerificationDiagnostic {
        diagnostic: crate::diagnostic::Diagnostic,
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
            crate::function_signature::ProjectFunctionIdentity,
        >,
        identity: Option<RowIdentity>,
    },
    Numeric {
        error: crate::numeric_compiler::CompiledEvaluationError<Infallible>,
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
        code: crate::diagnostic::ConditionCode,
        reason: String,
        records: Vec<CheckRecord>,
    },
    VerificationPredicate {
        error: crate::predicate::EvaluationError<CellError<Infallible>>,
        records: Vec<CheckRecord>,
    },
    Predicate {
        source_row: usize,
        error: crate::predicate::EvaluationError<CellError<E>>,
    },
    Limit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
    },
    /// Zero names the primary table; positive indices name the secondary ports.
    SourceLimit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
        source_index: usize,
    },
    /// Resource facts with an authored owner, such as a verification pattern.
    ScopedLimit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
        spec_paths: Vec<String>,
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
    /// Destination-free donor preparation cannot read named selections.
    Probe,
    /// One physical driving record while its ordered output keys are derived.
    KeyCandidate(usize),
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
fn own(value: ValueRef<'_>) -> Value {
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

fn regex_limit<E>(error: crate::regex::MatchError) -> ExecutionError<E> {
    ExecutionError::Limit {
        resource: Resource::RegexWork,
        limit: error.limit,
        required: None,
    }
}

/// Resources and semantic observations belong to the same attempted run.
struct EvaluationState<'a, E> {
    primary: &'a dyn TableAccess<Error = E>,
    functions: &'a mut dyn FunctionPort<Error = E>,
    secondary: &'a [&'a dyn TableAccess<Error = E>],
    self_rows: &'a [Candidate],
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
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let value = evaluate_raw_retained(table, assignment, candidate, plan, row, state, true)?;
    finish(value, assignment, candidate, plan, row, state.handlers)
}

fn evaluate_raw<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    evaluate_raw_retained(table, assignment, candidate, plan, row, state, false)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_raw_retained<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
    retained: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    if let Expression::Function(function) = &assignment.expression {
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
            state.handlers,
        );
    }
    if let Expression::Joined(read) = &assignment.expression {
        return joined::read(table, read, assignment, candidate, plan, row, state);
    }
    if let Expression::JoinedSource {
        read,
        filter,
        selection,
    } = &assignment.expression
    {
        return joined::read_options(
            table,
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

            state
                .handlers
                .record(&std::format!("{path}.absent"), HandlerKind::Absent)
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            return Ok(absent.clone());
        }
        return evaluate_raw_retained(table, source, candidate, plan, row, state, retained);
    }
    if let Expression::FirstAvailable(selection) = &assignment.expression {
        return evaluate_first_available(
            table, selection, assignment, candidate, plan, row, state, retained,
        );
    }
    if let Expression::Flag(flag) = &assignment.expression {
        return flags::evaluate(flag, table, candidate, plan, row, assignment, state);
    }
    if let Expression::Case(branches) = &assignment.expression {
        for branch in branches {
            let selected = match &branch.when {
                None => true,
                Some(when) => {
                    let truth = case_predicates::evaluate(
                        when, table, candidate, plan, row, assignment, state,
                    )?;
                    truth == crate::predicate::Truth::True
                }
            };
            if selected {
                return evaluate_raw_retained(
                    table,
                    &branch.assignment,
                    candidate,
                    plan,
                    row,
                    state,
                    retained,
                );
            }
        }
        return Ok(Value::Missing);
    }
    if let Expression::MappingCut { source, operation } = &assignment.expression {
        return mapping_cut::evaluate(
            assignment, source, operation, table, candidate, plan, row, state, retained,
        );
    }
    if let Expression::Round { source, rounding } = &assignment.expression {
        let value = evaluate_raw(table, source, candidate, plan, row, state)?;
        return rounding
            .apply(ValueRef::from(&value))
            .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)));
    }
    if let Expression::StrTemplate {
        operands,
        operation,
    } = &assignment.expression
    {
        return str_template::evaluate(operands, operation, table, candidate, plan, row, state);
    }
    if matches!(
        &assignment.expression,
        Expression::Extreme { .. } | Expression::StrConcat { .. } | Expression::StrContains { .. }
    ) {
        return scalar::evaluate(assignment, table, candidate, plan, row, state);
    }
    if let Expression::StrExtract { source, operation } = &assignment.expression {
        return str_extract::evaluate(source, operation, table, candidate, plan, row, state);
    }
    if let Expression::StrReplace { source, operation } = &assignment.expression {
        return str_replace::evaluate(source, operation, table, candidate, plan, row, state);
    }
    if let Expression::StrCase { source, operation } = &assignment.expression {
        return str_case::evaluate(source, operation, table, candidate, plan, row, state);
    }
    if let Expression::StrPad { source, operation } = &assignment.expression {
        return str_pad::evaluate(source, operation, table, candidate, plan, row, state);
    }
    if let Expression::Imputation {
        operands,
        operation,
    } = &assignment.expression
    {
        return imputation::evaluate(operands, operation, table, candidate, plan, row, state);
    }
    if let Expression::Temporal {
        operands,
        operation,
    } = &assignment.expression
    {
        return evaluate_temporal(table, operands, operation, candidate, plan, row, state);
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
    evaluate_leaf(table, assignment, candidate, plan, row, state)
}
/// Convert and account one completed result before publishing its column slot.
fn finish<E>(
    value: Value,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let converted = match convert(&value, plan.output().columns()[assignment.column].kind) {
        Ok(value) => value,
        Err(error) => {
            match conversion::recover(error, assignment, candidate, plan, row, handlers) {
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

                            let mut diagnostic = error.clone().into_diagnostic(result_path.clone());
                            diagnostic.spec_paths.push(std::format!("{path}.absent"));
                            diagnostic.context.insert(
                                "variable".into(),
                                crate::json::Value::String(variable.clone()),
                            );
                            diagnostic.context.insert(
                                "handler".into(),
                                crate::json::Value::String("absent".into()),
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

    Ok(converted)
}

impl DatasetPlan {
    /// Execute the admitted sources and explicit function port once, retaining
    /// handler and verification evidence on both success and failure.
    pub fn execute<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
    ) -> ExecutionAttempt<T::Error> {
        let mut ignore = |_| {};
        let observer = observer.unwrap_or(&mut ignore);
        let active = std::cell::Cell::new(ExecutionPhase::Admission);
        let mut observer = |event| {
            if let ExecutionEvent::Phase(phase) = event {
                if phase != ExecutionPhase::Finished {
                    active.set(phase);
                }
            }
            observer(event);
        };
        observer(ExecutionEvent::Phase(ExecutionPhase::Admission));
        let mut observations = AttemptObservations::default();
        for declaration in self.conversion_handlers() {
            observations
                .handlers
                .register(&declaration.handler.spec_path, HandlerKind::Unconvertible);
        }
        for intermediate in self.intermediates() {
            if let Some(derived) = &intermediate.derivations {
                for declaration in derived.plan.conversion_handlers() {
                    observations
                        .handlers
                        .register(&declaration.handler.spec_path, HandlerKind::Unconvertible);
                }
            }
        }
        for assignment in self
            .templates()
            .iter()
            .flat_map(|template| &template.assignments)
            .chain(self.columns())
            .chain(
                self.intermediates()
                    .iter()
                    .flat_map(|item| &item.keys)
                    .filter_map(|key| {
                        if let MatchValue::Expression(expression) = &key.value {
                            Some(&expression.assignment)
                        } else {
                            None
                        }
                    }),
            )
        {
            scalar::register(assignment, &mut observations.handlers);
            imputation::register(&assignment.expression, &mut observations.handlers);
            str_case::register(&assignment.expression, &mut observations.handlers);
            str_extract::register(&assignment.expression, &mut observations.handlers);
            str_replace::register(&assignment.expression, &mut observations.handlers);
            str_template::register(&assignment.expression, &mut observations.handlers);
        }
        let result = self.execute_inner(
            table,
            secondary,
            functions,
            &mut observations,
            &mut observer,
        );
        if let Err(error) = &result {
            if let ExecutionError::Limit {
                resource,
                limit,
                required,
            }
            | ExecutionError::ScopedLimit {
                resource,
                limit,
                required,
                ..
            }
            | ExecutionError::SourceLimit {
                resource,
                limit,
                required,
                ..
            } = error.as_ref()
            {
                observer(ExecutionEvent::Refusal {
                    phase: active.get(),
                    refusal: crate::application_observer::Refusal {
                        resource: *resource,
                        limit: *limit,
                        required: *required,
                    },
                });
            }
        }
        observer(ExecutionEvent::Exit {
            phase: active.get(),
            outcome: crate::application_observer::Outcome::from_result(&result),
        });
        let attempt = ExecutionAttempt {
            result,
            handler_counts: observations.handlers.snapshot().to_vec(),
            retained_verifications: observations.verifications,
        };
        observer(ExecutionEvent::Phase(ExecutionPhase::Finished));
        attempt
    }

    /// Share the retained semantic observations for this immutable plan attempt.
    #[allow(clippy::too_many_arguments)]
    fn execute_inner<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observations: &mut AttemptObservations,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        functions::admit_plan(self, functions)?;
        if table.schema() != self.source()
            || secondary.len() != self.secondary().len()
            || secondary
                .iter()
                .zip(self.secondary())
                .any(|(table, expected)| table.schema() != &expected.schema)
        {
            return Err(Box::new(ExecutionError::SchemaMismatch));
        }
        let sources = secondary.len().checked_add(1);
        if sources.is_none_or(|count| count > Limits::SOURCE_OPERANDS) {
            return Err(Box::new(ExecutionError::Limit {
                resource: Resource::SourceCount,
                limit: Limits::SOURCE_OPERANDS,
                required: sources,
            }));
        }
        for (source_index, (rows, columns)) in
            std::iter::once((table.row_count(), table.schema().columns().len()))
                .chain(
                    secondary
                        .iter()
                        .map(|source| (source.row_count(), source.schema().columns().len())),
                )
                .enumerate()
        {
            if let Err(error) = Limits::table(rows, columns) {
                return Err(Box::new(ExecutionError::SourceLimit {
                    resource: match error.resource {
                        "columns" => Resource::SourceColumns,
                        "table_rows" => Resource::SourceRows,
                        "table_cells" => Resource::SourceCells,
                        _ => unreachable!("table boundary resource"),
                    },
                    limit: error.limit,
                    required: error.actual,
                    source_index,
                }));
            }
        }
        self.check_capacity(0, 0)?;
        observer(ExecutionEvent::Exit {
            phase: ExecutionPhase::Admission,
            outcome: crate::application_observer::Outcome::Succeeded,
        });
        observer(ExecutionEvent::Phase(ExecutionPhase::Derivation));
        self.execute_admitted(table, secondary, functions, observations, observer)
    }
    #[allow(clippy::too_many_arguments)]
    fn execute_admitted<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observations: &mut AttemptObservations,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        let AttemptObservations {
            handlers,
            verifications: records,
        } = observations;
        let primary = row_drivers::Driver::Primary(table);
        let mut intermediate_run = intermediates::Run::default();
        intermediates::verify(
            self,
            false,
            &mut EvaluationState {
                primary: &primary,
                functions,
                secondary,
                self_rows: &[],
                handlers,
                intermediates: &mut intermediate_run,
                output_rows: &[],
                output_offset: 0,
                windows: &mut windows::Cache::default(),
                aggregate_row: None,
            },
        )?;
        let mut candidates = self.build_candidates(
            table,
            secondary,
            functions,
            handlers,
            &mut intermediate_run,
            &primary,
        )?;
        self.complete_columns(
            table,
            secondary,
            functions,
            handlers,
            &mut intermediate_run,
            &primary,
            &mut candidates,
            records,
        )?;
        self.verify_output(
            table,
            secondary,
            functions,
            handlers,
            &mut intermediate_run,
            &primary,
            &candidates,
            records,
            observer,
        )?;
        let mut dataset = Dataset {
            schema: self.output().clone(),
            rows: candidates.into_iter().map(|row| row.values).collect(),
        };
        ordering::apply(self.output_order(), &mut dataset)?;
        Ok(Execution {
            dataset,
            verifications: std::mem::take(records),
        })
    }

    /// Construct all output candidates, either from the key grain of the
    /// sole key-mode template or by processing every row template in order.
    #[allow(clippy::too_many_arguments)]
    fn build_candidates<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        handlers: &mut HandlerCounter,
        intermediate_run: &mut intermediates::Run,
        primary: &row_drivers::Driver<'_, T>,
    ) -> Result<Vec<Candidate>, Box<ExecutionError<T::Error>>> {
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut next_selection_identity = 0usize;
        if matches!(self.templates()[0].mode, RowMode::Keys) {
            candidates = key_grain::construct(
                self,
                table,
                &mut EvaluationState {
                    primary,
                    functions,
                    secondary,
                    self_rows: &[],
                    handlers,
                    intermediates: &mut *intermediate_run,
                    output_rows: &[],
                    output_offset: 0,
                    windows: &mut windows::Cache::default(),
                    aggregate_row: None,
                },
            )?;
        } else {
            for (template_index, template) in self.templates().iter().enumerate() {
                next_selection_identity = self.process_template(
                    template_index,
                    template,
                    table,
                    secondary,
                    functions,
                    handlers,
                    &mut *intermediate_run,
                    primary,
                    &mut candidates,
                    next_selection_identity,
                )?;
            }
        }
        Ok(candidates)
    }

    /// Build the candidates of one row template: evaluate its assignments,
    /// apply its pre- and post-window filters and publish its self rows.
    #[allow(clippy::too_many_arguments)]
    fn process_template<T: TableAccess + ?Sized>(
        &self,
        template_index: usize,
        template: &RowTemplate,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        handlers: &mut HandlerCounter,
        intermediate_run: &mut intermediates::Run,
        primary: &row_drivers::Driver<'_, T>,
        candidates: &mut Vec<Candidate>,
        mut next_selection_identity: usize,
    ) -> Result<usize, Box<ExecutionError<T::Error>>> {
        let named = if let RowMode::IntermediateDriver { index, .. } = &template.mode {
            Some(intermediates::driver(
                *index,
                self,
                &mut EvaluationState {
                    primary,
                    functions,
                    secondary,
                    self_rows: candidates,
                    handlers,
                    intermediates: &mut *intermediate_run,
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
        let stored = row_drivers::Driver::new(table, secondary, template.mode.source_index());
        let table = if let Some(data) = &data {
            row_drivers::Driver::Secondary(data)
        } else {
            stored
        };
        let table = &table;
        let first = candidates.len();
        let deferred = crate::dataset::window_stages(
            &template.assignments,
            self.output().columns().len(),
            self.intermediates(),
        );
        // Grouped predicates may read the completed window pass. Predicates
        // independent of windows keep their single prewindow eligibility pass.
        let postwindow_filter = matches!(template.mode.local_mode(), RowMode::Groups(_))
            && template.filter.as_ref().is_some_and(|filter| {
                filter.bindings().iter().any(|binding| {
                    crate::dataset::read_requires_window(
                        &binding.read,
                        &deferred,
                        self.intermediates(),
                    )
                })
            });
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
                self.check_capacity(candidates.len(), rows)?;
                if let Some(relation) = &named {
                    relation.eligible.iter().map(|&row| vec![row]).collect()
                } else {
                    (0..table.row_count()).map(|row| vec![row]).collect()
                }
            }
            RowMode::Groups(keys) => {
                if let Some(relation) = &named {
                    intermediates::driver_groups(table, &relation.eligible, keys)?
                } else {
                    partition(table, keys)
                        .map_err(|error| Box::new(ExecutionError::Grouping(error)))?
                }
            }
        };
        self.check_capacity(candidates.len(), groups.len())?;
        candidates
            .try_reserve(groups.len())
            .map_err(|_| Box::new(ExecutionError::Allocation))?;
        for members in groups {
            let selection_identity = SelectionIdentity::Candidate(next_selection_identity);
            next_selection_identity = next_selection_identity
                .checked_add(1)
                .ok_or_else(|| Box::new(ExecutionError::Capacity))?;
            let mut candidate = Candidate {
                template: Some(template_index),
                selection_identity,
                members,
                values: vec![Value::Missing; self.output().columns().len()],
                completed: vec![false; self.output().columns().len()],
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
                    &mut EvaluationState {
                        primary,
                        functions,
                        secondary,
                        self_rows: &candidates[..first],
                        handlers,
                        intermediates: &mut *intermediate_run,
                        output_rows: &[],
                        output_offset: 0,
                        windows: &mut windows::Cache::default(),
                        aggregate_row: None,
                    },
                )?;
                candidate.completed[assignment.column] = true;
            }
            if let Some(filter) = template.filter.as_ref().filter(|_| !postwindow_filter) {
                let source_row = candidate.members[0];
                let truth = if filter.bindings().iter().any(|binding| {
                    matches!(
                        binding.read,
                        crate::bound_expression::Read::Intermediate { .. }
                            | crate::bound_expression::Read::Joined(_)
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
                            primary,
                            functions,
                            secondary,
                            self_rows: &candidates[..first],
                            handlers,
                            intermediates: &mut *intermediate_run,
                            output_rows: &[],
                            output_offset: 0,
                            windows: &mut windows::Cache::default(),
                            aggregate_row: None,
                        },
                    )?
                } else {
                    crate::dataset::execution::predicates::evaluate(
                        filter,
                        table,
                        source_row,
                        &candidate.values,
                    )
                    .map_err(|error| match error.kind {
                        crate::predicate::ErrorKind::Regex(limit) => Box::new(regex_limit(limit)),
                        _ => Box::new(ExecutionError::Predicate { source_row, error }),
                    })?
                };
                if truth != crate::predicate::Truth::True {
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
            &mut EvaluationState {
                primary,
                functions,
                secondary,
                self_rows: prior,
                handlers,
                intermediates: &mut *intermediate_run,
                output_rows: &[],
                output_offset: 0,
                windows: &mut windows::Cache::default(),
                aggregate_row: None,
            },
        )?;
        if postwindow_filter {
            self.apply_postwindow_filter(
                template,
                table,
                primary,
                secondary,
                functions,
                handlers,
                &mut *intermediate_run,
                candidates,
                first,
            )?;
        }
        intermediate_run.publish_self(self)?;
        intermediates::verify(
            self,
            true,
            &mut EvaluationState {
                primary,
                functions,
                secondary,
                self_rows: candidates,
                handlers,
                intermediates: &mut *intermediate_run,
                output_rows: &[],
                output_offset: 0,
                windows: &mut windows::Cache::default(),
                aggregate_row: None,
            },
        )?;
        Ok(next_selection_identity)
    }

    /// Retain only the candidates whose deferred post-window filter is true,
    /// preserving their relative order.
    #[allow(clippy::too_many_arguments)]
    fn apply_postwindow_filter<T: TableAccess + ?Sized>(
        &self,
        template: &RowTemplate,
        table: &row_drivers::Driver<'_, T>,
        primary: &row_drivers::Driver<'_, T>,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        handlers: &mut HandlerCounter,
        intermediate_run: &mut intermediates::Run,
        candidates: &mut Vec<Candidate>,
        first: usize,
    ) -> Result<(), Box<ExecutionError<T::Error>>> {
        let filter = template.filter.as_ref().expect("deferred filter exists");
        let mut retained = first;
        for row in first..candidates.len() {
            let candidate = &candidates[row];
            let source_row = candidate.members[0];
            let truth = if filter.bindings().iter().any(|binding| {
                matches!(
                    binding.read,
                    crate::bound_expression::Read::Intermediate { .. }
                        | crate::bound_expression::Read::Joined(_)
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
                    candidate,
                    self,
                    row,
                    &assignment,
                    &mut EvaluationState {
                        primary,
                        functions,
                        secondary,
                        self_rows: &candidates[..first],
                        handlers,
                        intermediates: &mut *intermediate_run,
                        output_rows: &candidates[first..],
                        output_offset: first,
                        windows: &mut windows::Cache::default(),
                        aggregate_row: None,
                    },
                )?
            } else {
                crate::dataset::execution::predicates::evaluate(
                    filter,
                    table,
                    source_row,
                    &candidate.values,
                )
                .map_err(|error| match error.kind {
                    crate::predicate::ErrorKind::Regex(limit) => Box::new(regex_limit(limit)),
                    _ => Box::new(ExecutionError::Predicate { source_row, error }),
                })?
            };
            if truth == crate::predicate::Truth::True {
                candidates.swap(retained, row);
                retained += 1;
            }
        }
        candidates.truncate(retained);
        Ok(())
    }

    /// Complete the declared column assignments over every candidate,
    /// checking each available prefix of column verifications in order.
    #[allow(clippy::too_many_arguments)]
    fn complete_columns<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        handlers: &mut HandlerCounter,
        intermediate_run: &mut intermediates::Run,
        primary: &row_drivers::Driver<'_, T>,
        candidates: &mut Vec<Candidate>,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Box<ExecutionError<T::Error>>> {
        intermediate_run.begin_columns(self)?;
        let mut completed = vec![false; self.output().columns().len()];
        for assignment in &self.templates()[0].assignments {
            completed[assignment.column] = true;
        }
        let mut next_column = 0;
        self.check_columns(candidates, &completed, &mut next_column, records)?;
        for assignment in self.columns() {
            let mut nested_windows = windows::Cache::default();
            let mut numbers =
                windows::Evaluation::new(assignment, candidates, self, table, handlers)?;
            for row in 0..candidates.len() {
                let number = if let Some(run) = &mut numbers {
                    Some(run.value(row, candidates, table, assignment, self)?)
                } else {
                    None
                };
                let candidate = &candidates[row];
                let value = if let Some(value) = number {
                    finish(value, assignment, candidate, self, row, handlers)?
                } else {
                    evaluate(
                        table,
                        assignment,
                        candidate,
                        self,
                        row,
                        &mut EvaluationState {
                            primary,
                            functions,
                            secondary,
                            self_rows: candidates,
                            handlers,
                            intermediates: &mut *intermediate_run,
                            output_rows: candidates,
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
            self.check_columns(candidates, &completed, &mut next_column, records)?;
        }
        // A selector hidden by an empty input or a lazy branch must still report
        // its known filter-type refusal before a successful result can publish.
        intermediates::check_unobserved_filter_types(self)?;
        Ok(())
    }

    /// Check output key completeness and uniqueness, then run every
    /// declared verification over the completed rows.
    #[allow(clippy::too_many_arguments)]
    fn verify_output<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        handlers: &mut HandlerCounter,
        intermediate_run: &mut intermediates::Run,
        primary: &row_drivers::Driver<'_, T>,
        candidates: &[Candidate],
        records: &mut Vec<CheckRecord>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<(), Box<ExecutionError<T::Error>>> {
        observer(ExecutionEvent::Exit {
            phase: ExecutionPhase::Derivation,
            outcome: crate::application_observer::Outcome::Succeeded,
        });
        observer(ExecutionEvent::Phase(ExecutionPhase::OutputKeys));
        let dataset = CompletedRows {
            schema: self.output(),
            rows: candidates.iter().map(|row| &row.values).collect(),
        };
        let mut failures = Vec::new();

        for (position, &column) in self.keys().iter().enumerate() {
            let missing: Vec<_> = dataset
                .rows
                .iter()
                .enumerate()
                .filter_map(|(row, values)| matches!(values[column], Value::Missing).then_some(row))
                .collect();
            if !missing.is_empty() {
                failures.push(CheckRecord {
                    path: std::format!("keys[{position}]"),
                    code: crate::diagnostic::ConditionCode::OutputMissingKeyFailed,
                    evaluated_count: dataset.rows.len(),
                    failed_count: missing.len(),
                    output_rows: dataset.rows.len(),
                    codelist: None,
                    inventory: None,

                    row_count: None,
                    offending_rows: identities(&dataset, self.keys(), missing)?,
                });
            }
        }

        let groups = partition(&dataset, self.keys())
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
        let duplicates: Vec<_> = groups
            .iter()
            .filter(|members| members.len() > 1)
            .map(|members| members[0])
            .collect();
        if !duplicates.is_empty() {
            failures.push(CheckRecord {
                path: "keys".into(),
                code: crate::diagnostic::ConditionCode::OutputDuplicateKeyFailed,
                evaluated_count: groups.len(),
                failed_count: duplicates.len(),
                output_rows: dataset.rows.len(),
                codelist: None,
                inventory: None,

                row_count: None,
                offending_rows: identities(&dataset, self.keys(), duplicates)?,
            });
        }
        if !failures.is_empty() {
            return Err(Box::new(ExecutionError::KeyFailures(failures)));
        }
        observer(ExecutionEvent::Exit {
            phase: ExecutionPhase::OutputKeys,
            outcome: crate::application_observer::Outcome::Succeeded,
        });
        observer(ExecutionEvent::Phase(ExecutionPhase::Verification));
        for verification in self.verifications() {
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
                        records: std::mem::take(records),
                    }));
                }
                Check::InvalidDeclaration { code, reason } => {
                    return Err(Box::new(ExecutionError::VerificationDeclaration {
                        path: verification.path.clone(),
                        code: *code,
                        reason: reason.clone(),
                        records: std::mem::take(records),
                    }));
                }
                Check::Assert { .. } | Check::PredicateDeclaration(_) => {
                    let Some(record) = verification::predicate_check(
                        verification,
                        &dataset,
                        self,
                        table,
                        candidates,
                        &mut EvaluationState {
                            primary,
                            functions,
                            secondary,
                            self_rows: candidates,
                            handlers,
                            intermediates: &mut *intermediate_run,
                            output_rows: candidates,
                            aggregate_row: None,
                            output_offset: 0,
                            windows: &mut windows::Cache::default(),
                        },
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

                    let groups = partition(&dataset, &distinct)
                        .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
                    let repeated: Vec<_> =
                        groups.iter().filter(|members| members.len() > 1).collect();
                    CheckRecord {
                        path: verification.path.clone(),
                        code: crate::diagnostic::ConditionCode::VerificationUniqueFailed,
                        evaluated_count: groups.len(),
                        failed_count: repeated.len(),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        inventory: None,

                        row_count: None,
                        offending_rows: identities(
                            &dataset,
                            self.keys(),
                            repeated
                                .into_iter()
                                .flat_map(|members| members.iter().copied()),
                        )?,
                    }
                }
                Check::AllOrNone(columns) => {
                    let offending = dataset.rows.iter().enumerate().filter_map(|(index, row)| {
                        let missing = matches!(row[columns[0]], Value::Missing);
                        columns
                            .iter()
                            .any(|&column| matches!(row[column], Value::Missing) != missing)
                            .then_some(index)
                    });
                    let offending_rows = identities(&dataset, self.keys(), offending)?;
                    CheckRecord {
                        path: verification.path.clone(),
                        code: crate::diagnostic::ConditionCode::VerificationAllOrNoneFailed,
                        evaluated_count: dataset.rows.len(),
                        failed_count: offending_rows.len(),
                        output_rows: dataset.rows.len(),
                        codelist: None,
                        inventory: None,

                        row_count: None,
                        offending_rows,
                    }
                }
                Check::ScopedRowCount(check) => verification::row_count_check(
                    verification,
                    check,
                    &dataset,
                    self,
                    table,
                    candidates,
                    &mut EvaluationState {
                        primary,
                        functions,
                        secondary,
                        self_rows: candidates,
                        handlers,
                        intermediates: &mut *intermediate_run,
                        output_rows: candidates,
                        aggregate_row: None,
                        output_offset: 0,
                        windows: &mut windows::Cache::default(),
                    },
                    records,
                )?,
                Check::RowCount { .. } | Check::WideRowCount { .. } => {
                    let failed = !verification
                        .check
                        .row_count_accepts(dataset.rows.len())
                        .unwrap_or(false);
                    CheckRecord {
                        path: verification.path.clone(),
                        code: crate::diagnostic::ConditionCode::VerificationRowCountFailed,
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
                && self.verification_severity(&record.path)
                    == crate::dataset::VerificationSeverity::Error
        }) {
            return Err(Box::new(ExecutionError::VerificationFailures(
                std::mem::take(records),
            )));
        }
        Ok(())
    }

    /// Check only the available declared prefix, after every key is complete.
    fn check_columns<E>(
        &self,
        candidates: &[Candidate],
        completed: &[bool],
        next: &mut usize,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Box<ExecutionError<E>>> {
        if self.keys().iter().any(|&column| !completed[column]) {
            return Ok(());
        }
        while *next < completed.len() && completed[*next] {
            if let Some(group) = self
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
                                records: std::mem::take(records),
                            }));
                        }
                        Check::InvalidDeclaration { code, reason } => {
                            return Err(Box::new(ExecutionError::VerificationDeclaration {
                                path: verification.path.clone(),
                                code: *code,
                                reason: reason.clone(),
                                records: std::mem::take(records),
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
                                let values = positions
                                    .iter()
                                    .map(|&position| &candidates[position].values[group.column]);
                                let offending = if let Some(binding) = binding {
                                    values
                                        .enumerate()
                                        .filter_map(|(position, value)| {
                                            (!crate::dataset_checks::codelist_accepts(
                                                &binding.values,
                                                value,
                                            ))
                                            .then_some(position)
                                        })
                                        .collect()
                                } else if let Check::Matches(pattern) = &verification.check {
                                    crate::dataset_checks::matches_offenders(pattern, values)
                                        .map_err(|error| {
                                            let ExecutionError::Limit {
                                                resource,
                                                limit,
                                                required,
                                            } = regex_limit::<E>(error)
                                            else {
                                                unreachable!("predicate resource limit")
                                            };
                                            Box::new(ExecutionError::ScopedLimit {
                                                resource,
                                                limit,
                                                required,
                                                spec_paths: vec![format!(
                                                    "{}.pattern",
                                                    verification.path
                                                )],
                                            })
                                        })?
                                } else {
                                    crate::dataset_checks::column_offenders(
                                        &verification.check,
                                        values,
                                    )
                                };
                                let offending = offending
                                    .into_iter()
                                    .map(|index| positions[index])
                                    .collect::<Vec<_>>();
                                let code = crate::dataset_checks::column_code(&verification.check)
                                    .expect("admitted column check");
                                let identifier = binding.map(|binding| binding.id.as_str()).or({
                                    if let Check::Codelist { id, .. } = &verification.check {
                                        Some(id.as_str())
                                    } else {
                                        None
                                    }
                                });
                                let codelist = identifier.map(|id| CodelistObservation {
                                    id: id.into(),
                                    values: offending
                                        .iter()
                                        .map(|&position| {
                                            candidates[position].values[group.column].clone()
                                        })
                                        .collect(),
                                });
                                let inventory =
                                    if matches!(verification.check, Check::Inventory { .. }) {
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
                                            self.keys(),
                                            position,
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
                        && self.verification_severity(&record.path)
                            == crate::dataset::VerificationSeverity::Error
                }) {
                    return Err(Box::new(ExecutionError::VerificationFailures(
                        std::mem::take(records),
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
    ) -> Result<(), Box<ExecutionError<E>>> {
        let rows = current.checked_add(additional).ok_or_else(|| {
            Box::new(ExecutionError::Limit {
                resource: Resource::OutputRows,
                limit: Limits::TABLE_ROWS,
                required: None,
            })
        })?;
        Limits::table(rows, self.output().columns().len()).map_err(|error| {
            if error.resource == "columns" {
                return Box::new(ExecutionError::ScopedLimit {
                    resource: Resource::OutputColumns,
                    limit: error.limit,
                    required: error.actual,
                    spec_paths: vec!["output.columns".into()],
                });
            }
            Box::new(ExecutionError::Limit {
                resource: match error.resource {
                    "table_rows" => Resource::OutputRows,
                    "table_cells" => Resource::OutputCells,
                    _ => unreachable!("table boundary resource"),
                },
                limit: error.limit,
                required: error.actual,
            })
        })?;
        Ok(())
    }
}

fn scoped_table_boundary<E>(
    error: crate::limits::LimitExceeded,
    path: &str,
) -> Box<ExecutionError<E>> {
    Box::new(ExecutionError::ScopedLimit {
        resource: match error.resource {
            "columns" => Resource::SourceColumns,
            "table_rows" => Resource::SourceRows,
            "table_cells" => Resource::SourceCells,
            _ => unreachable!("table boundary resource"),
        },
        limit: error.limit,
        required: error.actual,
        spec_paths: vec![path.into()],
    })
}

/// Retain original output keys without substituting projected or row-number identity.
pub(crate) fn identities<E>(
    dataset: &CompletedRows<'_>,
    keys: &[usize],
    rows: impl IntoIterator<Item = usize>,
) -> Result<Vec<RowIdentity>, Box<ExecutionError<E>>> {
    let mut identities = Vec::new();
    for position in rows {
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
    diagnostic: crate::diagnostic::Diagnostic,
    retain_keys: bool,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
) -> Box<ExecutionError<E>> {
    let identity = if retain_keys {
        match failure_identity(candidate, plan.keys(), row) {
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
) -> Result<Option<RowIdentity>, Box<ExecutionError<E>>> {
    if keys.iter().any(|&key| !candidate.completed[key]) {
        return Ok(None);
    }

    Ok(Some(RowIdentity {
        position,
        values: keys
            .iter()
            .map(|&column| candidate.values[column].clone())
            .collect(),
    }))
}

mod verification;

use verification::CompletedRows;

impl<E: std::fmt::Debug> std::fmt::Display for ExecutionError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "dataset execution: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for ExecutionError<E> {}

/// Return the first present operand of a first-available selection,
/// or its declared missing value when every operand is missing.
#[allow(clippy::too_many_arguments)]
fn evaluate_first_available<T: TableAccess + ?Sized>(
    table: &T,
    selection: &FirstAvailable,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
    retained: bool,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    for operand in selection.sources() {
        let value = match &operand.read {
            crate::dataset::SelectionRead::Expression(source) => {
                evaluate_raw_retained(table, source, candidate, plan, row, state, retained)
                    .map_err(|mut error| {
                        // Collection cardinality belongs to the enclosing selection.
                        // Keep operand metadata for handlers and every other failure.
                        if let ExecutionError::MultipleValues { path, .. } = error.as_mut() {
                            path.clone_from(&assignment.path);
                        }
                        error
                    })?
            }
            crate::dataset::SelectionRead::Source(column) => {
                let source_row = candidate.members[0];
                let value = table.cell(source_row, *column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: operand.path.clone(),
                        source_row,
                        error,
                    })
                })?;
                own(value)
            }
            crate::dataset::SelectionRead::Column(column) => {
                let value = &candidate.values[*column];
                value.clone()
            }
            crate::dataset::SelectionRead::Collect {
                column,
                identifier,
                filter,
            } => key_grain::collect_bound(
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
                state.handlers,
            )
            .map_err(|mut error| {
                if let ExecutionError::MultipleValues { path, .. } = error.as_mut() {
                    *path = assignment.path.clone();
                }
                error
            })?,
        };
        if value != Value::Missing {
            return Ok(value);
        }
    }
    Ok(selection.missing().clone())
}

/// Evaluate one temporal operation over its already admitted operands,
/// recording the declared invalid handler when a step is invalid.
#[allow(clippy::too_many_arguments)]
fn evaluate_temporal<T: TableAccess + ?Sized>(
    table: &T,
    operands: &[Assignment],
    operation: &Temporal,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let mut values = Vec::with_capacity(operands.len());
    for (index, operand) in operands.iter().enumerate() {
        let value = evaluate_raw(table, operand, candidate, plan, row, state)?;
        operation
            .validate_operand(index, &value)
            .expect("admitted operand index")
            .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)))?;
        values.push(value);
    }
    let result = operation
        .apply_values(&values)
        .expect("admitted temporal arity")
        .map_err(|diagnostic| {
            if diagnostic.definition().phase == "validation" {
                Box::new(ExecutionError::ScalarDiagnostic(diagnostic))
            } else {
                retained_diagnostic(diagnostic, true, candidate, plan, row)
            }
        })?;
    let value = match result {
        crate::dataset::TemporalStep::Value(value) => value,
        crate::dataset::TemporalStep::Invalid => {
            state
                .handlers
                .record(
                    &std::format!("{}.invalid", operation.path()),
                    HandlerKind::Invalid,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
            operation
                .invalid_handler()
                .expect("declared recovery")
                .clone()
        }
    };
    Ok(value)
}

/// Evaluate the leaf expressions that read directly from the candidate,
/// the source table or a secondary lookup without recursive delegation.
#[allow(clippy::too_many_arguments)]
fn evaluate_leaf<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let EvaluationState {
        handlers,
        secondary,
        ..
    } = state;

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
        Expression::Literal(value) => value.clone(),
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
        | Expression::StrReplace { .. }
        | Expression::StrTemplate { .. } => {
            unreachable!("recursive evaluation handled above")
        }
        Expression::Column(column) => {
            let value = &candidate.values[*column];
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
        )?,
        Expression::Lookup(lookup) => lookup::read(
            secondary[lookup.source],
            lookup,
            assignment,
            candidate,
            plan,
            row,
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
            handlers,
        )?,
        Expression::Collect { .. } => {
            key_grain::collect(table, assignment, candidate, plan, row, handlers)?
        }
        Expression::Count { column, .. } => Value::from(
            count_selected(table, *column, &candidate.members).map_err(|error| {
                Box::new(ExecutionError::Reduction {
                    path: assignment.path.clone(),
                    error,
                    identity: None,
                })
            })?,
        ),
        Expression::Reduce {
            identifier,
            column,
            reducer,
            text,
        } => match reduce_column(table, *column, &candidate.members, *reducer, text) {
            Ok(value) => Value::from(value),
            Err(TableReductionError::Reduction(
                crate::reduction::ReductionError::IncompatibleInputType { actual, .. },
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
                let identity = if matches!(&error, TableReductionError::Reduction(crate::reduction::ReductionError::Arithmetic { error, .. }) if error.phase() == "derivation")
                {
                    failure_identity(candidate, plan.keys(), row)?
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
