//! The outcome of executing a dataset plan: the dataset, the evidence an
//! attempt retains on success and failure, and the failures it reports. The
//! plan runs over its resolved form (`dataset/resolved`), column at a time
//! over typed buffers (`dataset/columnar`).

mod diagnostics;

use super::*;
use crate::function_invocation::FunctionPort;
use crate::{
    conversion::ConversionError,
    numeric_lifecycle::{HandlerCount, HandlerCountOverflow},
    reduction::NumericReducer,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    table_grouping::GroupingError,
    table_reduction::TableReductionError,
    value::{Value, ValueType},
};
use std::convert::Infallible;

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
    /// Complete owned rows in their final order.
    pub(crate) fn from_rows(schema: TableSchema, rows: Vec<Vec<Value>>) -> Self {
        Self { schema, rows }
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
    /// The value-level entry identifiers each offending row matches, aligned
    /// with `offending_rows`, when two entries of one column select a row.
    pub entries: Option<Vec<Vec<String>>>,
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

/// One completed row template: the rows it emitted and the driver records it
/// read, counted where its candidates become final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateRows {
    pub emitted: usize,
    pub read: usize,
}

/// An attempted run retains handler evidence even when no dataset can be accepted.
#[derive(Debug, PartialEq)]
pub struct ExecutionAttempt<E> {
    pub result: Result<Execution, Box<ExecutionError<E>>>,
    pub handler_counts: Vec<HandlerCount>,
    /// Completed checks preceding an unrelated failure, outside its diagnostic.
    pub retained_verifications: Vec<CheckRecord>,
    /// Row templates completed in construction order; a failed construction
    /// retains the templates that finished before it.
    pub template_rows: Vec<TemplateRows>,
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
        error: crate::expression::EvaluationError<Infallible>,
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
    /// A lookup found no record under `require_match` (LOOKUP-24).
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
        error: crate::expression::EvaluationError<CellError<Infallible>>,
        records: Vec<CheckRecord>,
    },
    Predicate {
        source_row: usize,
        error: crate::expression::EvaluationError<CellError<E>>,
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

impl<E: std::fmt::Debug> std::fmt::Display for ExecutionError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "dataset execution: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for ExecutionError<E> {}

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
        match self.resolve() {
            Ok(resolved) => resolved.execute(&Borrowed(table), secondary, functions, observer),
            // Binding resolves every plan it accepts, so a failure here is a
            // compiler or resolver defect: loud in debug builds, refused as
            // a schema mismatch otherwise.
            Err(error) => {
                if cfg!(debug_assertions) {
                    panic!("an unresolvable plan reached execution: {error}");
                }
                ExecutionAttempt {
                    result: Err(Box::new(ExecutionError::SchemaMismatch)),
                    handler_counts: Vec::new(),
                    retained_verifications: Vec::new(),
                    template_rows: Vec::new(),
                }
            }
        }
    }
}

/// A sized view of a table port.
struct Borrowed<'t, T: ?Sized>(&'t T);
impl<T: TableAccess + ?Sized> TableAccess for Borrowed<'_, T> {
    type Error = T::Error;
    fn schema(&self) -> &TableSchema {
        self.0.schema()
    }
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<T::Error>> {
        self.0.cell(row, column)
    }
}
