//! Closed typed dataset plan for bounded row construction and column derivation.
//!
//! This is an internal bridge target, not a specification parser or public backend.
//! Unsupported syntax must be rejected by a compiler before creating this plan.

#[path = "dataset_conversion.rs"]
mod conversion;
#[path = "dataset_intermediates.rs"]
mod intermediates;
pub use conversion::ConversionHandler;
#[path = "dataset_keys.rs"]
mod key_grain;
#[path = "dataset_lookup.rs"]
mod lookup;
pub use intermediates::{Intermediate, SourceSchemas};
#[path = "dataset_numeric.rs"]
mod numeric;
pub use numeric::BoundNumeric;
#[path = "dataset_windows.rs"]
mod windows;
pub use windows::{OrderTerm, Window, WindowKind};

use crate::{
    dataset_budget::Budget,
    dataset_predicate::{BindingError, BoundPredicate},
    numeric_lifecycle::{HandlerCount, HandlerCountOverflow, HandlerCounter, HandlerKind},
    table_grouping::{partition, GroupingError},
    table_reduction::{reduce_column, TableReductionError},
};
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec, vec::Vec};
use core::convert::Infallible;
use yamaa_core::{
    conversion::{convert, ConversionError},
    reduction::NumericReducer,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::Value,
};

/// Already bound expressions; no host joins or lookup fallback are implicit.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Literal(Value),
    /// Compiled scalar arithmetic over statically bound source/completed output reads.
    Compute(BoundNumeric),
    /// Column-phase windows over completed key-grain output rows.
    Window(Window),
    Source(usize),
    /// Read one record from a secondary source on completed output match values.
    Lookup(Lookup),
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
    Reduce {
        column: usize,
        reducer: NumericReducer,
        text: String,
    },
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
}

/// Assignments are supplied in resolved dependency order, not map iteration order.
#[derive(Clone, Debug, PartialEq)]
pub struct RowTemplate {
    pub mode: RowMode,
    pub assignments: Vec<Assignment>,
    pub filter: Option<BoundPredicate>,
}

/// Error-severity dataset checks supported by this closed application slice.
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    Assert(BoundPredicate),
    /// Compiler checkpoint before a later deferred declaration error; emits no record.
    PredicateDeclaration(BoundPredicate),
    Implies {
        when: BoundPredicate,
        then: BoundPredicate,
    },
    Unique(Vec<usize>),
    RowCount {
        min: Option<i64>,
        max: Option<i64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verification {
    pub path: String,
    pub check: Check,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    NoTemplates,
    InvalidColumns,
    EmptyPath,
    InvalidSource,
    UnavailableColumn,
    DuplicateAssignment,
    IncompleteRow,
    InconsistentRowColumns,
    NonGroupSource,
    UngroupedReduction,
    NonNumericReduction,
    InvalidBounds,
    DuplicateVerificationPath,
    Filter(BindingError),
    Predicate(BindingError),
    InvalidKeyMode,
    InvalidWindow,
    InvalidSourceOrder,
    InvalidLookup,
    InvalidIntermediate,
    InvalidConversionHandler,
}

/// Admitted immutable plan: all references and phase dependencies are checked once.
#[derive(Clone, Debug, PartialEq)]
pub struct DatasetPlan {
    source: TableSchema,
    secondary: Vec<SecondarySource>,
    intermediates: Vec<Intermediate>,
    output: TableSchema,
    templates: Vec<RowTemplate>,
    columns: Vec<Assignment>,
    keys: Vec<usize>,
    verifications: Vec<Verification>,
    conversion_handlers: Vec<ConversionHandler>,
    conversion_sites: BTreeMap<String, usize>,
}

/// Validate nonempty, unique, bound column lists without reading table cells.
fn validate_columns(columns: &[usize], width: usize) -> Result<(), PlanError> {
    if columns.is_empty()
        || columns
            .iter()
            .enumerate()
            .any(|(position, column)| *column >= width || columns[..position].contains(column))
    {
        return Err(PlanError::InvalidColumns);
    }
    Ok(())
}

/// Enforce grouped source scope and already-completed output dependencies.
fn validate_assignment(
    assignment: &Assignment,
    available: &mut [bool],
    source: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Intermediate],
    output: &TableSchema,
    mode: &RowMode,
) -> Result<(), PlanError> {
    if assignment.path.is_empty() {
        return Err(PlanError::EmptyPath);
    }
    if assignment.column >= available.len() {
        return Err(PlanError::InvalidColumns);
    }
    if available[assignment.column] {
        return Err(PlanError::DuplicateAssignment);
    }
    match &assignment.expression {
        // REQ-0211/0213 convert one derived value at execution, including
        // literals. Eager conversion would invent failures for empty templates
        // and move runtime conversion conditions into the planning phase.
        Expression::Literal(_) => {}
        Expression::Compute(expression) => expression.validate(source, available, mode)?,
        Expression::Intermediate { index, column } => {
            if !matches!(mode, RowMode::Keys) {
                return Err(PlanError::InvalidIntermediate);
            }
            let item = intermediates
                .get(*index)
                .ok_or(PlanError::InvalidIntermediate)?;
            lookup::validate(
                &Lookup {
                    source: item.source,
                    column: *column,
                    keys: item.keys.clone(),
                },
                secondary,
                available,
                output,
            )?;
        }
        Expression::Lookup(lookup) => {
            if !matches!(mode, RowMode::Keys) {
                return Err(PlanError::InvalidLookup);
            }
            lookup::validate(lookup, secondary, available, output)?;
        }
        Expression::Window(window) => {
            if !matches!(mode, RowMode::Keys) {
                return Err(PlanError::InvalidWindow);
            }
            window.validate(available, output)?;
        }
        Expression::Column(column) => {
            if !available.get(*column).copied().unwrap_or(false) {
                return Err(PlanError::UnavailableColumn);
            }
        }
        Expression::Source(column) => {
            if *column >= source.columns().len() {
                return Err(PlanError::InvalidSource);
            }
            if let RowMode::Groups(keys) = mode {
                if !keys.contains(column) {
                    return Err(PlanError::NonGroupSource);
                }
            }
        }
        Expression::Collect {
            column,
            identifier,
            filter,
            selection,
        } => {
            if *column >= source.columns().len() {
                return Err(PlanError::InvalidSource);
            }
            if !matches!(mode, RowMode::Keys) || identifier.is_empty() {
                return Err(PlanError::InvalidKeyMode);
            }
            if let Some(filter) = filter {
                filter
                    .validate(source.columns().len(), &[], false)
                    .map_err(PlanError::Filter)?;
            }
            if let Some(selection) = selection {
                if selection.order_by.is_empty()
                    || selection
                        .order_by
                        .iter()
                        .any(|term| term.column >= source.columns().len())
                {
                    return Err(PlanError::InvalidSourceOrder);
                }
            }
        }
        Expression::Reduce { column, text, .. } => {
            if *column >= source.columns().len() {
                return Err(PlanError::InvalidSource);
            }
            if !matches!(
                source.columns()[*column].kind,
                yamaa_core::value::ColumnType::Int | yamaa_core::value::ColumnType::Float
            ) {
                return Err(PlanError::NonNumericReduction);
            }
            if !matches!(mode, RowMode::Groups(_)) {
                return Err(PlanError::UngroupedReduction);
            }
            if text.is_empty() {
                return Err(PlanError::EmptyPath);
            }
        }
    }
    available[assignment.column] = true;
    Ok(())
}

impl DatasetPlan {
    /// Admit the complete single-source plan before any source data is accessed.
    /// Every template completes the same row-phase columns. Remaining assignments
    /// execute a whole column at a time, in supplied resolved declaration order.
    pub fn new(
        source: TableSchema,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Result<Self, PlanError> {
        Self::new_with_sources(
            source,
            Vec::new(),
            output,
            templates,
            columns,
            keys,
            verifications,
        )
    }

    /// Admit additional source schemas before accepting any snapshots or lookup reads.
    pub fn new_with_sources(
        source: TableSchema,
        secondary: Vec<SecondarySource>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Result<Self, PlanError> {
        Self::new_with_intermediates(
            SourceSchemas {
                primary: source,
                secondary,
            },
            Vec::new(),
            output,
            templates,
            columns,
            keys,
            verifications,
        )
    }

    /// Bind named selections and all read dependencies before accessing snapshots.
    pub fn new_with_intermediates(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
    ) -> Result<Self, PlanError> {
        let SourceSchemas {
            primary: source,
            secondary,
        } = sources;
        intermediates::validate(&intermediates, &secondary, &output)?;
        for (index, relation) in secondary.iter().enumerate() {
            if relation.name.is_empty()
                || secondary[..index]
                    .iter()
                    .any(|other| other.name == relation.name)
            {
                return Err(PlanError::InvalidLookup);
            }
        }
        if templates.is_empty() {
            return Err(PlanError::NoTemplates);
        }
        let width = output.columns().len();
        validate_columns(&keys, width)?;
        let mut row_columns = None;
        for template in &templates {
            let keyed = matches!(template.mode, RowMode::Keys);
            if keyed && templates.len() != 1 {
                return Err(PlanError::InvalidKeyMode);
            }
            if let RowMode::Groups(keys) = &template.mode {
                validate_columns(keys, source.columns().len())?;
            }
            let mut available = vec![false; width];
            for assignment in &template.assignments {
                if keyed
                    && (!keys.contains(&assignment.column)
                        || matches!(
                            assignment.expression,
                            Expression::Collect { .. }
                                | Expression::Window(_)
                                | Expression::Lookup(_)
                                | Expression::Intermediate { .. }
                        ))
                {
                    return Err(PlanError::InvalidKeyMode);
                }
                validate_assignment(
                    assignment,
                    &mut available,
                    &source,
                    &secondary,
                    &intermediates,
                    &output,
                    &template.mode,
                )?;
            }
            if keyed && keys.iter().any(|&column| !available[column]) {
                return Err(PlanError::InvalidKeyMode);
            }
            if let Some(filter) = &template.filter {
                filter
                    .validate(
                        source.columns().len(),
                        if keyed { &[] } else { &available },
                        matches!(template.mode, RowMode::Groups(_)),
                    )
                    .map_err(PlanError::Filter)?;
            }
            if row_columns
                .as_ref()
                .is_some_and(|previous| previous != &available)
            {
                return Err(PlanError::InconsistentRowColumns);
            }
            row_columns = Some(available.clone());
            for assignment in &columns {
                if keyed && matches!(assignment.expression, Expression::Source(_)) {
                    // A key combination reads all its feeding records, never a chosen first row.
                    return Err(PlanError::InvalidKeyMode);
                }
                if keyed
                    && matches!(&assignment.expression, Expression::Compute(expression) if expression.reads_source())
                {
                    return Err(PlanError::InvalidKeyMode);
                }
                validate_assignment(
                    assignment,
                    &mut available,
                    &source,
                    &secondary,
                    &intermediates,
                    &output,
                    &template.mode,
                )?;
            }
            if available.contains(&false) {
                return Err(PlanError::IncompleteRow);
            }
        }
        for (position, verification) in verifications.iter().enumerate() {
            if verification.path.is_empty() {
                return Err(PlanError::EmptyPath);
            }
            if verifications[..position]
                .iter()
                .any(|previous| previous.path == verification.path)
            {
                return Err(PlanError::DuplicateVerificationPath);
            }
            match &verification.check {
                Check::Assert(predicate) | Check::PredicateDeclaration(predicate) => predicate
                    .validate(0, &vec![true; width], true)
                    .map_err(PlanError::Predicate)?,
                Check::Implies { when, then } => {
                    for predicate in [when, then] {
                        predicate
                            .validate(0, &vec![true; width], true)
                            .map_err(PlanError::Predicate)?;
                    }
                }
                Check::Unique(columns) => {
                    // Unlike row.group_by, unique.columns permits repeated names.
                    if columns.is_empty() || columns.iter().any(|&column| column >= width) {
                        return Err(PlanError::InvalidColumns);
                    }
                }
                Check::RowCount { min, max } => {
                    if (min.is_none() && max.is_none())
                        || matches!((min, max), (Some(a), Some(b)) if a > b)
                    {
                        return Err(PlanError::InvalidBounds);
                    }
                }
            }
        }
        Ok(Self {
            source,
            secondary,
            intermediates,
            output,
            templates,
            columns,
            keys,
            verifications,
            conversion_handlers: Vec::new(),
            conversion_sites: BTreeMap::new(),
        })
    }
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
}

#[derive(Debug, PartialEq)]
pub enum ExecutionError<E> {
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
        ..
    } = state;
    budget.work(1, 1)?;
    let reads = match &assignment.expression {
        Expression::Source(_) => 1,
        Expression::Reduce { .. } | Expression::Collect { .. } => candidate.members.len(),
        _ => 0,
    };
    budget.work(reads, 1)?;
    let value = match &assignment.expression {
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
        Expression::Lookup(lookup) => lookup::read(
            secondary[lookup.source],
            lookup,
            assignment,
            candidate,
            plan,
            row,
            budget,
        )?,
        Expression::Collect { .. } => {
            key_grain::collect(table, assignment, candidate, plan, row, budget, handlers)?
        }
        Expression::Reduce {
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
            Err(error) => {
                let identity = if matches!(&error, TableReductionError::Reduction(yamaa_core::reduction::ReductionError::Arithmetic { error, .. }) if error.phase() == "derivation")
                {
                    failure_identity(candidate, &plan.keys, row, budget)?
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
    let converted = match convert(&value, plan.output.columns()[assignment.column].kind) {
        Ok(value) => value,
        Err(error) => {
            conversion::recover(error, assignment, candidate, plan, row, budget, handlers)?
        }
    };
    budget.value(&converted)?;
    Ok(converted)
}

impl DatasetPlan {
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
        let mut handlers = HandlerCounter::default();
        for declaration in &self.conversion_handlers {
            handlers.register(&declaration.handler.spec_path, HandlerKind::Unconvertible);
        }
        let result = self.execute_inner(table, secondary, limits, &mut handlers);
        ExecutionAttempt {
            result,
            handler_counts: handlers.snapshot().to_vec(),
        }
    }

    /// Share one handler ledger and resource budget for this immutable plan attempt.
    fn execute_inner<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        limits: Limits,
        handlers: &mut HandlerCounter,
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        if table.schema() != &self.source
            || secondary.len() != self.secondary.len()
            || secondary
                .iter()
                .zip(&self.secondary)
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
        let mut budget = Budget::new(limits);
        let mut intermediate_run = intermediates::Run::default();
        let mut candidates: Vec<Candidate> = Vec::new();
        if matches!(self.templates[0].mode, RowMode::Keys) {
            candidates = key_grain::construct(self, table, limits, &mut budget, handlers)?;
        } else {
            for template in &self.templates {
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
                        values: vec![Value::Missing; self.output.columns().len()],
                        completed: vec![false; self.output.columns().len()],
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
                        let truth = filter
                            .evaluate(table, source_row, &candidate.values, budget.predicate())
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
        for assignment in &self.columns {
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
                            secondary,
                            budget: &mut budget,
                            handlers,
                            intermediates: &mut intermediate_run,
                        },
                    )?
                };
                candidate.completed[assignment.column] = true;
            }
        }
        let dataset = Dataset {
            schema: self.output.clone(),
            rows: candidates.into_iter().map(|row| row.values).collect(),
        };
        let mut failures = Vec::new();
        budget.work(dataset.rows.len(), self.keys.len())?;
        for (position, &column) in self.keys.iter().enumerate() {
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
                    offending_rows: identities(&dataset, &self.keys, missing, &mut budget)?,
                });
            }
        }
        budget.work(dataset.rows.len(), self.keys.len())?;
        let groups = partition(&dataset, &self.keys, limits.output_rows, limits.key_cells)
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
                offending_rows: identities(&dataset, &self.keys, duplicates, &mut budget)?,
            });
        }
        if !failures.is_empty() {
            return Err(Box::new(ExecutionError::KeyFailures(failures)));
        }
        let mut records = Vec::new();
        for verification in &self.verifications {
            let record = match &verification.check {
                Check::Assert(_) | Check::Implies { .. } | Check::PredicateDeclaration(_) => {
                    let Some(record) = crate::dataset_verification::predicate_check(
                        verification,
                        &dataset,
                        &self.keys,
                        &mut budget,
                        &mut records,
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
                        offending_rows: identities(
                            &dataset,
                            &self.keys,
                            repeated
                                .into_iter()
                                .flat_map(|members| members.iter().copied()),
                            &mut budget,
                        )?,
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
                        offending_rows: Vec::new(),
                    }
                }
            };
            records.push(record);
        }
        if records.iter().any(|record| record.failed_count != 0) {
            return Err(Box::new(ExecutionError::VerificationFailures(records)));
        }
        Ok(Execution {
            dataset,
            verifications: records,
        })
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
                .checked_mul(self.output.columns().len())
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
