//! Pure dataset declarations and admission. No study data or host effects.
use super::*;
use crate::value::ColumnType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordMatchKey {
    pub source_column: usize,
    pub driver_column: usize,
    pub identifier: String,
}
/// Immutable normalized source schemas, independent of any host buffers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSchemas {
    pub primary: TableSchema,
    pub secondary: Vec<SecondarySource>,
}

/// Eager equality checks over the complete filtered donor relation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntermediateUnique {
    pub path: String,
    pub columns: Vec<usize>,
    pub identity: Option<String>,
}
impl IntermediateUnique {
    pub fn diagnostic(
        &self,
        item: &Intermediate,
        schema: &TableSchema,
        dataset: &str,
        duplicate_count: usize,
    ) -> crate::diagnostic::Diagnostic {
        use crate::diagnostic::{ConditionCode, Diagnostic};
        use crate::json::Value as JsonValue;
        let mut context = std::collections::BTreeMap::from([
            (
                "intermediate".into(),
                JsonValue::String(item.identifier.clone()),
            ),
            ("dataset".into(), JsonValue::String(dataset.into())),
            (
                "columns".into(),
                JsonValue::Array(
                    self.columns
                        .iter()
                        .map(|&column| JsonValue::String(schema.columns()[column].name.clone()))
                        .collect(),
                ),
            ),
            ("duplicate_count".into(), JsonValue::from(duplicate_count)),
        ]);
        if let Some(identity) = &self.identity {
            context.insert(
                "verification_id".into(),
                JsonValue::String(identity.clone()),
            );
        }
        Diagnostic {
            code: ConditionCode::IntermediateDuplicateRecords,
            spec_paths: vec![self.path.clone()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}

/// A donor plan evaluates values in declaration order before filter/key/order.
#[derive(Clone, Debug, PartialEq)]
pub struct IntermediateDerivations {
    pub plan: std::sync::Arc<DatasetPlan>,
    pub current: Vec<CurrentField>,
    pub stored_columns: usize,
    /// Explicitly typed results convert before completing their donor slot.
    pub typed_columns: Vec<usize>,
}
impl IntermediateDerivations {
    /// A field's type: a stored field's, a typed derivation's declared type,
    /// or an untyped derivation's one result type (EXPR-14), and none for a
    /// result type decided per value.
    pub fn column_kind(&self, column: usize) -> Option<ColumnType> {
        self.plan.output_kind(column)
    }
}

/// A named secondary record selector with literal absence handling.
#[derive(Clone, Debug, PartialEq)]
pub struct Intermediate {
    pub identifier: String,
    pub path: String,
    pub source: IntermediateSource,
    pub keys: Vec<IntermediateKey>,
    pub filter: Option<RelationPredicate>,
    pub selection: Option<SourceSelection>,
    pub range: Option<RelationRange>,
    /// Visible selected fields; donor keys, filters and order still see every slot.
    pub columns: Option<Vec<usize>>,
    /// The declared `no_match` literal; `None` answers absence with missing.
    /// Either way the answer is counted under `no_match` (LOOKUP-24).
    pub no_match: Option<Value>,
    /// `require_match: true` fails an absence as `unmatched_key` instead;
    /// admission refuses it beside a declared `no_match`.
    pub require_match: bool,
    pub unique: Vec<IntermediateUnique>,
    pub derivations: Option<IntermediateDerivations>,
    /// Retained compilation metadata for a rejected augmentation; never executable.
    pub pending_schema: Option<TableSchema>,
}

impl Intermediate {
    pub fn current_fields(&self) -> impl Iterator<Item = &CurrentField> + '_ {
        self.filter
            .iter()
            .flat_map(|filter| filter.current())
            .chain(self.range.iter().map(|range| range.current()))
            .chain(
                self.derivations
                    .iter()
                    .flat_map(|derived| derived.current.iter()),
            )
    }
    pub fn admits_column(&self, column: usize) -> bool {
        (self
            .pending_schema
            .as_ref()
            .is_some_and(|schema| column < schema.columns().len())
            || self.admits_donor_column(column))
            && self
                .columns
                .as_ref()
                .is_none_or(|columns| columns.contains(&column))
    }
    pub(crate) fn admits_donor_column(&self, column: usize) -> bool {
        self.source.admits_column(column)
            || self.derivations.as_ref().is_some_and(|derived| {
                column >= derived.stored_columns && column < derived.plan.output().columns().len()
            })
    }
    pub fn output_dependencies(&self) -> impl Iterator<Item = usize> + '_ {
        self.keys
            .iter()
            .flat_map(|key| key.value.reads())
            .filter_map(MatchRead::output_column)
            .chain(
                self.filter
                    .iter()
                    .flat_map(|filter| filter.current())
                    .filter_map(|field| match field.read {
                        CurrentRead::Output(column) => Some(column),
                        _ => None,
                    }),
            )
            .chain(
                self.range
                    .iter()
                    .filter_map(|range| match range.current().read {
                        CurrentRead::Output(column) => Some(column),
                        _ => None,
                    }),
            )
    }
    pub fn has_expression_keys(&self) -> bool {
        self.keys.iter().any(|key| key.value.is_expression())
    }
    /// Whether derivations that read the current record evaluate only for the
    /// donor records whose stored keys match the current row (LOOKUP-15). A
    /// key or `between` bound on a derived field, or a window among the
    /// derivations, reads every record's derived values, so every record is
    /// augmented; so it is without keys, where every record matches.
    pub fn derives_matched_records(&self) -> bool {
        let Some(derived) = &self.derivations else {
            return false;
        };
        let stored = |column: usize| column < derived.stored_columns;
        !derived.current.is_empty()
            && !self.keys.is_empty()
            && self.keys.iter().all(|key| stored(key.source_column))
            && self.range.as_ref().is_none_or(|range| {
                range
                    .lower()
                    .into_iter()
                    .chain(range.upper())
                    .all(|bound| stored(bound.column))
            })
            && !derived
                .plan
                .columns()
                .any(|assignment| contains_window(&assignment.expression))
    }
}

/// Whether an expression is, or holds, a window over the donor records.
fn contains_window(expression: &Expression) -> bool {
    match expression {
        Expression::Window(_) | Expression::ContextWindow(_) => true,
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::Round { source, .. } => contains_window(&source.expression),
        Expression::Extreme { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => operands
            .iter()
            .any(|operand| contains_window(&operand.expression)),
        Expression::FirstAvailable(selection) => selection.sources().iter().any(|source| {
            matches!(&source.read, SelectionRead::Expression(read) if contains_window(&read.expression))
        }),
        Expression::Case(branches) => branches
            .iter()
            .any(|branch| contains_window(&branch.assignment.expression)),
        // Preparation refuses a window in a derive binding; a hand-built
        // plan that holds one still keeps every record.
        Expression::Aggregate(aggregate) => aggregate
            .derivations()
            .iter()
            .any(|binding| contains_window(&binding.assignment.expression)),
        _ => false,
    }
}

/// The handlers a read of a named intermediate counts at the reading path:
/// `multiple_matches` when `keep` chose among several candidates
/// (LOOKUP-10), and `no_match` when no candidate survived and no
/// `require_match` fails the read instead (LOOKUP-24).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadHandlers {
    pub multiple_matches: bool,
    pub no_match: bool,
}

/// The handlers a read of each intermediate counts. An intermediate its
/// `between` value, filter or derivation operands read answers uncounted, as
/// in a predicate, so only the read's own answer counts (COLUMN-14).
pub fn read_handlers(intermediates: &[Intermediate]) -> Vec<ReadHandlers> {
    intermediates
        .iter()
        .map(|item| ReadHandlers {
            multiple_matches: item.selection.is_some(),
            no_match: !item.require_match,
        })
        .collect()
}

/// Kept current-side reads carry the dependencies of the earlier selector too.
/// The lookup borrows either a complete plan or still-binding optional declarations.
pub fn visit_output_dependencies<'a>(
    index: usize,
    lookup: impl Fn(usize) -> Option<&'a Intermediate>,
    mut visitor: impl FnMut(usize),
) -> Result<(), PlanError> {
    fn visit<'a>(
        index: usize,
        lookup: &impl Fn(usize) -> Option<&'a Intermediate>,
        visitor: &mut impl FnMut(usize),
        depth: usize,
    ) -> Result<(), PlanError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PlanError::InvalidIntermediate);
        }
        let item = lookup(index).ok_or(PlanError::InvalidIntermediate)?;
        for column in item.output_dependencies() {
            visitor(column);
        }
        for field in item.current_fields() {
            if let CurrentRead::Intermediate { index: child, .. } = field.read {
                if child >= index {
                    return Err(PlanError::InvalidIntermediate);
                }
                visit(child, lookup, visitor, depth + 1)?;
            }
        }
        Ok(())
    }
    visit(index, &lookup, &mut visitor, 0)
}

/// Bind declarations globally; completed-value availability is checked at each read.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate(
    items: &[Intermediate],
    sources: &[SecondarySource],
    primary: &TableSchema,
    primary_kind: &impl Fn(usize) -> Option<ColumnType>,
    output: &TableSchema,
    output_kind: &impl Fn(usize) -> Option<ColumnType>,
    templates: &[RowTemplate],
) -> Result<(), PlanError> {
    for (index, item) in items.iter().enumerate() {
        if item.pending_schema.is_some()
            || item.identifier.is_empty()
            || item.path.is_empty()
            || items[..index]
                .iter()
                .any(|other| other.identifier == item.identifier || other.path == item.path)
        {
            return Err(PlanError::InvalidIntermediate);
        }
        let donor = item
            .source_schema(sources)
            .ok_or(PlanError::InvalidIntermediate)?;
        if let Some(derived) = &item.derivations {
            if derived.stored_columns > donor.columns().len() {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if matches!(&item.source,IntermediateSource::SelfRows {columns,..} if columns.is_empty()) {
            return Err(PlanError::InvalidIntermediate);
        }
        let driver = templates.iter().any(|template| {
            matches!(template.mode, RowMode::IntermediateDriver { index: driver, .. } if driver == index)
        });
        if !item.keys.is_empty() || !driver {
            match_values::validate_declaration(
                &item.keys,
                primary,
                sources,
                donor,
                &|column| item.source_kind(sources, column),
                output,
                &vec![true; output.columns().len()],
                &|read, physical| match read {
                    MatchRead::Output { column, .. } => output_kind(*column),
                    MatchRead::Record { column, .. } => primary_kind(*column),
                    MatchRead::SecondaryRecord { .. } => Some(physical),
                },
            )?;
        }
        let width = item
            .source_schema(sources)
            .ok_or(PlanError::InvalidIntermediate)?
            .columns()
            .len();
        if item.columns.as_ref().is_some_and(|columns| {
            columns.iter().enumerate().any(|(position, &column)| {
                column >= width
                    || !item.admits_donor_column(column)
                    || columns[..position].contains(&column)
            })
        }) {
            return Err(PlanError::InvalidIntermediate);
        }
        if let Some(range) = &item.range {
            range
                .validate_source(
                    item.source_schema(sources)
                        .ok_or(PlanError::InvalidIntermediate)?,
                    &|column| item.source_kind(sources, column).map(Into::into),
                )
                .map_err(|_| PlanError::InvalidIntermediate)?;
            range
                .current()
                .validate(primary, sources, output, items, index)?;
            let current_kind = match range.current().read {
                CurrentRead::Record { source: 0, column } => primary_kind(column),
                CurrentRead::Record { source, column } => {
                    Some(sources[source - 1].schema.columns()[column].kind)
                }
                CurrentRead::Output(column) => output_kind(column),
                CurrentRead::Intermediate { index, column } => {
                    items[index].source_kind(sources, column)
                }
            };
            if range
                .value_kind()
                .zip(current_kind)
                .is_some_and(|(value, current)| value != current.into())
            {
                return Err(PlanError::InvalidIntermediate);
            }
            if [range.lower(), range.upper()]
                .into_iter()
                .flatten()
                .any(|bound| !item.admits_donor_column(bound.column))
            {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if let IntermediateSource::Primary { schema, name } = &item.source {
            if schema != primary || name.is_empty() {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if let IntermediateSource::SelfRows { schema, columns } = &item.source {
            if schema != output
                || columns
                    .iter()
                    .enumerate()
                    .any(|(index, &column)| column >= width || columns[..index].contains(&column))
            {
                return Err(PlanError::InvalidIntermediate);
            }
            if item.keys.iter().any(|key| !item.admits_donor_column(key.source_column))
                || item.filter.as_ref().is_some_and(|filter| filter.bindings().iter().any(|binding| matches!(binding.read, Read::Source(column) if !item.admits_donor_column(column))))
                || item.selection.as_ref().is_some_and(|selection| selection.order_by.iter().any(|term| !item.admits_donor_column(term.column))) {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        for check in &item.unique {
            if check.path.is_empty()
                || check.columns.is_empty()
                || check
                    .columns
                    .iter()
                    .any(|&column| column >= width || !item.admits_donor_column(column))
            {
                return Err(PlanError::InvalidIntermediate);
            }
        }
        if !item.unique.is_empty()
            && item
                .filter
                .as_ref()
                .is_some_and(RelationPredicate::correlated)
        {
            return Err(PlanError::InvalidIntermediate);
        }
        if let Some(filter) = &item.filter {
            filter.validate(width, primary, sources, output, items, index)?;
        }
        if item.selection.as_ref().is_some_and(|selection| {
            selection.order_by.is_empty()
                || selection.order_by.iter().any(|term| term.column >= width)
        }) {
            return Err(PlanError::InvalidSourceOrder);
        }
    }
    Ok(())
}

pub(super) fn validate_read(
    item: &Intermediate,
    column: usize,
    primary: &TableSchema,
    sources: &[SecondarySource],
    available: &[bool],
    output: &TableSchema,
) -> Result<(), PlanError> {
    let source = item
        .source_schema(sources)
        .ok_or(PlanError::InvalidIntermediate)?;
    if column >= source.columns().len() || !item.admits_column(column) {
        return Err(PlanError::InvalidIntermediate);
    }
    match_values::validate(
        &item.keys,
        primary,
        sources,
        source,
        // Key kinds were checked once with the owning plan's inferred metadata.
        // A read checks scope and availability without promoting layout placeholders.
        &|_| None,
        output,
        available,
    )
}

/// Direct scalar selections use only admitted complete records or completed keys.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_context_read(
    item: &Intermediate,
    column: usize,
    primary: &TableSchema,
    sources: &[SecondarySource],
    available: &[bool],
    output: &TableSchema,
    mode: &RowMode,
) -> Result<(), PlanError> {
    if !matches!(
        mode.local_mode(),
        RowMode::Records | RowMode::Keys | RowMode::Groups(_)
    ) && !item.source.is_self()
    {
        return Err(PlanError::InvalidIntermediate);
    }
    if let RowMode::Groups(keys) = mode.local_mode() {
        if item
            .keys
            .iter()
            .flat_map(|key| key.value.reads())
            .any(|read| matches!(read, MatchRead::Record { column, .. } if !keys.contains(column)))
        {
            return Err(PlanError::NonGroupSource);
        }
    }
    validate_read(item, column, primary, sources, available, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::{compile, Context};
    use crate::value::Value;

    fn assignment(expression: Expression) -> Assignment {
        Assignment {
            column: 0,
            expression,
            path: "intermediates[0].derivations.D".into(),
        }
    }

    fn window() -> Expression {
        Expression::Window(Window {
            kind: WindowKind::RowNumber,
            group_by: Vec::new(),
            order_by: Vec::new(),
            filter: None,
        })
    }

    /// An aggregate whose one derive binding evaluates `derivation`.
    fn aggregate(derivation: Expression) -> Expression {
        let bound = BoundAggregate::new(
            compile(
                "COUNT(D.*)",
                Context::Aggregate,
                "intermediates[0].derivations.N.aggregate.expr",
            )
            .unwrap(),
            AggregateRelation::CurrentGroup,
            Vec::new(),
            None,
            AggregatePhase::Derivation,
            None,
            "intermediates[0].derivations.N.aggregate",
        )
        .unwrap()
        .with_record_operations(
            None,
            vec![AggregateDerivation {
                assignment: assignment(derivation),
                kind: ColumnType::Int,
                handler: None,
                path: "intermediates[0].derivations.N.aggregate.derive.D".into(),
            }],
            None,
        );
        Expression::Aggregate(Box::new(bound))
    }

    /// A window anywhere among the derivations keeps every donor record
    /// augmented (LOOKUP-15): directly, in a case branch, or in an
    /// aggregate's derive binding, which preparation refuses but a
    /// hand-built plan can hold.
    #[test]
    fn a_window_is_found_wherever_a_derivation_nests_it() {
        let literal = || Expression::Literal(Value::Int(1));
        assert!(contains_window(&window()));
        assert!(!contains_window(&literal()));
        let case = |then: Expression| {
            Expression::Case(vec![CaseBranch {
                when: None,
                assignment: Box::new(assignment(then)),
            }])
        };
        assert!(contains_window(&case(window())));
        assert!(!contains_window(&case(literal())));
        assert!(contains_window(&aggregate(window())));
        assert!(!contains_window(&aggregate(literal())));
    }
}
