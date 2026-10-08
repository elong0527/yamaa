//! Immutable admitted dataset model, independent of execution and host effects.
pub use crate::bound_expression::BoundNumeric;
use crate::{
    bound_expression::{BindingError, BoundPredicate},
    reduction::NumericReducer,
    table::TableSchema,
    value::Value,
};
use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
#[path = "dataset_conversion.rs"]
mod conversion;
pub use conversion::ConversionHandler;
#[path = "dataset_functions.rs"]
mod functions;
pub use functions::{BoundFunction, FunctionArgument, FunctionInput};
#[path = "dataset_intermediates.rs"]
mod intermediates;
pub use intermediates::{Intermediate, SourceSchemas};
#[path = "dataset_lookup.rs"]
mod lookup;
#[path = "dataset_selection.rs"]
mod selection;
pub use selection::{FirstAvailable, SelectionRead, SelectionSource};
#[path = "dataset_windows.rs"]
mod windows;
pub use windows::{OrderTerm, Window, WindowKind};

/// Already bound expressions; no host joins or lookup fallback are implicit.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Literal(Value),
    /// Compiled scalar arithmetic over statically bound source/completed output reads.
    Compute(BoundNumeric),
    /// Explicit prebound host invocation; never discovered or activated during execution.
    Function(BoundFunction),
    /// Column-phase windows over completed key-grain output rows.
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
    Range {
        min: Option<Value>,
        max: Option<Value>,
    },
    /// Unicode scalar count, rather than encoded bytes or grapheme clusters.
    MaxLength(usize),
    /// A core-owned declaration finding deferred until dataset verification.
    InvalidDiagnostic(crate::diagnostic::Diagnostic),
    /// A compiler finding evaluated in declaration order after output keys.
    InvalidDeclaration {
        condition: &'static str,
        requirement: &'static str,
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verification {
    pub path: String,
    pub check: Check,
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
        Expression::Compute(expression) => expression
            .validate(
                source.columns().len(),
                available,
                matches!(mode, RowMode::Groups(_)),
            )
            .map_err(|error| match error {
                crate::bound_expression::ScopeError::GroupedSource => PlanError::NonGroupSource,
                crate::bound_expression::ScopeError::InvalidSource => PlanError::InvalidSource,
                crate::bound_expression::ScopeError::UnavailableColumn => {
                    PlanError::UnavailableColumn
                }
            })?,
        Expression::Function(function) => function.validate(source, available, mode)?,
        Expression::FirstAvailable(selection) => selection.validate(source, available, mode)?,
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
        Expression::RowLookup(lookup) => lookup::validate_row(lookup, source, secondary, mode)?,
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
                            Expression::FirstAvailable(_)
                                | Expression::Collect { .. }
                                | Expression::Window(_)
                                | Expression::Lookup(_)
                                | Expression::RowLookup(_)
                                | Expression::Function(_)
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
                if keyed
                    && matches!(&assignment.expression, Expression::Function(function) if function.reads_source())
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
                Check::NotMissing
                | Check::AllowedValues(_)
                | Check::Range { .. }
                | Check::MaxLength(_) => return Err(PlanError::InvalidColumns),
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
            conversion_handlers: Vec::new(),
            conversion_sites: BTreeMap::new(),
        })
    }
}

impl DatasetPlan {
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
                        | Check::Range { .. }
                        | Check::MaxLength(_)
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
