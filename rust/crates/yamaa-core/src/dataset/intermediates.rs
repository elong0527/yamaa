//! Pure dataset declarations. No study data or host effects.
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
    /// An `error` check fails the run; a `warning` one is a row of the run
    /// report and fails nothing (LOOKUP-17).
    pub severity: VerificationSeverity,
    /// The intermediate's position among those the specification declares,
    /// which orders its warning rows (CHECK-23).
    pub declared: usize,
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
        let mut context = crate::json::Map::from([
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
    /// A field's result type: [`Self::column_kind`]'s one type, or the
    /// types an untyped derivation decides per value (EXPR-14).
    pub fn column_type(&self, column: usize) -> crate::result_type::ResultType {
        self.plan.output_type(column)
    }
}

/// A selection by distance (LOOKUP-26): of the candidates, the one whose
/// `source` lies nearest the current row's reference, counted in the
/// declared unit as `diff` counts it (TIME-4). A tie goes to the earlier
/// `source`, then to stored order, and a candidate whose distance is missing
/// is never chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct Nearest {
    /// The donor column, stored or derived, whose type is `date` or
    /// `datetime`.
    pub source: usize,
    /// The current row's value, read as a key match value is: its
    /// `source_column` is [`Self::source`] and its path is the `to` field's.
    pub reference: IntermediateKey,
    /// The signed difference from a candidate's `source` to the reference,
    /// in the declared unit (TIME-4).
    pub difference: Temporal,
}

impl Nearest {
    /// The type of the source, which the reference shares.
    pub fn source_type(&self) -> crate::value::ValueType {
        match self.difference.kind() {
            TemporalKind::DateTimeDifference { .. } => crate::value::ValueType::DateTime,
            _ => crate::value::ValueType::Date,
        }
    }
    /// A candidate's distance to the reference: the absolute value of the
    /// difference from its `source` to `reference`, missing when either is
    /// missing.
    pub fn distance(
        &self,
        source: &Value,
        reference: &Value,
    ) -> Result<Value, crate::diagnostic::Diagnostic> {
        let step = self
            .difference
            .apply_values(&[source.clone(), reference.clone()])
            .expect("a difference reads two operands")?;
        Ok(match step {
            TemporalStep::Value(Value::Int(count)) => Value::Int(count.saturating_abs()),
            TemporalStep::Value(Value::Float(amount)) => Value::float(amount.get().abs()),
            TemporalStep::Value(_) | TemporalStep::Invalid => Value::Missing,
        })
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
    /// The selection by distance, which a declaration writes instead of
    /// `order_by` and `keep` (LOOKUP-26).
    pub nearest: Option<Nearest>,
    pub range: Option<RelationRange>,
    /// Visible selected fields; donor keys, filters and order still see every slot.
    pub columns: Option<Vec<usize>>,
    /// The declared absence answer, written `absent`; `None` answers absence
    /// with missing. Either way each read counts the answer under the
    /// `absent` handler at its path (LOOKUP-24, D202).
    pub no_match: Option<Value>,
    /// `require_match: true` fails an absence as `unmatched_key` instead;
    /// the compiler refuses it beside a declared absence answer.
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
            .chain(self.nearest.iter().map(|nearest| &nearest.reference))
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
        | Expression::StrSubstr { source, .. }
        | Expression::StrLength { source, .. }
        | Expression::StrDecimal { source, .. }
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
/// `multiple_matches` when `keep` or `nearest` chose among several
/// candidates (LOOKUP-10, LOOKUP-26), and `absent` when no candidate survived
/// and no `require_match` fails the read instead (LOOKUP-24, D202).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadHandlers {
    pub multiple_matches: bool,
    /// Whether the read counts its absence under `absent`; false when the
    /// read requires a match.
    pub absent: bool,
}

/// The handlers a read of each intermediate counts. An intermediate its
/// `between` value, filter or derivation operands read answers uncounted, as
/// in a predicate, so only the read's own answer counts (COLUMN-14).
pub fn read_handlers(intermediates: &[Intermediate]) -> Vec<ReadHandlers> {
    intermediates
        .iter()
        .map(|item| ReadHandlers {
            multiple_matches: item.selection.is_some() || item.nearest.is_some(),
            absent: !item.require_match,
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
