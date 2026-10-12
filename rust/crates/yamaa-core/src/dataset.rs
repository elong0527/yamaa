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
pub use count::{CountBoundError, RowCountBound, MAX_COUNT_BOUND_BYTES};
#[path = "dataset_flag.rs"]
mod flags;
pub use flags::{
    flag_condition_diagnostic, Flag, FlagBinding, FlagError, FlagRead, MAX_FLAG_METADATA_BYTES,
};
#[path = "dataset_functions.rs"]
mod functions;
pub use functions::{BoundFunction, BoundProjectFunction, FunctionArgument, FunctionInput};
#[path = "dataset_intermediates.rs"]
mod intermediates;
pub use intermediates::{Intermediate, RecordMatchKey, SourceSchemas};
#[path = "dataset_self.rs"]
mod self_relations;
pub use self_relations::IntermediateSource;
#[path = "dataset_match_values.rs"]
mod match_values;
pub use match_values::{IntermediateKey, MatchRead, MatchValue};
#[path = "dataset_lookup.rs"]
mod lookup;
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
pub use windows::{OrderTerm, Window, WindowKind};

/// Already bound expressions; no host joins or lookup fallback are implicit.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
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
    Range {
        min: Option<Value>,
        max: Option<Value>,
    },
    /// Unicode scalar count, rather than encoded bytes or grapheme clusters.
    MaxLength(usize),
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
}

impl Check {
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
fn validate_assignment(
    assignment: &Assignment,
    available: &mut [bool],
    source: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Intermediate],
    output: &TableSchema,
    mode: &RowMode,
) -> Result<(), PlanError> {
    validate_assignment_at(
        assignment,
        available,
        source,
        secondary,
        intermediates,
        output,
        mode,
        0,
        &mut 65_536,
    )
}
#[allow(clippy::too_many_arguments)]
fn validate_assignment_at(
    assignment: &Assignment,
    available: &mut [bool],
    source: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Intermediate],
    output: &TableSchema,
    mode: &RowMode,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), PlanError> {
    if depth > 64 || *remaining == 0 {
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
    match &assignment.expression {
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
                    if let RowMode::Groups(keys) = mode {
                        if when.bindings().iter().any(|binding| matches!(binding.read, Read::Source(column) if !keys.contains(&column))) {
                            return Err(PlanError::NonGroupSource);
                        }
                    }
                    when.validate(source.columns().len(), available, false)
                        .map_err(PlanError::Filter)?;
                }
                validate_assignment_at(
                    &branch.assignment,
                    available,
                    source,
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
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
                    if !matches!(mode, RowMode::Keys) && !item.source.is_self() {
                        return Err(PlanError::InvalidIntermediate);
                    }
                    intermediates::validate_context_read(
                        item, column, source, secondary, available, output, mode,
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
                secondary,
                intermediates,
                output,
                mode,
                depth + 1,
                remaining,
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
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
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
                secondary,
                intermediates,
                output,
                mode,
                depth + 1,
                remaining,
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
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
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
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
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
                    secondary,
                    intermediates,
                    output,
                    mode,
                    depth + 1,
                    remaining,
                )?;
                available[assignment.column] = false;
            }
        }
        Expression::Function(function) => function.validate(source, available, mode)?,
        Expression::ProjectFunction(function) => function.validate(source, available, mode)?,
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
                        secondary,
                        intermediates,
                        output,
                        mode,
                        depth + 1,
                        remaining,
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
                secondary,
                intermediates,
                output,
                mode,
                depth + 1,
                remaining,
            )?;
            available[assignment.column] = false;
        }
        Expression::SourceLookup {
            lookup: bound,
            filter,
            selection,
        } => {
            if !matches!(mode, RowMode::Keys) {
                return Err(PlanError::InvalidLookup);
            }
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
                item, *column, source, secondary, available, output, mode,
            )?;
        }
        Expression::RowLookup(lookup) => lookup::validate_row(lookup, source, secondary, mode)?,
        Expression::Lookup(lookup) => {
            if !matches!(mode, RowMode::Keys) {
                return Err(PlanError::InvalidLookup);
            }
            lookup::validate(lookup, secondary, available, output)?;
        }
        Expression::Window(window) => {
            if depth != 0 {
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
        Expression::Case(branches) => branches
            .iter()
            .all(|branch| key_expression(&branch.assignment.expression)),
        _ => false,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordReadScope {
    Driver,
    PrimaryMatching,
    All,
}
fn current_record_read(
    expression: &Expression,
    intermediates: &[Intermediate],
    scope: RecordReadScope,
) -> bool {
    let include_raw = scope != RecordReadScope::PrimaryMatching;
    match expression {
        Expression::Source(_) | Expression::RowLookup(_) => include_raw,
        Expression::Intermediate { index, .. } => {
            scope != RecordReadScope::Driver
                && intermediates.get(*index).is_some_and(|item| {
                    item.keys
                        .iter()
                        .filter_map(|key| key.value.read())
                        .any(|read| matches!(read, MatchRead::Record { .. }))
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
        Expression::Function(function) => include_raw && function.reads_source(),
        Expression::ProjectFunction(function) => include_raw && function.reads_source(),
        Expression::Case(branches) => branches.iter().any(|branch| {
            include_raw
                && branch.when.as_ref().is_some_and(|predicate| {
                    predicate
                        .bindings()
                        .iter()
                        .any(|binding| matches!(binding.read, Read::Source(_)))
                })
                || current_record_read(&branch.assignment.expression, intermediates, scope)
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
        validate_columns(&keys, width)?;
        let mut row_columns = None;
        for template in &templates {
            let (template_source, mode) = template.mode.context(&source, &secondary)?;
            let keyed = matches!(mode, RowMode::Keys);
            if keyed && templates.len() != 1 {
                return Err(PlanError::InvalidKeyMode);
            }
            if let RowMode::Groups(keys) = mode {
                validate_columns(keys, template_source.columns().len())?;
            }
            let mut available = vec![false; width];
            for assignment in &template.assignments {
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
                validate_assignment(
                    assignment,
                    &mut available,
                    template_source,
                    &secondary,
                    &intermediates,
                    &output,
                    mode,
                )?;
            }
            if keyed && keys.iter().any(|&column| !available[column]) {
                return Err(PlanError::InvalidKeyMode);
            }
            if let Some(filter) = &template.filter {
                let deferred = window_stages(&template.assignments, width, &intermediates);
                if !matches!(mode, RowMode::Groups(_))
                    && filter.bindings().iter().any(|binding| {
                        matches!(binding.read, Read::Column(column) if deferred.get(column).copied().unwrap_or(false))
                    })
                {
                    return Err(PlanError::Filter(BindingError::UnavailableColumn));
                }
                filter
                    .validate(
                        template_source.columns().len(),
                        if keyed { &[] } else { &available },
                        matches!(mode, RowMode::Groups(_)),
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
                validate_assignment(
                    assignment,
                    &mut available,
                    &source,
                    &secondary,
                    &intermediates,
                    &output,
                    mode,
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
                | Check::Range { .. }
                | Check::MaxLength(_)
                | Check::Matches(_) => return Err(PlanError::InvalidColumns),
                Check::InvalidDiagnostic(_) => {}
                Check::InvalidDeclaration { .. } => {}
                Check::PredicateDeclaration(predicate) => predicate
                    .validate(0, &vec![true; width], true)
                    .map_err(PlanError::Predicate)?,
                Check::Assert { when, require } => {
                    for predicate in when.iter().chain([require]) {
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
                        | Check::Range { .. }
                        | Check::MaxLength(_)
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
