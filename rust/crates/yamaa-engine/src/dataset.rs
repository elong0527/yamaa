//! Closed typed dataset application plan for direct, literal and ordered reductions.
//!
//! This is an internal bridge target, not a specification parser or public backend.
//! Unsupported syntax must be rejected by a compiler before creating this plan.

use crate::{
    table_grouping::{partition, GroupingError},
    table_reduction::{reduce_column, TableReductionError},
};
use alloc::{boxed::Box, string::String, vec, vec::Vec};
use core::convert::Infallible;
use yamaa_core::{
    conversion::{convert, ConversionError},
    reduction::NumericReducer,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::Value,
};

/// Already bound expressions; no implicit joins or lookup fallback are represented.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Literal(Value),
    Source(usize),
    Column(usize),
    Reduce {
        column: usize,
        reducer: NumericReducer,
        text: String,
    },
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
}

/// Assignments are supplied in resolved dependency order, not map iteration order.
#[derive(Clone, Debug, PartialEq)]
pub struct RowTemplate {
    pub mode: RowMode,
    pub assignments: Vec<Assignment>,
}

/// Error-severity dataset checks supported by this closed application slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    Unique(Vec<usize>),
    RowCount { min: Option<i64>, max: Option<i64> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
}

/// Admitted immutable plan: all references and phase dependencies are checked once.
#[derive(Clone, Debug, PartialEq)]
pub struct DatasetPlan {
    source: TableSchema,
    output: TableSchema,
    templates: Vec<RowTemplate>,
    columns: Vec<Assignment>,
    keys: Vec<usize>,
    verifications: Vec<Verification>,
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
        Expression::Literal(_) => {}
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
            if matches!(mode, RowMode::Records) {
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
        if templates.is_empty() {
            return Err(PlanError::NoTemplates);
        }
        let width = output.columns().len();
        validate_columns(&keys, width)?;
        let mut row_columns = None;
        for template in &templates {
            if let RowMode::Groups(keys) = &template.mode {
                validate_columns(keys, source.columns().len())?;
            }
            let mut available = vec![false; width];
            for assignment in &template.assignments {
                validate_assignment(assignment, &mut available, &source, &template.mode)?;
            }
            if row_columns
                .as_ref()
                .is_some_and(|previous| previous != &available)
            {
                return Err(PlanError::InconsistentRowColumns);
            }
            row_columns = Some(available.clone());
            for assignment in &columns {
                validate_assignment(assignment, &mut available, &source, &template.mode)?;
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
            output,
            templates,
            columns,
            keys,
            verifications,
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

#[derive(Debug, PartialEq)]
pub enum ExecutionError<E> {
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
    },
    Conversion {
        path: String,
        output_row: usize,
        error: ConversionError,
    },
    KeyFailures(Vec<CheckRecord>),
    VerificationFailures(Vec<CheckRecord>),
    OutputGrouping(GroupingError<Infallible>),
}

/// Keep source provenance per candidate for the later whole-column phase.
struct Candidate {
    members: Vec<usize>,
    values: Vec<Value>,
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

/// Evaluate one admitted assignment and finish conversion before publishing it.
fn evaluate<T: TableAccess + ?Sized>(
    table: &T,
    assignment: &Assignment,
    candidate: &Candidate,
    output: &TableSchema,
    row: usize,
    limits: Limits,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let value = match &assignment.expression {
        Expression::Literal(value) => value.clone(),
        Expression::Column(column) => candidate.values[*column].clone(),
        Expression::Source(column) => {
            let source_row = candidate.members[0];
            own(table.cell(source_row, *column).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: assignment.path.clone(),
                    source_row,
                    error,
                })
            })?)
        }
        Expression::Reduce {
            column,
            reducer,
            text,
        } => Value::from(
            reduce_column(
                table,
                *column,
                &candidate.members,
                limits.source_rows,
                *reducer,
                text,
            )
            .map_err(|error| {
                Box::new(ExecutionError::Reduction {
                    path: assignment.path.clone(),
                    error,
                })
            })?,
        ),
    };
    convert(&value, output.columns()[assignment.column].kind).map_err(|error| {
        Box::new(ExecutionError::Conversion {
            path: alloc::format!("columns.{}", output.columns()[assignment.column].name),
            output_row: row,
            error,
        })
    })
}

impl DatasetPlan {
    /// Execute only the admitted scope, returning no accepted table on any failure.
    /// Source access errors remain errors. No handlers, callbacks, filters, windows,
    /// joins, key-grain construction, file publication or fallback are implicit.
    pub fn execute<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        limits: Limits,
    ) -> Result<Execution, Box<ExecutionError<T::Error>>> {
        if table.schema() != &self.source {
            return Err(Box::new(ExecutionError::SchemaMismatch));
        }
        if table.row_count() > limits.source_rows {
            return Err(Box::new(ExecutionError::Capacity));
        }
        let mut candidates: Vec<Candidate> = Vec::new();
        for template in &self.templates {
            let groups = match &template.mode {
                RowMode::Records => {
                    // Admit the known cardinality before materializing record memberships.
                    self.check_capacity(candidates.len(), table.row_count(), limits)?;
                    (0..table.row_count()).map(|row| vec![row]).collect()
                }
                RowMode::Groups(keys) => {
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
                };
                for assignment in &template.assignments {
                    candidate.values[assignment.column] = evaluate(
                        table,
                        assignment,
                        &candidate,
                        &self.output,
                        candidates.len(),
                        limits,
                    )?;
                }
                candidates.push(candidate);
            }
        }
        for assignment in &self.columns {
            for (row, candidate) in candidates.iter_mut().enumerate() {
                candidate.values[assignment.column] =
                    evaluate(table, assignment, candidate, &self.output, row, limits)?;
            }
        }
        let dataset = Dataset {
            schema: self.output.clone(),
            rows: candidates.into_iter().map(|row| row.values).collect(),
        };
        let mut failures = Vec::new();
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
                    offending_rows: identities(&dataset, &self.keys, missing),
                });
            }
        }
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
                offending_rows: identities(&dataset, &self.keys, duplicates),
            });
        }
        if !failures.is_empty() {
            return Err(Box::new(ExecutionError::KeyFailures(failures)));
        }
        let mut records = Vec::new();
        for verification in &self.verifications {
            let record = match &verification.check {
                Check::Unique(columns) => {
                    // Repeated references do not change tuple equality. Keep first
                    // declaration order while using the strict grouping primitive.
                    let mut distinct = Vec::new();
                    for &column in columns {
                        if !distinct.contains(&column) {
                            distinct.push(column);
                        }
                    }
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
                        ),
                    }
                }
                Check::RowCount { min, max } => {
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
fn identities(
    dataset: &Dataset,
    keys: &[usize],
    rows: impl IntoIterator<Item = usize>,
) -> Vec<RowIdentity> {
    rows.into_iter()
        .map(|position| RowIdentity {
            position,
            values: keys
                .iter()
                .map(|&column| dataset.rows[position][column].clone())
                .collect(),
        })
        .collect()
}
