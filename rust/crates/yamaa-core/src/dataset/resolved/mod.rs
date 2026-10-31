//! The resolved plan graph: every bound [`DatasetPlan`] compiled once into
//! dense integer slots (rewrite 1, #2840).
//!
//! Resolution lowers a bound plan, its donor plans included, into one
//! immutable arena. Donor plans become regions of that arena and nothing in
//! it is an `Rc`, so a [`ResolvedPlan`] is `Send + Sync` and can be shared by
//! concurrent runs. Every identifier, handler site and
//! cache key is a slot: [`ColumnId`], [`SourceId`], [`TemplateId`],
//! [`BindingId`], [`ExprId`], [`RelationId`], [`WindowId`],
//! [`HandlerSiteId`], [`CheckId`] and [`StepId`]. Names and authored paths
//! are kept only for diagnostics and reports.
//!
//! Bindings are keyed by (template, column). A column is row-only,
//! column-only or mixed; each template that completes a column in the column
//! phase is served by a binding with its own [`DriverContext`], and cells with
//! equal contexts share one binding.
//!
//! The [`ResolvedPlan::schedule`] orders every step of a run. Resolution
//! checks on every bind, whatever the build profile, that each data edge
//! reads a cell an earlier step of the reading cell's template completed, and
//! refuses the plan as internal when one does not, naming the step.
//!
//! Every node carries its evaluation payload, copied from the bound plan: an
//! expression node its [`Op`], a named relation its intermediate, a check its
//! declaration, a region its output schema and inputs. The plan is therefore
//! self-contained, which the executor over this graph and the compile-once
//! plan handle (#2821) need.
use crate::{
    numeric_lifecycle::HandlerKind,
    table::TableSchema,
    value::{ColumnType, Value},
};
use std::{ops::Range, sync::Arc};

mod build;
mod registration;
mod schedule;
#[cfg(test)]
mod tests;
mod verify;

use super::{
    AggregateRowContext, BoundAggregate, BoundNumeric, BoundOdm, BoundProjectFunction, Check,
    ContextWindow, DatasetPlan, Flag, Imputation, Intermediate, IntermediateUnique, JoinAbsence,
    JoinedRead, Keep, Lookup, OdmTemplateScope, RelationRange, Rounding, SourceSelection,
    StringCase, StringDecimal, StringExtract, StringLength, StringPad, StringReplace, StringSubstr,
    StringTemplate, Temporal, VerificationSeverity, Window,
};
use crate::bound_expression::BoundPredicate;
use crate::conversion::LiteralHandler;

macro_rules! slot {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);
        impl $name {
            /// Position of this slot in its table.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
        impl TryFrom<usize> for $name {
            type Error = ResolutionError;
            fn try_from(index: usize) -> Result<Self, ResolutionError> {
                u32::try_from(index)
                    .map(Self)
                    .map_err(|_| ResolutionError::Capacity)
            }
        }
    };
}
slot!(
    /// One column of one region, in declaration order within the region.
    ColumnId
);
slot!(
    /// One source of one region: the driver first, then the secondary inputs.
    SourceId
);
slot!(
    /// One row template; a plan without rows has one key-grain template.
    TemplateId
);
slot!(
    /// One bound derivation, serving one or more (template, column) cells.
    BindingId
);
slot!(
    /// One expression node of the arena.
    ExprId
);
slot!(
    /// One selection shape: a named intermediate, an implicit join, an
    /// aggregate relation or an ODM item read.
    RelationId
);
slot!(
    /// One window evaluation.
    WindowId
);
slot!(
    /// One (specification path, handler kind) site.
    HandlerSiteId
);
slot!(
    /// One check, including the compiler's deferred findings.
    CheckId
);
slot!(
    /// One position in the schedule.
    StepId
);
slot!(
    /// One region: the dataset itself, or the donor plan of a derived intermediate.
    RegionId
);
impl RegionId {
    /// The dataset's own region; donor regions follow it.
    pub const DATASET: Self = Self(0);
}

/// Immutable once resolved: nothing in it is written during a run.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPlan {
    regions: Vec<Region>,
    sources: Vec<SourceSlot>,
    columns: Vec<ColumnNode>,
    templates: Vec<TemplateNode>,
    bindings: Vec<Binding>,
    cells: Vec<Option<BindingId>>,
    exprs: Vec<ExprNode>,
    operands: Vec<Operand>,
    reads: Vec<Read>,
    node_sites: Vec<NodeSite>,
    relations: Vec<RelationNode>,
    windows: Vec<WindowNode>,
    checks: Vec<CheckNode>,
    handlers: Vec<HandlerSite>,
    functions: Vec<FunctionSlot>,
    schedule: Vec<StepInfo>,
    keys: Vec<ColumnId>,
    output_order: Vec<OrderKey>,
}

/// The dataset (region 0) or the donor plan of a derived intermediate.
/// Each region owns contiguous ranges of every table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub kind: RegionKind,
    sources: Range<u32>,
    columns: Range<u32>,
    templates: Range<u32>,
    bindings: Range<u32>,
    relations: Range<u32>,
    schedule: Range<u32>,
    cells: u32,
    /// Conversion handler sites in the plan's declared order.
    conversions: Vec<HandlerSiteId>,
    /// Column-phase columns in the compiler's order.
    column_order: Vec<ColumnId>,
    /// The region's output schema, in declaration order.
    pub output: TableSchema,
    /// Inputs whose every implicit join must find a record (LOOKUP-24).
    pub required_matches: std::collections::BTreeSet<String>,
    /// The region's secondary inputs, in plan order.
    pub secondary: Vec<super::SecondarySource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegionKind {
    Dataset,
    /// Donor rows copy `stored_columns` fields, then evaluate the derivations
    /// in declaration order. Only `typed_columns` convert their results.
    Donor {
        stored_columns: u32,
        typed_columns: Vec<ColumnId>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceSlot {
    /// The driver of the dataset region has no name in the bound plan.
    pub name: Option<Arc<str>>,
    pub schema: TableSchema,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnNode {
    pub name: Arc<str>,
    /// Untyped donor results have no declared type.
    pub kind: Option<ColumnType>,
    pub phase: ColumnPhase,
    /// The column-phase bindings of this column: each column binding of the
    /// bound plan in turn, its templates split by driver context.
    column_bindings: Range<u32>,
}

/// Which phase completes a column, decided from the templates' row bindings (#2754).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnPhase {
    RowOnly,
    ColumnOnly,
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateNode {
    pub region: RegionId,
    pub driver: DriverContext,
    pub filter: Option<TemplateFilter>,
    /// Authored row identity, when the bound plan retains it.
    pub row: Option<Arc<str>>,
    /// The authored row identity and group fields of a grouped template,
    /// for aggregate diagnostics.
    pub row_context: Option<AggregateRowContext>,
    /// The template's row bindings, in resolved dependency order.
    row_bindings: Range<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateFilter {
    pub expr: ExprId,
    pub placement: FilterPlacement,
}

/// When a template filter runs relative to its bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterPlacement {
    /// Key grain: over every source record, before any key binding.
    BeforeKeys,
    /// After the prewindow bindings, before the window stage (ROW-3, COLUMN-9).
    Prewindow,
    /// A grouped filter that reads the window stage.
    Postwindow,
}

/// The driver and ODM scope a binding evaluates against (#2829).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverContext {
    pub source: SourceId,
    /// The named intermediate that drives the template.
    pub named: Option<RelationId>,
    pub grain: Grain,
    /// This template's scope for deferred column-phase ODM reads only.
    pub odm: Option<OdmTemplateScope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Grain {
    Records,
    /// Group key fields of the driver.
    Groups(Vec<u32>),
    Keys,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub column: ColumnId,
    /// The cells this binding serves.
    pub templates: Vec<TemplateId>,
    pub stage: Stage,
    pub driver: DriverContext,
    pub expr: ExprId,
    /// Authored path, for diagnostics only.
    pub path: Arc<str>,
    /// The conversion handler site of the completed result.
    pub unconvertible: Option<HandlerSiteId>,
    /// The declared replacement for an unconvertible result.
    pub handler: Option<LiteralHandler>,
    pub origin: BindingOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Prewindow,
    Window,
    Column,
}

/// The bound plan's own coordinates of a binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingOrigin {
    /// Assignment `assignment` of the region's template `template`.
    Template { template: u32, assignment: u32 },
    /// Column binding `assignment` of the region's column phase.
    Column { assignment: u32 },
}

/// One expression node. Operands and reads are slots; the operation and its
/// authored facts are the node's [`Op`].
#[derive(Clone, Debug, PartialEq)]
pub struct ExprNode {
    pub kind: ExprKind,
    pub path: Arc<str>,
    pub op: Op,
    operands: Range<u32>,
    reads: Range<u32>,
    sites: Range<u32>,
}

/// What an expression node computes, copied from the bound plan so that the
/// resolved plan is self-contained. Operands and reads stay slots on the
/// node; the payload keeps the operation and the authored facts its
/// diagnostics need. Payload coordinates are local to the node's region.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Literal(Value),
    /// A plain read: its one read slot is the whole operation.
    Read,
    /// Operand `index` of a match expression, resolved for the current row
    /// before the expression is evaluated.
    MatchOperand {
        index: usize,
    },
    /// Distinct present readings across the records feeding the row; a
    /// filter operand narrows them first.
    Collect {
        identifier: String,
        selection: Option<SourceSelection>,
    },
    /// Reads follow the bindings in order.
    Compute(BoundNumeric),
    /// Reads follow the bindings in order.
    Flag(Flag),
    /// Operands are the conditions and branches in authored order.
    Case,
    /// Reads follow the bindings in order.
    Predicate(BoundPredicate),
    /// A cut or mapping. A literal it selects through a handler is checked
    /// against the column's type unless a conversion handler is declared at
    /// the node's own path.
    MappingCut {
        operation: crate::mapping_cut::MappingCut,
        column: usize,
        handled: bool,
    },
    Round(Rounding),
    Imputation(Imputation),
    Temporal(Temporal),
    Extreme {
        largest: bool,
    },
    StrTemplate(StringTemplate),
    StrExtract(StringExtract),
    StrReplace(StringReplace),
    StrCase(StringCase),
    StrPad(StringPad),
    StrSubstr(StringSubstr),
    StrLength(StringLength),
    StrDecimal(StringDecimal),
    SourceBinding {
        absent: Option<Value>,
        variable: String,
        path: String,
    },
    /// Operands are the sources in authored order, each with its path.
    FirstAvailable {
        missing: Value,
        missing_path: String,
        sources: Vec<String>,
    },
    /// Reads follow the read and collected arguments in order.
    ProjectFunction(BoundProjectFunction),
    /// A window and the column of the assignment that reads it.
    Window {
        window: Window,
        column: usize,
    },
    ContextWindow {
        context: ContextWindow,
        column: usize,
    },
    Intermediate {
        column: usize,
    },
    Joined(JoinedRead),
    JoinedSource {
        read: JoinedRead,
        range: Option<RelationRange>,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
        absence: Option<JoinAbsence>,
    },
    SourceLookup {
        lookup: Lookup,
        range: Option<RelationRange>,
        filter: Option<BoundPredicate>,
        selection: Option<SourceSelection>,
        absence: Option<JoinAbsence>,
    },
    /// Operands are the expression keys, then one wrapper per derivation.
    Aggregate(Box<BoundAggregate>),
    Odm(Box<BoundOdm>),
    /// One aggregate derivation: its operand is the derived expression.
    Derivation {
        column: usize,
        kind: ColumnType,
        handler: Option<LiteralHandler>,
    },
}

impl Op {
    /// Whether this payload is the operation of a node of `kind`; the payload
    /// of every resolved node is.
    pub fn fits(&self, kind: ExprKind) -> bool {
        use ExprKind as K;
        matches!(
            (kind, self),
            (K::Literal, Op::Literal(_))
                | (K::Source, Op::Read | Op::MatchOperand { .. })
                | (K::Column, Op::Read)
                | (K::Collect, Op::Collect { .. })
                | (K::Compute, Op::Compute(_))
                | (K::Flag, Op::Flag(_))
                | (K::Case, Op::Case)
                | (K::Predicate, Op::Predicate(_))
                | (K::MappingCut, Op::MappingCut { .. })
                | (K::Round, Op::Round(_))
                | (K::Imputation, Op::Imputation(_))
                | (K::Temporal, Op::Temporal(_))
                | (K::Extreme, Op::Extreme { .. })
                | (K::StrTemplate, Op::StrTemplate(_))
                | (K::StrExtract, Op::StrExtract(_))
                | (K::StrReplace, Op::StrReplace(_))
                | (K::StrCase, Op::StrCase(_))
                | (K::StrPad, Op::StrPad(_))
                | (K::StrSubstr, Op::StrSubstr(_))
                | (K::StrLength, Op::StrLength(_))
                | (K::StrDecimal, Op::StrDecimal(_))
                | (K::SourceBinding, Op::SourceBinding { .. })
                | (K::FirstAvailable, Op::FirstAvailable { .. })
                | (K::ProjectFunction, Op::ProjectFunction(_))
                | (K::Window(_), Op::Window { .. })
                | (K::ContextWindow(_), Op::ContextWindow { .. })
                | (K::Intermediate(_), Op::Intermediate { .. })
                | (K::Joined(_), Op::Joined(_))
                | (K::JoinedSource(_), Op::JoinedSource { .. })
                | (K::SourceLookup(_), Op::SourceLookup { .. })
                | (K::Aggregate(_), Op::Aggregate(_))
                | (K::Odm(_), Op::Odm(_))
                | (K::Derivation, Op::Derivation { .. })
        )
    }
}

/// The operation family of one expression node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Literal,
    Source,
    Column,
    Collect,
    Compute,
    Flag,
    Case,
    /// A bound predicate: a case condition, a filter or a check predicate.
    Predicate,
    MappingCut,
    Round,
    Imputation,
    Temporal,
    Extreme,
    StrTemplate,
    StrExtract,
    StrReplace,
    StrCase,
    StrPad,
    StrSubstr,
    StrLength,
    StrDecimal,
    SourceBinding,
    FirstAvailable,
    ProjectFunction,
    Window(WindowId),
    ContextWindow(WindowId),
    Intermediate(RelationId),
    Joined(RelationId),
    JoinedSource(RelationId),
    SourceLookup(RelationId),
    Aggregate(RelationId),
    Odm(RelationId),
    /// One aggregate derivation and its conversion handler.
    Derivation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub expr: ExprId,
    pub role: OperandRole,
    pub demand: Demand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperandRole {
    /// A nested value assignment.
    Value,
    /// A case condition.
    When,
    /// A case branch assignment.
    Branch,
    /// One `first_available` operand.
    Selection,
    /// A filter over the records feeding a collection.
    Filter,
    /// An aggregate key expression.
    Key,
    /// An aggregate derivation.
    Derivation,
}

/// Demand edges are evaluated only when reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Demand {
    Always,
    Reached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// Completed value of the current row.
    Column(ColumnId),
    /// Raw field of the binding's driver record, or of the records feeding it.
    Field(u32),
    /// Field of a record the relation selected or considers.
    Relation(RelationId, u32),
    Window(WindowId),
}

/// One handler site an expression node can fire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeSite {
    pub site: HandlerSiteId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationNode {
    pub region: RegionId,
    pub kind: RelationKind,
    pub path: Arc<str>,
    pub donor: Donor,
    /// Equality keys: a donor field and the current value it must equal.
    pub keys: Vec<RelationKey>,
    /// A nearest selection's source field and the current value its
    /// distance is measured to (LOOKUP-26).
    pub nearest: Option<RelationKey>,
    /// Correlated current-row operands: a named relation's filter fields in
    /// their declared positions, then its donor operands; an ODM read's scope.
    pub current: Vec<Read>,
    /// A donor predicate; its field reads address the donor record.
    pub filter: Option<ExprId>,
    pub range: Option<RangeNode>,
    pub selection: Option<SelectionShape>,
    pub no_match: Option<Value>,
    /// The region evaluating the donor derivations.
    pub derivations: Option<RegionId>,
    /// The region evaluating a pair relation's key values over its left
    /// records (LOOKUP-27, LOOKUP-6).
    pub pair_values: Option<RegionId>,
    /// A key value depends on a window over the completed relation.
    pub windowed: bool,
    /// Output columns read across every candidate (output aggregates).
    pub scan: Vec<ColumnId>,
    /// The bound declaration of a named intermediate; its donor plan, if
    /// any, is the `derivations` region.
    pub named: Option<Box<Intermediate>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationKind {
    /// Intermediate `index` of the region's plan.
    Named {
        index: u32,
    },
    Join,
    CurrentGroup,
    MatchedAggregate,
    OutputAggregate,
    OdmItems,
}

/// Where a relation's records come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Donor {
    Source(SourceId),
    /// The driver of the reading row.
    Driver,
    /// Completed rows of earlier templates.
    SelfRows,
    /// Completed output candidates.
    Output,
    /// The records feeding the current grouped row.
    Group,
    /// The matching pairs of a pair relation's two sides, each a joined
    /// record, materialized before any row is built (LOOKUP-27).
    Pairs,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationKey {
    pub field: u32,
    pub value: KeyValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyValue {
    Read(Read),
    Padded(Read),
    Literal(Value),
    Expr(ExprId),
    /// An earlier selector copied into a donor plan whose operand is not
    /// visible there; nothing in that region may read the relation.
    Unbound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeNode {
    pub current: Read,
    pub lower: Option<u32>,
    pub upper: Option<u32>,
    pub closed: crate::expression_table::Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionShape {
    pub order_by: Vec<FieldOrder>,
    pub keep: Keep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldOrder {
    pub field: u32,
    pub descending: bool,
    pub nulls_first: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowNode {
    pub region: RegionId,
    pub function: WindowFunction,
    pub path: Arc<str>,
    /// Reads across the window's scope: completed columns, or driver fields
    /// collected for a context window.
    pub group_by: Vec<Read>,
    pub order_by: Vec<ReadOrder>,
    /// The value, date or reference-date operands of `function`.
    pub operands: Vec<Read>,
    pub filter: Option<ExprId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowFunction {
    RowNumber,
    Competition,
    Dense,
    RowValue {
        offset: i64,
    },
    /// The closest earlier present value; the current row's own present value
    /// answers first when `include_current` is set.
    Locf {
        include_current: bool,
    },
    /// Operands: the date, the reference date and the value; the order terms
    /// hold the declared `tie_break`.
    BaselineFlag {
        inclusive: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOrder {
    pub read: Read,
    pub descending: bool,
    pub nulls_first: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckNode {
    pub path: Arc<str>,
    pub scope: CheckScope,
    pub family: CheckFamily,
    pub severity: VerificationSeverity,
    /// The step at which the check is ready and evaluated.
    pub ready_at: StepId,
    /// Columns the check reads across every candidate.
    reads: Range<u32>,
    pub predicates: Vec<ExprId>,
    /// Position within its column group, key list or dataset checks.
    position: u32,
    pub payload: CheckPayload,
}

/// What a check evaluates, copied from the bound plan.
#[derive(Clone, Debug, PartialEq)]
pub enum CheckPayload {
    /// An output key check; its reads are the keys.
    Keys,
    /// An intermediate's `unique` verification.
    Unique(IntermediateUnique),
    /// A declared column or dataset check.
    Declared(Check),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckScope {
    OutputKeys,
    Intermediate(RelationId),
    Column(ColumnId),
    Dataset,
}

/// The check vocabulary of `dataset::Check`, plus key and intermediate checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckFamily {
    KeyMissing,
    KeyDuplicates,
    IntermediateUnique,
    NotMissing,
    Mandatory,
    DataType,
    AllowedValues,
    Codelist,
    ValueCodelists,
    Inventory,
    ValueMaxLength,
    ValueEntries,
    ValueEntryMatch,
    Range,
    MaxLength,
    WideMaxLength,
    Matches,
    InvalidDiagnostic,
    InvalidDeclaration,
    Assert,
    PredicateDeclaration,
    Unique,
    AllOrNone,
    RowCount,
    WideRowCount,
    ScopedRowCount,
}

/// One reported handler site. Every site is registered at zero before any
/// row is read, in table order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerSite {
    pub path: Arc<str>,
    pub kind: HandlerKind,
}

/// One callback site, admitted against the function port before any table read.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSlot {
    pub expr: ExprId,
    pub slot: usize,
    pub signature: crate::function_signature::ProjectInvocationPlan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderKey {
    pub column: ColumnId,
    pub descending: bool,
    pub nulls_first: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepInfo {
    pub step: Step,
    /// A cancellation poll before the step (#2822). Only dataset steps are
    /// checkpoints: a donor region runs inside the dataset step that reads
    /// it, sometimes once per current row (LOOKUP-15), so that step's
    /// checkpoint covers it.
    pub checkpoint: bool,
    /// The step's class under partitions of consecutive records (#3697).
    pub partition: PartitionClass,
    /// The step's class under key-bounded partitions (#3763).
    pub keyed: KeyedClass,
    pub effects: Effects,
}

impl StepInfo {
    /// The step's class when the partitions never split a group of the
    /// leading `keys` declared keys, or, for zero keys, when they are
    /// consecutive records (#3763).
    pub fn class(&self, keys: usize) -> PartitionClass {
        if keys > 0 && self.keyed.keys as usize >= keys {
            self.keyed.class
        } else {
            self.partition
        }
    }
}

/// A step's class under key-bounded partitions (#3763). A window or an
/// aggregate whose grouping includes every partition key reads only rows of
/// one partition, so it is partition-safe there, while
/// [`StepInfo::partition`] marks it global.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyedClass {
    /// How many leading declared keys every window and aggregate of the step
    /// groups by, each a key the driver input holds; `u32::MAX` for a step
    /// that reads none.
    pub keys: u32,
    /// The step's class when the partitions keep that many leading keys'
    /// groups whole.
    pub class: PartitionClass,
}

/// One step of the schedule, in the order a run visits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Admit,
    VerifyIntermediates { self_rows: bool },
    KeyGrain,
    Driver(TemplateId),
    Row(BindingId),
    Filter(TemplateId),
    Window(BindingId),
    PostwindowFilter(TemplateId),
    PublishSelf(TemplateId),
    Column(ColumnId),
    Checks(CheckId),
    UnobservedFilterTypes,
    OutputKeys,
    Order,
}

/// Whether a step can run partitioned and keep the whole-table result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PartitionClass {
    Safe,
    CombineStep,
    Global,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effects {
    Pure,
    Handlers,
    Callbacks,
}

/// A resolution check failed: the bind is refused as internal.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ResolutionError {
    #[error("resolved plan capacity")]
    Capacity,
    /// The bound plan has a shape resolution does not represent.
    #[error("resolved plan unsupported: {0}")]
    Unsupported(String),
    /// A data edge reads a cell no earlier step completed.
    #[error("resolved plan reads before completion in region {region} at step {position} ({step:?}): {read}")]
    Topology {
        region: usize,
        position: usize,
        step: Step,
        read: String,
    },
}

impl DatasetPlan {
    /// Compile this bound plan into its resolved graph and check its
    /// topology. Binding calls this once on every plan it binds and returns
    /// the resolved plan in place of the bound one.
    pub(crate) fn resolve(&self) -> Result<ResolvedPlan, ResolutionError> {
        let resolved = build::resolve(self)?;
        verify::verify(&resolved)?;
        Ok(resolved)
    }
}

/// The plan is shared by concurrent runs (#2821).
const _: fn() = || {
    fn shared<T: Send + Sync>() {}
    shared::<ResolvedPlan>();
};

fn range(range: &Range<u32>) -> Range<usize> {
    range.start as usize..range.end as usize
}

impl ResolvedPlan {
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }
    pub fn region(&self, id: RegionId) -> &Region {
        &self.regions[id.index()]
    }
    pub fn sources(&self) -> &[SourceSlot] {
        &self.sources
    }
    pub fn columns(&self) -> &[ColumnNode] {
        &self.columns
    }
    pub fn column(&self, id: ColumnId) -> &ColumnNode {
        &self.columns[id.index()]
    }
    pub fn templates(&self) -> &[TemplateNode] {
        &self.templates
    }
    pub fn template(&self, id: TemplateId) -> &TemplateNode {
        &self.templates[id.index()]
    }
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
    pub fn binding(&self, id: BindingId) -> &Binding {
        &self.bindings[id.index()]
    }
    pub fn exprs(&self) -> &[ExprNode] {
        &self.exprs
    }
    pub fn expr(&self, id: ExprId) -> &ExprNode {
        &self.exprs[id.index()]
    }
    pub fn relations(&self) -> &[RelationNode] {
        &self.relations
    }
    pub fn relation(&self, id: RelationId) -> &RelationNode {
        &self.relations[id.index()]
    }
    pub fn windows(&self) -> &[WindowNode] {
        &self.windows
    }
    pub fn window(&self, id: WindowId) -> &WindowNode {
        &self.windows[id.index()]
    }
    pub fn checks(&self) -> &[CheckNode] {
        &self.checks
    }
    pub fn check(&self, id: CheckId) -> &CheckNode {
        &self.checks[id.index()]
    }
    /// Every handler site, in report order.
    pub fn handlers(&self) -> &[HandlerSite] {
        &self.handlers
    }
    pub fn handler(&self, id: HandlerSiteId) -> &HandlerSite {
        &self.handlers[id.index()]
    }
    /// Callback sites in the order the function port admits them.
    pub fn functions(&self) -> &[FunctionSlot] {
        &self.functions
    }
    /// Every step of every region; [`Self::region_schedule`] selects one region's.
    pub fn schedule(&self) -> &[StepInfo] {
        &self.schedule
    }
    pub fn keys(&self) -> &[ColumnId] {
        &self.keys
    }
    pub fn output_order(&self) -> &[OrderKey] {
        &self.output_order
    }
    /// The binding that completes `column` for `template`.
    pub fn cell(&self, template: TemplateId, column: ColumnId) -> Option<BindingId> {
        let node = self.template(template);
        let region = self.region(node.region);
        let width = region.columns.end - region.columns.start;
        let row = template.0.checked_sub(region.templates.start)?;
        let offset = column.0.checked_sub(region.columns.start)?;
        if offset >= width {
            return None;
        }
        self.cells[(region.cells + row * width + offset) as usize]
    }
    pub fn operands(&self, id: ExprId) -> &[Operand] {
        &self.operands[range(&self.expr(id).operands)]
    }
    pub fn reads(&self, id: ExprId) -> &[Read] {
        &self.reads[range(&self.expr(id).reads)]
    }
    pub fn sites(&self, id: ExprId) -> &[NodeSite] {
        &self.node_sites[range(&self.expr(id).sites)]
    }
    pub fn check_reads(&self, id: CheckId) -> &[Read] {
        &self.reads[range(&self.check(id).reads)]
    }
    pub fn region_schedule(&self, id: RegionId) -> &[StepInfo] {
        &self.schedule[range(&self.region(id).schedule)]
    }
    pub fn region_templates(&self, id: RegionId) -> impl Iterator<Item = TemplateId> {
        range(&self.region(id).templates).map(|index| TemplateId(index as u32))
    }
    pub fn region_columns(&self, id: RegionId) -> impl Iterator<Item = ColumnId> {
        range(&self.region(id).columns).map(|index| ColumnId(index as u32))
    }
    pub fn region_sources(&self, id: RegionId) -> impl Iterator<Item = SourceId> {
        range(&self.region(id).sources).map(|index| SourceId(index as u32))
    }
    pub fn region_relations(&self, id: RegionId) -> impl Iterator<Item = RelationId> {
        range(&self.region(id).relations).map(|index| RelationId(index as u32))
    }
    pub fn region_bindings(&self, id: RegionId) -> impl Iterator<Item = BindingId> {
        range(&self.region(id).bindings).map(|index| BindingId(index as u32))
    }
    /// A template's row bindings, in resolved dependency order.
    pub fn row_bindings(&self, id: TemplateId) -> impl Iterator<Item = BindingId> {
        range(&self.template(id).row_bindings).map(|index| BindingId(index as u32))
    }
    /// A column's column-phase bindings, in the order its step runs them:
    /// each column binding of the bound plan in turn, in the order of the
    /// first template each serves (#2829).
    pub fn column_bindings(&self, id: ColumnId) -> impl Iterator<Item = BindingId> {
        range(&self.column(id).column_bindings).map(|index| BindingId(index as u32))
    }
    /// The conversion handler sites a region declares, in declared order.
    pub fn conversions(&self, id: RegionId) -> &[HandlerSiteId] {
        &self.region(id).conversions
    }
    /// How many secondary ports of the dataset follow its inputs: one for
    /// each pair relation, which the run fills before any row is built
    /// (LOOKUP-27).
    pub fn pair_ports(&self) -> usize {
        self.region_relations(RegionId::DATASET)
            .filter(|&relation| {
                self.relation(relation)
                    .named
                    .as_ref()
                    .is_some_and(|item| item.source.pairs().is_some())
            })
            .count()
    }
    /// A region's named intermediate `index`: named intermediates keep their
    /// plan positions as the region's first relation slots.
    /// Returns `None` when the index does not name an intermediate in that region.
    pub fn named(&self, region: RegionId, index: usize) -> Option<RelationId> {
        let relations = &self.region(region).relations;
        let id = relations.start.checked_add(u32::try_from(index).ok()?)?;
        (id < relations.end
            && matches!(
                self.relations[id as usize].kind,
                RelationKind::Named { index: slot } if slot as usize == index
            ))
        .then_some(RelationId(id))
    }
    /// A region's column-phase columns, in the compiler's order.
    pub fn column_order(&self, id: RegionId) -> &[ColumnId] {
        &self.region(id).column_order
    }
    /// Whether a dataset template reads the driver input record by record:
    /// it is driven neither by a secondary input nor by a named
    /// intermediate, so its rows divide with the input's partitions (#3697).
    pub fn reads_driver(&self, id: TemplateId) -> bool {
        let driver = &self.template(id).driver;
        driver.named.is_none()
            && self.region_sources(RegionId::DATASET).next() == Some(driver.source)
    }
}
