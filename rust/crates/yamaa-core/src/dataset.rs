//! Immutable admitted dataset model, independent of execution and host effects.
pub use crate::bound_expression::BoundNumeric;
use crate::bound_expression::Read;
use crate::{
    bound_expression::{BindingError, BoundPredicate},
    reduction::NumericReducer,
    table::TableSchema,
    value::Value,
};
use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
#[path = "dataset_compiler.rs"]
mod compiler;
pub use compiler::{
    compile_dataset, validate_window_form, AssignmentDeclaration, CompiledDataset,
    DatasetCompileError, DatasetCompileLimits, DatasetCompiler, DatasetDeclaration,
    DatasetMetadata, ExpressionDeclaration, TemplateDeclaration, WindowDeclarationKind,
};
#[path = "dataset_conversion.rs"]
mod conversion;
pub use conversion::ConversionHandler;
#[path = "dataset_count.rs"]
mod count;
pub use count::{CountBoundError, RowCount, RowCountBound, MAX_COUNT_BOUND_BYTES};
#[path = "dataset_flag.rs"]
mod flags;
pub use flags::{
    flag_condition_diagnostic, Flag, FlagBinding, FlagError, FlagRead, MAX_FLAG_METADATA_BYTES,
};
#[path = "dataset_functions.rs"]
mod functions;
pub use functions::{BoundFunction, BoundProjectFunction, FunctionArgument, FunctionInput};
#[path = "dataset_aggregate.rs"]
mod aggregate;
pub use aggregate::{
    AggregateBinding, AggregateBindingError, AggregateDerivation, AggregatePhase,
    AggregateRelation, AggregateRowContext, AggregateSource, BoundAggregate,
};
#[path = "dataset_odm.rs"]
pub(crate) mod odm;
pub use odm::{BoundOdm, OdmRead, OdmScope, OdmTemplateScope};
#[path = "dataset_intermediates.rs"]
mod intermediates;
pub use intermediates::{
    visit_output_dependencies, Intermediate, IntermediateDerivations, IntermediateUnique,
    RecordMatchKey, SourceSchemas,
};
#[path = "dataset_relation_predicate.rs"]
pub(crate) mod relation_predicates;
pub use relation_predicates::{CurrentField, CurrentRead, RelationPredicate};
#[path = "dataset_relation_shape.rs"]
mod relation_shape;
pub use relation_shape::{
    RelationField, RelationFieldOrigin, RelationShape, RelationShapeError, RelationShapeLimits,
};
#[path = "dataset_relation_range.rs"]
mod relation_range;
pub use relation_range::{
    RelationRange, RelationRangeBound, RelationRangeError, RelationRangeFailure, RelationRangeTypes,
};
#[path = "dataset_self.rs"]
mod self_relations;
pub use self_relations::IntermediateSource;
#[path = "dataset_match_values.rs"]
mod match_values;
pub use match_values::{ContextualMatch, IntermediateKey, MatchRead, MatchValue};
#[path = "dataset_joined.rs"]
mod joined;
#[path = "dataset_lookup.rs"]
mod lookup;
pub use joined::JoinedRead;
#[path = "dataset_rounding.rs"]
mod rounding;
pub use rounding::{InvalidDigits, Rounding};
#[path = "dataset_temporal_imputation.rs"]
mod imputation;
pub use imputation::{
    date_policy, DatePolicyError, DayDeclaration, DayRule, Imputation, ImputationKind,
    ImputationStep, PartialDate, TimeRule, MAX_IMPUTATION_POLICY_BYTES,
};
#[path = "dataset_temporal.rs"]
mod temporal;
pub use crate::text_case::StringCase;
pub use crate::text_extract::StringExtract;
pub use crate::text_pad::StringPad;
pub use crate::text_template::StringTemplate;
pub use temporal::{CalendarUnit, DateBounds, InvalidTemporal, Temporal, TemporalKind};
#[path = "dataset_selection.rs"]
mod selection;
pub use selection::{FirstAvailable, SelectionRead, SelectionSource};
#[path = "dataset_row_stages.rs"]
mod row_stages;
pub use row_stages::window_stages;
#[path = "dataset_row_drivers.rs"]
mod row_drivers;
#[path = "dataset_windows.rs"]
mod windows;
pub use windows::{ContextWindow, OrderTerm, Window, WindowKind};

/// Already bound expressions; no host joins or lookup fallback are implicit.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Joined(alloc::boxed::Box<JoinedRead>),
    Odm(alloc::boxed::Box<BoundOdm>),
    Aggregate(alloc::boxed::Box<BoundAggregate>),
    MappingCut {
        source: alloc::boxed::Box<Assignment>,
        operation: crate::mapping_cut::MappingCut,
    },
    Extreme {
        operands: Vec<Assignment>,
        largest: bool,
    },
    StrConcat {
        operands: Vec<Assignment>,
        missing: Option<Value>,
        path: String,
    },
    StrContains {
        source: alloc::boxed::Box<Assignment>,
        operation: StringExtract,
    },
    StrTemplate {
        operands: Vec<Assignment>,
        operation: StringTemplate,
    },
    StrExtract {
        source: alloc::boxed::Box<Assignment>,
        operation: StringExtract,
    },
    StrCase {
        source: alloc::boxed::Box<Assignment>,
        operation: StringCase,
    },
    StrPad {
        source: alloc::boxed::Box<Assignment>,
        operation: StringPad,
    },
    /// Only the first TRUE branch is evaluated; FALSE and UNKNOWN continue.
    Case(Vec<CaseBranch>),
    Literal(Value),
    /// Binding handler ownership is retained even when the variable exists.
    SourceBinding {
        source: alloc::boxed::Box<Assignment>,
        absent: Option<Value>,
        variable_absent: bool,
        variable: String,
        path: String,
    },
    /// Source-only eligibility and stable choice on an implicit secondary join.
    SourceLookup {
        lookup: Lookup,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
    },
    /// A qualified source keeps its filter and stable choice on its declared port.
    JoinedSource {
        read: alloc::boxed::Box<JoinedRead>,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
    },
    /// One condition selects a raw true/false/UNKNOWN literal.
    Flag(Flag),
    /// Compiled scalar arithmetic over statically bound source/completed output reads.
    Compute(BoundNumeric),
    /// Declarative rounding over one bound operand; no formula text is invented.
    Round {
        source: alloc::boxed::Box<Assignment>,
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
    /// Explicit prebound host invocation; never discovered or activated during execution.
    Function(BoundFunction),
    /// Versionless package invocation, activated before any study read.
    ProjectFunction(BoundProjectFunction),
    /// Windows over completed output rows in their column or row-template context.
    Window(Window),
    ContextWindow(ContextWindow),
    /// First present raw operand in authored order, followed by one output conversion.
    FirstAvailable(alloc::boxed::Box<FirstAvailable>),
    Source(usize),
    /// Read one record from a secondary source on completed output match values.
    Lookup(Lookup),
    /// Equality against raw driver fields, never unfinished outputs.
    /// Runs in its owning assignment's phase; see [`RowLookup`].
    RowLookup(RowLookup),
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
        /// Authored operand identity when supplied by the specification compiler.
        /// Typed plans without an authored name use the bound source field name.
        identifier: Option<String>,
        column: usize,
        reducer: NumericReducer,
        text: String,
    },
    /// Count grouped records (None) or present field values (Some), without coercion.
    Count {
        column: Option<usize>,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaseBranch {
    pub when: Option<BoundPredicate>,
    pub assignment: alloc::boxed::Box<Assignment>,
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

/// Equality pairs compare a secondary field with a raw driver field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowMatchKey {
    pub source_column: usize,
    pub driver_column: usize,
}

/// Secondary read on raw record fields or available grouping fields.
///
/// A template assignment runs before that template's filter, so a duplicate
/// match fails even if the filter would discard the candidate. A whole-column
/// assignment runs after filtering and only reads retained candidates. Both
/// phases use the candidate's original driver membership, not its output keys.
/// Key-grain plans reject this expression in either phase. The normalized Python
/// frontend currently admits this expression only in template assignments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowLookup {
    pub source: usize,
    pub column: usize,
    pub keys: Vec<RowMatchKey>,
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
        mode: alloc::boxed::Box<RowMode>,
    },
    /// A stored secondary source with one bounded record/group construction mode.
    Driver {
        source: usize,
        mode: alloc::boxed::Box<RowMode>,
    },
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
    /// Presence of the owning completed column, before later derivations.
    NotMissing,
    /// Already converted, present permitted values for the owning column.
    AllowedValues(Vec<Value>),
    /// Fixed environment terminology at the owning column's checkpoint.
    /// Numeric codes retain their original integer/binary64 representation.
    Codelist {
        id: String,
        values: Vec<Value>,
    },
    /// Several inherited/value-level bindings share one declaration path.
    /// Selection uses the completed discriminator, never a donor row or template.
    ValueCodelists {
        discriminator: usize,
        overrides: Vec<String>,
        bindings: Vec<ValueCodelist>,
    },
    Inventory {
        values: Vec<Value>,
        scope: Option<InventoryScope>,
    },
    /// Effective per-value metadata length, selected by its completed owner.
    ValueMaxLength {
        max: RowCountBound,
        discriminator: usize,
        codes: Vec<String>,
        include: bool,
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
    Matches(crate::regex::Pattern),
    /// A core-owned declaration finding deferred until dataset verification.
    InvalidDiagnostic(crate::diagnostic::Diagnostic),
    /// A compiler finding evaluated in declaration order after output keys.
    InvalidDeclaration {
        code: crate::diagnostic::ConditionCode,
        reason: String,
    },
    /// `require` must hold on every row that `when`, if present, binds (REQ-0383).
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

#[derive(Clone, Debug, PartialEq)]
pub struct InventoryScope {
    pub discriminator: usize,
    pub default: bool,
    pub overrides: Vec<(String, bool)>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ValueCodelist {
    pub id: String,
    pub values: Vec<Value>,
    pub codes: Vec<String>,
    pub default: bool,
}

impl Check {
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
        use core::cmp::Ordering;
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

    pub fn row_count_work(&self) -> usize {
        match self {
            Self::WideRowCount { min, max } => min
                .iter()
                .chain(max)
                .map(|bound| bound.decimal().len())
                .sum::<usize>()
                .max(1),
            _ => 1,
        }
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
impl VerificationSeverity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// Checks for one declared column, in authored order and its own ID namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnVerifications {
    pub column: usize,
    pub checks: Vec<Verification>,
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
    InvalidBounds,
    DuplicateVerificationPath,
    Filter(BindingError),
    Predicate(BindingError),
    InvalidKeyMode,
    InvalidWindow,
    InvalidSourceOrder,
    InvalidOutputOrder,
    InvalidLookup,
    InvalidIntermediate,
    InvalidConversionHandler,
    InvalidFunction,
}

/// Admitted immutable plan: all references and phase dependencies are checked once.
#[derive(Clone, Debug, PartialEq)]
pub struct DatasetPlan {
    source: TableSchema,
    secondary: Vec<SecondarySource>,
    intermediates: Vec<Intermediate>,
    output: TableSchema,
    templates: Vec<RowTemplate>,
    row_contexts: Vec<Option<AggregateRowContext>>,
    columns: Vec<Assignment>,
    keys: Vec<usize>,
    verifications: Vec<Verification>,
    column_verifications: Vec<ColumnVerifications>,
    warning_verifications: alloc::collections::BTreeSet<String>,
    conversion_handlers: Vec<ConversionHandler>,
    conversion_sites: BTreeMap<String, usize>,
    output_order: Vec<OrderTerm>,
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
#[allow(clippy::too_many_arguments)]
const EXPRESSION_DEPTH_LIMIT: usize = 64;
const EXPRESSION_NODE_LIMIT: usize = 65_536;

#[allow(clippy::too_many_arguments)]
fn validate_function_intermediates(
    arguments: &[FunctionArgument],
    source: &TableSchema,
    secondary: &[SecondarySource],
    items: &[Intermediate],
    available: &[bool],
    output: &TableSchema,
    mode: &RowMode,
) -> Result<(), PlanError> {
    for argument in arguments {
        if let FunctionInput::Read(Read::Intermediate { index, column }) = argument.input {
            let item = items.get(index).ok_or(PlanError::InvalidIntermediate)?;
            if item.selection.is_none() {
                return Err(PlanError::InvalidIntermediate);
            }
            intermediates::validate_context_read(
                item, column, source, secondary, available, output, mode,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_assignment(
    assignment: &Assignment,
    available: &mut [bool],
    source: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Intermediate],
    output: &TableSchema,
    mode: &RowMode,
    record_derivation: bool,
) -> Result<(), PlanError> {
    let mut remaining = EXPRESSION_NODE_LIMIT;
    validate_assignment_at(
        assignment,
        available,
        source,
        source,
        secondary,
        intermediates,
        output,
        mode,
        0,
        &mut remaining,
        record_derivation,
    )
}
/// Donor reads and named current-row reads use separate admitted schemas.
#[allow(clippy::too_many_arguments)]
fn validate_record_assignment(
    assignment: &Assignment,
    available: &mut [bool],
    donor: &TableSchema,
    primary: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Intermediate],
    output: &TableSchema,
) -> Result<(), PlanError> {
    let mut remaining = EXPRESSION_NODE_LIMIT;
    validate_assignment_at(
        assignment,
        available,
        donor,
        primary,
        secondary,
        intermediates,
        output,
        &RowMode::Records,
        0,
        &mut remaining,
        false,
    )
}
/// The augmented primary stores completed fields in output coordinates and
/// appends immutable correlated operands after them. A raw source binding cannot
/// use a later output coordinate just because the merged source schema contains it.
fn validate_donor_reads(
    expression: &Expression,
    available: &[bool],
    intermediates: &[Intermediate],
) -> Result<(), PlanError> {
    let column = |column: usize| {
        if column < available.len() && !available[column] {
            Err(PlanError::UnavailableColumn)
        } else {
            Ok(())
        }
    };
    let read = |read: &Read| match read {
        Read::Source(index) => column(*index),
        Read::Intermediate { index, .. } => {
            let item = intermediates
                .get(*index)
                .ok_or(PlanError::InvalidIntermediate)?;
            for key in &item.keys {
                if let Some(MatchRead::Record { column: index, .. }) = key.value.read() {
                    column(*index)?;
                }
            }
            for field in item.current_fields() {
                if let CurrentRead::Record {
                    source: 0,
                    column: index,
                } = field.read
                {
                    column(index)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    };
    match expression {
        Expression::Source(index)
        | Expression::Collect { column: index, .. }
        | Expression::Reduce { column: index, .. } => column(*index)?,
        Expression::Count {
            column: Some(index),
            ..
        } => column(*index)?,
        Expression::Intermediate { index, column } => read(&Read::Intermediate {
            index: *index,
            column: *column,
        })?,
        Expression::Compute(expression) => {
            for binding in expression.bindings() {
                read(&binding.read)?;
            }
        }
        Expression::Flag(flag) => {
            for binding in flag.bindings() {
                match &binding.read {
                    FlagRead::Read(value) => read(value)?,
                    FlagRead::Collect { column: index, .. } => column(*index)?,
                }
            }
        }
        Expression::Function(function) => {
            for argument in function.arguments() {
                if let FunctionInput::Read(value) = &argument.input {
                    read(value)?;
                }
            }
        }
        Expression::ProjectFunction(function) => {
            for argument in function.arguments() {
                if let FunctionInput::Read(value) = &argument.input {
                    read(value)?;
                }
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                if let Some(predicate) = &branch.when {
                    for binding in predicate.bindings() {
                        read(&binding.read)?;
                    }
                }
            }
        }
        Expression::ContextWindow(window) => {
            for (_, index) in &window.sources {
                column(*index)?;
            }
        }
        Expression::FirstAvailable(selection) => {
            for operand in selection.sources() {
                match &operand.read {
                    SelectionRead::Source(index) | SelectionRead::Collect { column: index, .. } => {
                        column(*index)?
                    }
                    _ => (),
                }
            }
        }
        Expression::RowLookup(lookup) => {
            for key in &lookup.keys {
                column(key.driver_column)?;
            }
        }
        Expression::Aggregate(aggregate)
            if matches!(aggregate.relation(), AggregateRelation::CurrentGroup) =>
        {
            for binding in aggregate.bindings() {
                column(binding.column)?;
            }
        }
        _ => (),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_assignment_at(
    assignment: &Assignment,
    available: &mut [bool],
    source: &TableSchema,
    primary: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Intermediate],
    output: &TableSchema,
    mode: &RowMode,
    depth: usize,
    remaining: &mut usize,
    record_derivation: bool,
) -> Result<(), PlanError> {
    if depth > EXPRESSION_DEPTH_LIMIT || *remaining == 0 {
        return Err(PlanError::InvalidColumns);
    }
    *remaining -= 1;
    if assignment.path.is_empty() {
        return Err(PlanError::EmptyPath);
    }
    if assignment.column >= available.len() {
        return Err(PlanError::InvalidColumns);
    }
    if available[assignment.column] {
        return Err(PlanError::DuplicateAssignment);
    }
    if record_derivation {
        validate_donor_reads(&assignment.expression, available, intermediates)?;
    }
    match &assignment.expression {
        Expression::Joined(read) => read.validate(source, secondary, output, available)?,
        Expression::JoinedSource {
            read,
            filter,
            selection,
        } => {
            read.validate(source, secondary, output, available)?;
            let width = read.schema.columns().len();
            if let Some(filter) = filter {
                filter
                    .validate(width, &[], false)
                    .map_err(PlanError::Filter)?;
            }
            if selection.as_ref().is_some_and(|s| {
                s.order_by.is_empty() || s.order_by.iter().any(|term| term.column >= width)
            }) {
                return Err(PlanError::InvalidSourceOrder);
            }
        }
        Expression::Case(branches) => {
            if branches.is_empty() || branches.len() > 1024 || branches[0].when.is_none() {
                return Err(PlanError::InvalidColumns);
            }
            for (index, branch) in branches.iter().enumerate() {
                if branch.assignment.column != assignment.column
                    || branch.when.is_none() && index + 1 != branches.len()
                {
                    return Err(PlanError::InvalidColumns);
                }
                if let Some(when) = &branch.when {
                    if !when.collected().is_empty()
                        && !matches!(mode, RowMode::Keys | RowMode::Records)
                    {
                        return Err(PlanError::InvalidKeyMode);
                    }
                    if let RowMode::Groups(keys) = mode {
                        if when.bindings().iter().any(|binding| matches!(binding.read, Read::Source(column) if !keys.contains(&column))) {
                            return Err(PlanError::NonGroupSource);
                        }
                    }
                    when.validate_contextual(source.columns().len(), available)
                        .map_err(PlanError::Filter)?;
                    for binding in when.bindings() {
                        if let Read::Intermediate { index, column } = binding.read {
                            let item = intermediates
                                .get(index)
                                .ok_or(PlanError::InvalidIntermediate)?;
                            intermediates::validate_context_read(
                                item, column, primary, secondary, available, output, mode,
                            )?;
                        }
                    }
                }
                validate_assignment_at(
                    &branch.assignment,
                    available,
                    source,
                    primary,
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
                    record_derivation,
                )?;
                available[assignment.column] = false;
            }
        }
        // REQ-0211/0213 convert one derived value at execution, including
        // literals. Eager conversion would invent failures for empty templates
        // and move runtime conversion conditions into the planning phase.
        Expression::Literal(_) => {}
        Expression::Flag(flag) => {
            flag.validate(source, available, secondary, intermediates, output, mode)?
        }
        Expression::Compute(expression) => {
            for binding in expression.bindings() {
                if let crate::bound_expression::Read::Intermediate { index, column } = binding.read
                {
                    let item = intermediates
                        .get(index)
                        .ok_or(PlanError::InvalidIntermediate)?;
                    intermediates::validate_context_read(
                        item, column, primary, secondary, available, output, mode,
                    )?;
                }
            }
            if let RowMode::Groups(keys) = mode {
                if expression.bindings().iter().any(|binding| matches!(binding.read, Read::Source(column) if !keys.contains(&column))) {
                    return Err(PlanError::NonGroupSource);
                }
            }
            expression
                .validate(source.columns().len(), available, false)
                .map_err(|error| match error {
                    crate::bound_expression::ScopeError::GroupedSource => PlanError::NonGroupSource,
                    crate::bound_expression::ScopeError::InvalidSource => PlanError::InvalidSource,
                    crate::bound_expression::ScopeError::UnavailableColumn => {
                        PlanError::UnavailableColumn
                    }
                })?;
        }
        Expression::Round {
            source: operand,
            rounding,
        } => {
            if rounding.source().is_empty() || rounding.path().is_empty() {
                return Err(PlanError::EmptyPath);
            }
            if operand.column != assignment.column {
                return Err(PlanError::InvalidColumns);
            }
            validate_assignment_at(
                operand,
                available,
                source,
                primary,
                secondary,
                intermediates,
                output,
                mode,
                depth + 1,
                remaining,
                record_derivation,
            )?;
            available[assignment.column] = false;
        }
        Expression::Imputation {
            operands,
            operation,
        } => {
            if operands.len() != 1 + usize::from(operation.bound_source().is_some()) {
                return Err(PlanError::InvalidColumns);
            }
            for operand in operands {
                if operand.column != assignment.column {
                    return Err(PlanError::InvalidColumns);
                }
                validate_assignment_at(
                    operand,
                    available,
                    source,
                    primary,
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
                    record_derivation,
                )?;
                available[assignment.column] = false;
            }
        }
        Expression::MappingCut {
            source: operand, ..
        }
        | Expression::StrCase {
            source: operand, ..
        }
        | Expression::StrPad {
            source: operand, ..
        }
        | Expression::StrExtract {
            source: operand, ..
        }
        | Expression::StrContains {
            source: operand, ..
        } => {
            if operand.column != assignment.column {
                return Err(PlanError::InvalidColumns);
            }
            validate_assignment_at(
                operand,
                available,
                source,
                primary,
                secondary,
                intermediates,
                output,
                mode,
                depth + 1,
                remaining,
                record_derivation,
            )?;
            available[assignment.column] = false;
        }
        Expression::Extreme { operands, .. } | Expression::StrConcat { operands, .. } => {
            for operand in operands {
                if operand.column != assignment.column {
                    return Err(PlanError::InvalidColumns);
                }
                validate_assignment_at(
                    operand,
                    available,
                    source,
                    primary,
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
                    record_derivation,
                )?;
                available[assignment.column] = false;
            }
        }
        Expression::StrTemplate {
            operands,
            operation,
        } => {
            if operands.len() != operation.identifiers().len() {
                return Err(PlanError::InvalidColumns);
            }
            for operand in operands {
                if operand.column != assignment.column {
                    return Err(PlanError::InvalidColumns);
                }
                validate_assignment_at(
                    operand,
                    available,
                    source,
                    primary,
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
                    record_derivation,
                )?;
                available[assignment.column] = false;
            }
        }
        Expression::Temporal {
            operands,
            operation,
        } => {
            if operands.len() != operation.sources().len() {
                return Err(PlanError::InvalidColumns);
            }
            for operand in operands {
                if operand.column != assignment.column {
                    return Err(PlanError::InvalidColumns);
                }
                validate_assignment_at(
                    operand,
                    available,
                    source,
                    primary,
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
                    record_derivation,
                )?;
                available[assignment.column] = false;
            }
        }
        Expression::Function(function) => {
            function.validate(source, secondary, output, available, mode)?;
            validate_function_intermediates(
                function.arguments(),
                primary,
                secondary,
                intermediates,
                available,
                output,
                mode,
            )?;
        }
        Expression::ProjectFunction(function) => {
            function.validate(source, secondary, output, available, mode)?;
            validate_function_intermediates(
                function.arguments(),
                primary,
                secondary,
                intermediates,
                available,
                output,
                mode,
            )?;
        }
        Expression::FirstAvailable(selection) => {
            selection.validate(source, available, mode)?;
            for operand in selection.sources() {
                if let SelectionRead::Expression(operand) = &operand.read {
                    if operand.column != assignment.column {
                        return Err(PlanError::InvalidColumns);
                    }
                    validate_assignment_at(
                        operand,
                        available,
                        source,
                        primary,
                        secondary,
                        intermediates,
                        output,
                        mode,
                        depth + 1,
                        remaining,
                        record_derivation,
                    )?;
                    available[assignment.column] = false;
                }
            }
        }
        Expression::SourceBinding {
            source: operand,
            path,
            variable,
            absent,
            variable_absent,
        } => {
            if operand.column != assignment.column
                || path.is_empty()
                || variable.is_empty()
                || (*variable_absent && absent.is_none())
            {
                return Err(PlanError::InvalidColumns);
            }
            validate_assignment_at(
                operand,
                available,
                source,
                primary,
                secondary,
                intermediates,
                output,
                mode,
                depth + 1,
                remaining,
                record_derivation,
            )?;
            available[assignment.column] = false;
        }
        Expression::SourceLookup {
            lookup: bound,
            filter,
            selection,
        } => {
            // Implicit source joins use completed output keys in every row
            // mode. Their ownership and availability are admitted below; they
            // do not borrow a raw record from the enclosing driver.
            lookup::validate_source(bound, secondary, available, output)?;
            let width = secondary[bound.source].schema.columns().len();
            if let Some(filter) = filter {
                filter
                    .validate(width, &[], false)
                    .map_err(PlanError::Filter)?;
            }
            if selection.as_ref().is_some_and(|s| {
                s.order_by.is_empty() || s.order_by.iter().any(|term| term.column >= width)
            }) {
                return Err(PlanError::InvalidSourceOrder);
            }
        }
        Expression::Intermediate { index, column } => {
            let item = intermediates
                .get(*index)
                .ok_or(PlanError::InvalidIntermediate)?;
            intermediates::validate_context_read(
                item, *column, primary, secondary, available, output, mode,
            )?;
        }
        Expression::RowLookup(lookup) => lookup::validate_row(lookup, source, secondary, mode)?,
        Expression::Lookup(lookup) => {
            if !matches!(mode, RowMode::Keys) {
                return Err(PlanError::InvalidLookup);
            }
            lookup::validate(lookup, secondary, available, output)?;
        }
        Expression::ContextWindow(window) => {
            window.validate(available, output, source, mode)?;
        }
        Expression::Window(window) => {
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
            let scalar_records = matches!(mode, RowMode::Records)
                || matches!(mode, RowMode::Groups(keys) if keys.contains(column));
            if (!matches!(mode, RowMode::Keys) && !scalar_records) || identifier.is_empty() {
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
        Expression::Aggregate(aggregate) => {
            aggregate.validate(source, secondary, intermediates, output, available, mode)?;
        }
        Expression::Odm(odm) => odm.validate(source, secondary, mode)?,
        Expression::Count { column, text } => {
            if column.is_some_and(|column| column >= source.columns().len()) {
                return Err(PlanError::InvalidSource);
            }
            if !matches!(mode, RowMode::Groups(_)) {
                return Err(PlanError::UngroupedReduction);
            }
            if text.is_empty() {
                return Err(PlanError::EmptyPath);
            }
        }
        Expression::Reduce {
            column,
            text,
            identifier,
            ..
        } => {
            if *column >= source.columns().len() {
                return Err(PlanError::InvalidSource);
            }
            if !matches!(mode, RowMode::Groups(_)) {
                return Err(PlanError::UngroupedReduction);
            }
            if text.is_empty() || identifier.as_ref().is_some_and(String::is_empty) {
                return Err(PlanError::EmptyPath);
            }
        }
    }
    match &assignment.expression {
        Expression::Compute(numeric) => {
            for binding in numeric.bindings() {
                if let Read::Joined(read) = &binding.read {
                    read.validate(source, secondary, output, available)?;
                }
            }
        }
        Expression::Flag(flag) => {
            for binding in flag.bindings() {
                if let FlagRead::Read(Read::Joined(read)) = &binding.read {
                    read.validate(source, secondary, output, available)?;
                }
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                if let Some(when) = &branch.when {
                    for binding in when.bindings() {
                        if let Read::Joined(read) = &binding.read {
                            read.validate(source, secondary, output, available)?;
                        }
                    }
                }
            }
        }
        _ => (),
    }
    available[assignment.column] = true;
    Ok(())
}

// Recursively preserve key-phase authority through selected case branches.
fn key_expression(expression: &Expression) -> bool {
    match expression {
        Expression::Literal(_) | Expression::Source(_) | Expression::Column(_) => true,
        Expression::Extreme { operands, .. } | Expression::StrConcat { operands, .. } => operands
            .iter()
            .all(|source| key_expression(&source.expression)),
        Expression::StrContains { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::Round { source, .. } => key_expression(&source.expression),
        Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => operands
            .iter()
            .all(|source| key_expression(&source.expression)),
        Expression::Flag(flag) => flag.key_expression(),
        Expression::Compute(expression) => expression
            .bindings()
            .iter()
            .all(|binding| !matches!(binding.read, Read::Intermediate { .. })),
        Expression::Case(branches) => branches.iter().all(|branch| {
            branch.when.as_ref().is_none_or(|when| {
                when.collected().is_empty()
                    && when
                        .bindings()
                        .iter()
                        .all(|binding| !matches!(binding.read, Read::Intermediate { .. }))
            }) && key_expression(&branch.assignment.expression)
        }),
        _ => false,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordReadScope {
    Driver,
    PrimaryMatching,
    All,
}
fn function_record_read(
    arguments: &[FunctionArgument],
    intermediates: &[Intermediate],
    scope: RecordReadScope,
) -> bool {
    arguments.iter().any(|argument| match argument.input {
        FunctionInput::Read(Read::Source(_)) => scope != RecordReadScope::PrimaryMatching,
        FunctionInput::Read(Read::Intermediate { index, column }) => current_record_read(
            &Expression::Intermediate { index, column },
            intermediates,
            scope,
        ),
        _ => false,
    })
}
fn current_record_read(
    expression: &Expression,
    intermediates: &[Intermediate],
    scope: RecordReadScope,
) -> bool {
    let include_raw = scope != RecordReadScope::PrimaryMatching;
    match expression {
        Expression::Odm(odm) => {
            include_raw && !matches!(odm.read().scope, OdmScope::Keys | OdmScope::Templates(_))
        }
        Expression::Aggregate(aggregate) => match aggregate.relation() {
            AggregateRelation::CurrentGroup => include_raw,
            AggregateRelation::Matched { keys, .. } => {
                scope != RecordReadScope::Driver
                    && (keys
                        .iter()
                        .filter_map(|key| key.value.read())
                        .any(|read| matches!(read, MatchRead::Record { .. }))
                        || aggregate.range().is_some_and(|range| {
                            matches!(range.current().read, CurrentRead::Record { .. })
                        })
                        || aggregate.derivations().iter().any(|binding| {
                            current_record_read(
                                &binding.assignment.expression,
                                intermediates,
                                RecordReadScope::PrimaryMatching,
                            )
                        }))
            }
            AggregateRelation::Output { .. } => false,
        },
        Expression::Joined(_) | Expression::JoinedSource { .. } => false,
        Expression::Source(_) | Expression::RowLookup(_) => include_raw,
        // A key-grain collection is valid in the primary implicit form. A
        // global column shared by foreign drivers cannot borrow its coordinates.
        Expression::Collect { .. } => scope == RecordReadScope::All,
        Expression::Intermediate { index, .. } => {
            scope != RecordReadScope::Driver
                && intermediates.get(*index).is_some_and(|item| {
                    item.keys
                        .iter()
                        .filter_map(|key| key.value.read())
                        .any(|read| matches!(read, MatchRead::Record { .. }))
                        || item
                            .current_fields()
                            .any(|field| matches!(field.read, CurrentRead::Record { .. }))
                })
        }
        Expression::SourceBinding { source, .. } => {
            current_record_read(&source.expression, intermediates, scope)
        }
        Expression::FirstAvailable(selection) => {
            selection.sources().iter().any(|source| match &source.read {
                SelectionRead::Source(_) => include_raw,
                SelectionRead::Expression(source) => {
                    current_record_read(&source.expression, intermediates, scope)
                }
                SelectionRead::Column(_) | SelectionRead::Collect { .. } => false,
            })
        }
        Expression::Extreme { operands, .. } | Expression::StrConcat { operands, .. } => operands
            .iter()
            .any(|source| current_record_read(&source.expression, intermediates, scope)),
        Expression::StrContains { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::Round { source, .. } => {
            current_record_read(&source.expression, intermediates, scope)
        }
        Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => operands
            .iter()
            .any(|source| current_record_read(&source.expression, intermediates, scope)),
        Expression::Compute(expression) => include_raw && expression.reads_source(),
        Expression::Flag(flag) => include_raw && flag.reads_source(),
        Expression::Function(function) => {
            function_record_read(function.arguments(), intermediates, scope)
        }
        Expression::ProjectFunction(function) => {
            function_record_read(function.arguments(), intermediates, scope)
        }
        Expression::Case(branches) => branches.iter().any(|branch| {
            branch.when.as_ref().is_some_and(|predicate| {
                predicate
                    .bindings()
                    .iter()
                    .any(|binding| match binding.read {
                        Read::Source(_) => {
                            if predicate.collects(&binding.name) {
                                scope == RecordReadScope::All
                            } else {
                                include_raw
                            }
                        }
                        Read::Intermediate { index, column } => current_record_read(
                            &Expression::Intermediate { index, column },
                            intermediates,
                            scope,
                        ),
                        Read::Column(_) | Read::Joined(_) => false,
                    })
            }) || current_record_read(&branch.assignment.expression, intermediates, scope)
        }),
        _ => false,
    }
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
        Self::new_implementation(
            sources,
            intermediates,
            output,
            templates,
            columns,
            keys,
            verifications,
            true,
        )
    }
    /// Donor evaluation has no publication key gate; all expression and metadata
    /// admission is shared with the dataset compiler. Only that compiler creates it.
    pub(crate) fn new_donor(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
    ) -> Result<Self, PlanError> {
        Self::new_implementation(
            sources,
            intermediates,
            output,
            templates,
            columns,
            Vec::new(),
            Vec::new(),
            false,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn new_implementation(
        sources: SourceSchemas,
        intermediates: Vec<Intermediate>,
        output: TableSchema,
        templates: Vec<RowTemplate>,
        columns: Vec<Assignment>,
        keys: Vec<usize>,
        verifications: Vec<Verification>,
        publication: bool,
    ) -> Result<Self, PlanError> {
        let SourceSchemas {
            primary: source,
            secondary,
        } = sources;
        intermediates::validate(&intermediates, &secondary, &source, &output)?;
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
        self_relations::validate(&intermediates, &templates)?;
        let width = output.columns().len();
        if publication {
            validate_columns(&keys, width)?;
        }
        let mut row_columns = None;
        for (template_index, template) in templates.iter().enumerate() {
            let (template_source, mode) =
                template.mode.context(&source, &secondary, &intermediates)?;
            let keyed = matches!(mode, RowMode::Keys);
            if keyed && templates.len() != 1 {
                return Err(PlanError::InvalidKeyMode);
            }
            if let RowMode::Groups(keys) = mode {
                validate_columns(keys, template_source.columns().len())?;
            }
            let mut available = vec![false; width];
            for assignment in &template.assignments {
                odm::visit(&assignment.expression, &mut |bound| {
                    if bound.read().source.is_some() {
                        Err(PlanError::InvalidSource)
                    } else {
                        Ok(())
                    }
                })?;
                relation_predicates::validate_reads(
                    &assignment.expression,
                    &intermediates,
                    template.mode.source_index(),
                    &template.mode,
                    &available,
                )?;
                // A selector compiled against the primary record cannot borrow a
                // different driver's record coordinates. Output match values are shared.
                if template.mode.source_index() != 0
                    && current_record_read(
                        &assignment.expression,
                        &intermediates,
                        RecordReadScope::PrimaryMatching,
                    )
                {
                    return Err(PlanError::InvalidIntermediate);
                }
                if keyed
                    && (!keys.contains(&assignment.column)
                        || !key_expression(&assignment.expression))
                {
                    return Err(PlanError::InvalidKeyMode);
                }
                joined::validate_sources(&assignment.expression, &source, &secondary)?;
                validate_assignment(
                    assignment,
                    &mut available,
                    template_source,
                    &secondary,
                    &intermediates,
                    &output,
                    mode,
                    false,
                )?;
            }
            if keyed && keys.iter().any(|&column| !available[column]) {
                return Err(PlanError::InvalidKeyMode);
            }
            if let Some(filter) = &template.filter {
                let deferred = window_stages(&template.assignments, width, &intermediates);
                if filter.bindings().iter().any(|binding| {
                    matches!(binding.read, Read::Column(column) if deferred.get(column).copied().unwrap_or(false))
                })
                {
                    return Err(PlanError::Filter(BindingError::UnavailableColumn));
                }
                if filter.bindings().iter().any(|binding| {
                    matches!(binding.read, Read::Intermediate { .. } | Read::Joined(_))
                }) {
                    filter
                        .validate_contextual(template_source.columns().len(), &available)
                        .map_err(PlanError::Filter)?;
                    for binding in filter.bindings() {
                        if let Read::Joined(read) = &binding.read {
                            joined::validate_source(read, &source, &secondary)?;
                            read.validate(&source, &secondary, &output, &available)?;
                            if read
                                .keys
                                .iter()
                                .any(|key| deferred.get(key.output_column).copied().unwrap_or(true))
                            {
                                return Err(PlanError::Filter(BindingError::UnavailableColumn));
                            }
                        }
                        if let Read::Intermediate { index, column } = binding.read {
                            let item = intermediates
                                .get(index)
                                .ok_or(PlanError::InvalidIntermediate)?;
                            intermediates::validate_context_read(
                                item,
                                column,
                                template_source,
                                &secondary,
                                &available,
                                &output,
                                mode,
                            )?;
                            for field in item.current_fields() {
                                field.validate_context(
                                    template.mode.source_index(),
                                    mode,
                                    &available,
                                )?;
                            }
                            let mut later = false;
                            visit_output_dependencies(
                                index,
                                |index| intermediates.get(index),
                                |column| {
                                    later |= deferred.get(column).copied().unwrap_or(true);
                                },
                            )?;
                            if later {
                                return Err(PlanError::Filter(BindingError::UnavailableColumn));
                            }
                        }
                    }
                } else {
                    filter
                        .validate(
                            template_source.columns().len(),
                            if keyed { &[] } else { &available },
                            matches!(mode, RowMode::Groups(_)),
                        )
                        .map_err(PlanError::Filter)?;
                }
            }
            if row_columns
                .as_ref()
                .is_some_and(|previous| previous != &available)
            {
                return Err(PlanError::InconsistentRowColumns);
            }
            row_columns = Some(available.clone());
            for assignment in &columns {
                odm::validate_template(
                    &assignment.expression,
                    template,
                    template_index,
                    templates.len(),
                )?;
                relation_predicates::validate_reads(
                    &assignment.expression,
                    &intermediates,
                    template.mode.source_index(),
                    &template.mode,
                    &available,
                )?;
                if (keyed || template.mode.source_index() != 0)
                    && current_record_read(
                        &assignment.expression,
                        &intermediates,
                        if keyed {
                            RecordReadScope::Driver
                        } else {
                            RecordReadScope::All
                        },
                    )
                {
                    return Err(PlanError::InvalidKeyMode);
                }
                joined::validate_sources(&assignment.expression, &source, &secondary)?;
                validate_assignment(
                    assignment,
                    &mut available,
                    &source,
                    &secondary,
                    &intermediates,
                    &output,
                    mode,
                    !publication,
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
                Check::NotMissing
                | Check::AllowedValues(_)
                | Check::Codelist { .. }
                | Check::ValueCodelists { .. }
                | Check::Inventory { .. }
                | Check::ValueMaxLength { .. }
                | Check::Range { .. }
                | Check::MaxLength(_)
                | Check::WideMaxLength(_)
                | Check::Matches(_) => return Err(PlanError::InvalidColumns),
                Check::InvalidDiagnostic(_) => {}
                Check::InvalidDeclaration { .. } => {}
                Check::PredicateDeclaration(predicate) => verification_predicates::validate(
                    predicate,
                    &source,
                    &secondary,
                    &intermediates,
                    &output,
                    &templates,
                )?,
                Check::Assert { when, require } => {
                    for predicate in when.iter().chain([require]) {
                        verification_predicates::validate(
                            predicate,
                            &source,
                            &secondary,
                            &intermediates,
                            &output,
                            &templates,
                        )?;
                    }
                }
                Check::Unique(columns) => {
                    // Unlike row.group_by, unique.columns permits repeated names.
                    if columns.is_empty() || columns.iter().any(|&column| column >= width) {
                        return Err(PlanError::InvalidColumns);
                    }
                }
                Check::AllOrNone(columns) => {
                    if columns.iter().any(|&column| column >= width)
                        || !columns.iter().any(|column| columns.first() != Some(column))
                    {
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
                Check::ScopedRowCount(check) => check.validate(width)?,
                Check::WideRowCount { min, max } => {
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
            row_contexts: Vec::new(),
            column_verifications: Vec::new(),
            warning_verifications: Default::default(),
            conversion_handlers: Vec::new(),
            conversion_sites: BTreeMap::new(),
            output_order: Vec::new(),
        })
    }
}

impl DatasetPlan {
    /// Admit bound presentation terms separately from derivation and window order.
    pub fn with_output_order(mut self, terms: Vec<OrderTerm>) -> Result<Self, PlanError> {
        if terms.iter().enumerate().any(|(position, term)| {
            term.column >= self.output.columns().len()
                || terms[..position]
                    .iter()
                    .any(|other| other.column == term.column)
        }) {
            return Err(PlanError::InvalidOutputOrder);
        }
        self.output_order = terms;
        Ok(self)
    }
    /// Presentation occurs only after all keys and verifications succeed.
    pub fn output_order(&self) -> &[OrderTerm] {
        &self.output_order
    }
    /// Severity belongs to admitted check declarations, never host issue rows.
    pub fn with_warning_verifications(mut self, paths: Vec<String>) -> Result<Self, PlanError> {
        let checks: alloc::collections::BTreeSet<_> = self
            .verifications
            .iter()
            .chain(
                self.column_verifications
                    .iter()
                    .flat_map(|group| &group.checks),
            )
            .map(|check| check.path.as_str())
            .collect();
        let mut warnings = alloc::collections::BTreeSet::new();
        for path in paths {
            if !checks.contains(path.as_str()) || !warnings.insert(path) {
                return Err(PlanError::InvalidColumns);
            }
        }
        self.warning_verifications = warnings;
        Ok(self)
    }
    pub fn verification_severity(&self, path: &str) -> VerificationSeverity {
        if self.warning_verifications.contains(path) {
            VerificationSeverity::Warning
        } else {
            VerificationSeverity::Error
        }
    }
    /// Admit ordered column checks before any snapshot or value access.
    pub fn with_column_verifications(
        mut self,
        groups: Vec<ColumnVerifications>,
    ) -> Result<Self, PlanError> {
        let mut paths = alloc::collections::BTreeSet::new();
        for (index, group) in groups.iter().enumerate() {
            if group.column >= self.output.columns().len()
                || groups[..index]
                    .iter()
                    .any(|previous| previous.column >= group.column)
            {
                return Err(PlanError::InvalidColumns);
            }
            for verification in &group.checks {
                if verification.path.is_empty() {
                    return Err(PlanError::EmptyPath);
                }
                if !paths.insert(&verification.path)
                    || self
                        .verifications
                        .iter()
                        .any(|check| check.path == verification.path)
                {
                    return Err(PlanError::DuplicateVerificationPath);
                }
                if !matches!(
                    verification.check,
                    Check::NotMissing
                        | Check::AllowedValues(_)
                        | Check::Codelist { .. }
                        | Check::ValueCodelists { .. }
                        | Check::Inventory { .. }
                        | Check::ValueMaxLength { .. }
                        | Check::Range { .. }
                        | Check::MaxLength(_)
                        | Check::WideMaxLength(_)
                        | Check::Matches(_)
                        | Check::InvalidDeclaration { .. }
                        | Check::InvalidDiagnostic(_)
                ) {
                    return Err(PlanError::InvalidColumns);
                }
                crate::dataset_checks::validate(
                    &verification.check,
                    self.output.columns()[group.column].kind,
                )?;
                if let Check::ValueMaxLength {
                    discriminator,
                    codes,
                    ..
                } = &verification.check
                {
                    if self
                        .output
                        .columns()
                        .get(*discriminator)
                        .is_none_or(|column| column.kind != crate::value::ColumnType::Str)
                        || codes.is_empty()
                        || codes
                            .iter()
                            .enumerate()
                            .any(|(index, code)| codes[..index].contains(code))
                    {
                        return Err(PlanError::InvalidColumns);
                    }
                }
                if let Check::Inventory {
                    scope: Some(scope), ..
                } = &verification.check
                {
                    if self
                        .output
                        .columns()
                        .get(scope.discriminator)
                        .is_none_or(|column| column.kind != crate::value::ColumnType::Str)
                        || scope
                            .overrides
                            .iter()
                            .enumerate()
                            .any(|(index, (code, _))| {
                                scope.overrides[..index]
                                    .iter()
                                    .any(|(prior, _)| prior == code)
                            })
                    {
                        return Err(PlanError::InvalidColumns);
                    }
                }
                if let Check::ValueCodelists {
                    discriminator,
                    overrides,
                    bindings,
                } = &verification.check
                {
                    if self
                        .output
                        .columns()
                        .get(*discriminator)
                        .is_none_or(|column| column.kind != crate::value::ColumnType::Str)
                        || overrides.is_empty()
                        || overrides
                            .iter()
                            .enumerate()
                            .any(|(index, code)| overrides[..index].contains(code))
                        || bindings.iter().filter(|binding| binding.default).count() > 1
                        || bindings.iter().enumerate().any(|(index, binding)| {
                            binding.codes.iter().enumerate().any(|(code_index, code)| {
                                binding.codes[..code_index].contains(code)
                                    || bindings[..index]
                                        .iter()
                                        .any(|prior| prior.codes.contains(code))
                            })
                        })
                        || bindings.iter().any(|binding| {
                            binding.codes.iter().any(|code| !overrides.contains(code))
                        })
                    {
                        return Err(PlanError::InvalidColumns);
                    }
                }
            }
        }
        self.column_verifications = groups;
        Ok(self)
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
    /// Borrow admitted templates without exposing mutation.
    pub fn templates(&self) -> &[RowTemplate] {
        &self.templates
    }
    /// Borrow admitted columns without exposing mutation.
    pub fn columns(&self) -> &[Assignment] {
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
#[path = "dataset_donor_admission.rs"]
mod donor_admission_tests;

#[path = "dataset_verification_predicate.rs"]
mod verification_predicates;
