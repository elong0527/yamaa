//! Named intermediates under the lookup model (LOOKUP-25): match the records
//! whose keys equal the current row's match values and whose bounds hold its
//! range value, narrow them by the filter, select one, and answer absence.
//! One choice per row and intermediate is cached for every read of the row.
//!
//! Selection keeps today's semantics: a plain intermediate of at least 128
//! records is indexed over its full key tuple, every other shape scans its
//! donor records. #2842 replaces both with the join operator.
use super::buffer::Values;
use super::donor::{self, Donor};
use super::eval::owned;
use super::frame::{Frame, Identity};
use super::{Cx, Fail, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, KeyValue, Read, RegionId, RelationId, ResolvedPlan};
use crate::dataset::{
    CurrentField, CurrentRead, Intermediate, IntermediateSource, Keep, MatchRead, MatchValue,
    OrderTerm, RelationPredicate, SourceSelection,
};
use crate::expression::Truth;
use crate::numeric_lifecycle::HandlerKind;
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::value::{Value, ValueType};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

pub(super) use super::aggregates::{aggregate, Record};
pub(super) use super::joins::{join, joined_value, JoinSite};
pub(super) use super::odm::odm;

/// A row's match values: each key's donor column and the value it must equal.
pub(super) type MatchValues = Vec<(usize, Value)>;

/// One row's record choice: immutable donor coordinates and whether
/// several records competed.
#[derive(Clone, Copy, Debug)]
enum Choice {
    Record { row: usize, multiple: bool },
    Absent,
}

/// The read whose path counts a named read's handlers and locates its
/// nested reads: a node's own path, or an owner path such as a range's.
#[derive(Clone, Copy)]
pub(super) struct Site<'s> {
    pub expr: Option<ExprId>,
    pub path: &'s str,
}

impl<'s> Site<'s> {
    /// A node's own path.
    pub(super) fn node(plan: &'s ResolvedPlan, expr: ExprId) -> Self {
        Self {
            expr: Some(expr),
            path: &plan.expr(expr).path,
        }
    }
}

/// Run-local state of one named intermediate.
#[derive(Default)]
struct Named {
    /// Per record, whether a filter that reads only the donor records keeps
    /// it, decided the first time the record matches (LOOKUP-8).
    narrowed: Option<Vec<Option<bool>>>,
    index: Option<Option<KeyIndex>>,
    choices: HashMap<Identity, Choice>,
    materialized: HashMap<Option<Identity>, Rc<Donor>>,
}

/// A cache entry a batch unit added, removed if the unit replays.
enum Entry {
    Choice(RelationId, Identity),
    Materialized(RelationId, Option<Identity>),
}

/// The relation state of one region: fresh for every attempt, and for every
/// donor materialization.
pub(super) struct State {
    first: usize,
    named: Vec<Named>,
    journal: Vec<Entry>,
}

/// The journal position before a batch unit.
pub(super) struct Mark(usize);

impl State {
    pub(super) fn new(plan: &ResolvedPlan, region: RegionId) -> Self {
        let relations: Vec<RelationId> = plan.region_relations(region).collect();
        let first = relations.first().map_or(0, |relation| relation.index());
        let named = relations
            .iter()
            .take_while(|&&relation| plan.relation(relation).named.is_some())
            .map(|_| Named::default())
            .collect();
        Self {
            first,
            named,
            journal: Vec::new(),
        }
    }

    fn named(&mut self, relation: RelationId) -> &mut Named {
        &mut self.named[relation.index() - self.first]
    }

    pub(super) fn mark(&self) -> Mark {
        Mark(self.journal.len())
    }

    pub(super) fn restore(&mut self, mark: Mark) {
        while self.journal.len() > mark.0 {
            match self.journal.pop().expect("a journal entry") {
                Entry::Choice(relation, identity) => {
                    self.named(relation).choices.remove(&identity);
                }
                Entry::Materialized(relation, key) => {
                    self.named(relation).materialized.remove(&key);
                }
            }
        }
    }

    /// A donor materialized for a key, if any.
    pub(super) fn materialized(
        &mut self,
        relation: RelationId,
        key: Option<Identity>,
    ) -> Option<Rc<Donor>> {
        self.named(relation).materialized.get(&key).cloned()
    }

    pub(super) fn insert_materialized(
        &mut self,
        relation: RelationId,
        key: Option<Identity>,
        donor: Rc<Donor>,
    ) {
        self.named(relation).materialized.insert(key, donor);
        self.journal.push(Entry::Materialized(relation, key));
    }

    /// A key window observes a different relation after row templates
    /// finish: choices and correlated materializations from template-local
    /// windows cannot identify a choice in the complete output relation.
    pub(super) fn begin_columns(&mut self, plan: &ResolvedPlan) {
        let items: Vec<&Intermediate> = (0..self.named.len())
            .map(|index| {
                plan.relation(RelationId::try_from(self.first + index).expect("a slot"))
                    .named
                    .as_deref()
                    .expect("a named relation")
            })
            .collect();
        let windowed_key = |item: &Intermediate| {
            item.keys.iter().any(
                |key| matches!(&key.value, MatchValue::Expression(expression) if expression.windowed),
            )
        };
        if !items.iter().any(|item| windowed_key(item)) {
            return;
        }
        let mut affected: Vec<bool> = Vec::with_capacity(items.len());
        for item in &items {
            let mut windowed = windowed_key(item);
            for field in item.current_fields() {
                if let CurrentRead::Intermediate { index, .. } = field.read {
                    windowed |= affected.get(index).copied().unwrap_or(true);
                }
            }
            affected.push(windowed);
        }
        for (named, affected) in self.named.iter_mut().zip(affected) {
            if affected {
                *named = Named::default();
            }
        }
    }

    /// Every completed template changes only SELF eligibility and match choices.
    pub(super) fn publish_self(&mut self, plan: &ResolvedPlan) {
        for index in 0..self.named.len() {
            let relation = RelationId::try_from(self.first + index).expect("a slot");
            let item = plan
                .relation(relation)
                .named
                .as_deref()
                .expect("a named relation");
            if item.source.is_self() {
                let named = &mut self.named[index];
                named.narrowed = None;
                named.choices.clear();
                named.materialized.clear();
            }
        }
    }
}

/// The bound declaration of a named relation.
pub(super) fn item(plan: &ResolvedPlan, relation: RelationId) -> &Intermediate {
    plan.relation(relation)
        .named
        .as_deref()
        .expect("a named relation holds its declaration")
}

/// The dataset a named intermediate reads, as diagnostics name it.
fn source_name<'i>(
    item: &'i Intermediate,
    secondary: &'i [crate::dataset::SecondarySource],
) -> &'i str {
    match &item.source {
        IntermediateSource::Input(index) => &secondary[*index].name,
        IntermediateSource::Primary { name, .. } => name,
        IntermediateSource::SelfRows { .. } => "SELF",
    }
}

/// The table a named intermediate's records come from.
fn source_table<'t, E>(item: &Intermediate, cx: &Cx<'t, E>) -> Table<'t, E> {
    match &item.source {
        IntermediateSource::Input(index) => cx.secondary[*index],
        IntermediateSource::Primary { .. } => cx.primary,
        IntermediateSource::SelfRows { .. } => Table::Rows {
            frame: cx.frame,
            schema: cx.schema,
            start: 0,
            rows: cx.self_rows,
        },
    }
}

/// Preserve each opaque donor read and its source coordinates.
pub(super) fn cell<'t, E>(
    table: Table<'t, E>,
    item: &Intermediate,
    row: usize,
    column: usize,
) -> Result<ValueRef<'t>, Fail<E>> {
    table.get(row, column).map_err(|error| {
        Box::new(match item.source.input_index() {
            Some(source) => ExecutionError::SecondaryCell {
                path: item.path.clone(),
                source,
                source_row: row,
                error,
            },
            None if matches!(item.source, IntermediateSource::Primary { .. }) => {
                ExecutionError::Cell {
                    path: item.path.clone(),
                    source_row: row,
                    error,
                }
            }
            None => ExecutionError::SchemaMismatch,
        })
    })
}

pub(super) fn check_deferred_filter_types<E>(item: &Intermediate) -> Result<(), Fail<E>> {
    let findings = item
        .filter
        .as_ref()
        .map(|filter| filter.deferred_type_findings())
        .unwrap_or(&[]);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(Box::new(ExecutionError::IntermediateFilterTypes(
            findings.to_vec(),
        )))
    }
}

/// A selector hidden by an empty input or a lazy branch still reports its
/// known filter-type refusal before a successful result can publish.
pub(super) fn check_unobserved_filter_types<E>(plan: &ResolvedPlan) -> Result<(), Fail<E>> {
    for relation in plan.region_relations(RegionId::DATASET) {
        if let Some(item) = plan.relation(relation).named.as_deref() {
            check_deferred_filter_types(item)?;
        }
    }
    Ok(())
}

/// Whether a filter that reads only the donor records keeps one of them.
fn keeps<E>(
    table: Table<'_, E>,
    filter: &RelationPredicate,
    source_row: usize,
) -> Result<bool, Fail<E>> {
    super::predicates::row_truth(filter.predicate(), table, source_row, |_| ValueRef::Missing)
        .map(|truth| truth == Truth::True)
        .map_err(|error| {
            Box::new(ExecutionError::Predicate {
                source_row,
                error: *error,
            })
        })
}

/// Every donor record a filter that reads only the donor records keeps.
fn eligibility<E>(table: Table<'_, E>, item: &Intermediate) -> Result<Vec<usize>, Fail<E>> {
    check_deferred_filter_types(item)?;
    let mut eligible = Vec::with_capacity(table.row_count());
    for source_row in 0..table.row_count() {
        if let Some(filter) = item.filter.as_ref().filter(|filter| !filter.correlated()) {
            if !keeps(table, filter, source_row)? {
                continue;
            }
        }
        eligible.push(source_row);
    }
    Ok(eligible)
}

/// A table restricted to some of its records.
struct Filtered<'t, E> {
    table: Table<'t, E>,
    rows: &'t [usize],
}
impl<E> TableAccess for Filtered<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.table.schema()
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.table.cell(self.rows[row], column)
    }
}

/// A named driver's eligible records, grouped by the template's group fields.
pub(super) fn driver_groups<E>(
    table: Table<'_, E>,
    eligible: &[usize],
    keys: &[usize],
) -> Result<Vec<Vec<usize>>, Fail<E>> {
    let filtered = Filtered {
        table,
        rows: eligible,
    };
    crate::table_grouping::partition(&filtered, keys)
        .map(|groups| {
            groups
                .into_iter()
                .map(|group| group.into_iter().map(|row| eligible[row]).collect())
                .collect()
        })
        .map_err(|error| Box::new(ExecutionError::Grouping(error)))
}

/// A template's driver relation: the donor records a named driver
/// materializes and the records its filter keeps.
pub(super) struct Driver {
    pub materialized: Option<Rc<Donor>>,
    pub eligible: Option<Vec<usize>>,
}

/// Row drivers consume the complete source-only donor set without executing a match.
pub(super) fn driver<'a, E>(
    run: &mut Run<'_, 'a, E>,
    template: crate::dataset::resolved::TemplateId,
    frame: &Frame,
    secondary: &[Table<'a, E>],
) -> Result<Driver, Fail<E>> {
    let plan = run.plan;
    let node = plan.template(template);
    let Some(relation) = node.driver.named else {
        return Ok(Driver {
            materialized: None,
            eligible: None,
        });
    };
    let cx = run.probe_cx(frame, node.region, secondary);
    let item = item(plan, relation);
    let prepared = donor::materialize(run, &cx, relation, None, None)?;
    let table = match &prepared {
        Some(data) => data.table(),
        None => match &item.source {
            IntermediateSource::Input(index) => secondary[*index],
            IntermediateSource::Primary { .. } => Table::Source(run.primary),
            IntermediateSource::SelfRows { .. } => {
                return Err(Box::new(ExecutionError::SchemaMismatch))
            }
        },
    };
    let eligible = eligibility(table, item)?;
    Ok(Driver {
        materialized: prepared,
        eligible: Some(eligible),
    })
}

/// Intermediate `unique` verifications. Source-only checks are eager even
/// when no output expression reads the selector; the check narrows every
/// record, and a later match reuses each answer.
pub(super) fn verify<'a, E>(
    run: &mut Run<'_, 'a, E>,
    frame: &Frame,
    secondary: &[Table<'a, E>],
    self_only: bool,
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let region = RegionId::DATASET;
    let mut failures = Vec::new();
    let relations: Vec<RelationId> = plan
        .region_relations(region)
        .filter(|&relation| plan.relation(relation).named.is_some())
        .collect();
    for relation in relations {
        let item = item(plan, relation);
        if item.source.is_self() != self_only || item.unique.is_empty() {
            continue;
        }
        let mut cx = run.probe_cx(frame, region, secondary);
        cx.self_rows = frame.rows();
        let prepared = donor::materialize(run, &cx, relation, None, None)?;
        let table = match &prepared {
            Some(data) => data.table(),
            None => source_table(item, &cx),
        };
        let name = source_name(item, &plan.region(region).secondary);
        let eligible = eligibility(table, item)?;
        let filtered = Filtered {
            table,
            rows: &eligible,
        };
        for check in &item.unique {
            let mut columns = Vec::new();
            for &column in &check.columns {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
            let groups = crate::table_grouping::partition(&filtered, &columns)
                .map_err(|error| Box::new(ExecutionError::Grouping(error)))?;
            let count = groups.iter().filter(|group| group.len() > 1).count();
            if count != 0 {
                failures.push(check.diagnostic(item, table.schema(), name, count));
            }
        }
        let mut narrowed = vec![Some(false); table.row_count()];
        for row in eligible {
            narrowed[row] = Some(true);
        }
        run.relations.named(relation).narrowed = Some(narrowed);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Box::new(ExecutionError::IntermediateVerifications(
            failures,
        )))
    }
}

/// An intermediate read node over rows.
pub(super) fn read<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
    observe: bool,
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Read::Relation(relation, column) = plan.reads(expr)[0] else {
        unreachable!("an intermediate node reads its relation")
    };
    let mut values = Values::missing(0);
    for &row in rows {
        let value = named_value(
            run,
            cx,
            Site::node(plan, expr),
            relation,
            column as usize,
            row,
            observe,
        )?;
        values.push_value(value);
    }
    Ok(values)
}

/// One row's read of a named intermediate's column. `owner` is the node
/// whose path counts the read's handlers and locates nested reads; a
/// predicate read counts nothing.
pub(super) fn named_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    relation: RelationId,
    column: usize,
    row: usize,
    observe: bool,
) -> Result<Value, Fail<E>> {
    // An aggregate derivation keeps its donor record apart from the current
    // row: a named, kept selection reads the enclosing row on the primary.
    if cx.record.is_some() {
        let enclosing = Cx {
            record: None,
            table: cx.primary,
            ..cx.clone()
        };
        return named_value(run, &enclosing, site, relation, column, row, observe);
    }
    let plan = run.plan;
    let item = item(plan, relation);
    let identity = cx.frame.identity(row);
    let cached = run
        .relations
        .named(relation)
        .choices
        .get(&identity)
        .copied();
    // Derivations that read the current record augment only the records the
    // match values select (LOOKUP-15), so those values resolve first.
    let (prepared, keys) = if cached.is_none() && item.derives_matched_records() {
        let (data, keys) = donor::materialize_matched(run, cx, relation, site, row)?;
        (Some(data), Some(keys))
    } else {
        (
            donor::materialize(run, cx, relation, Some(site), Some(row))?,
            None,
        )
    };
    let table = match &prepared {
        Some(data) => data.table(),
        None => source_table(item, cx),
    };
    let secondary = &plan.region(cx.region).secondary;
    let name = source_name(item, secondary);
    let choice = match cached {
        Some(choice) => choice,
        None => resolve_choice(
            run, cx, site, relation, item, table, name, row, identity, keys,
        )?,
    };
    let (value, handler) = match choice {
        Choice::Record { row, multiple } => (
            owned(cell(table, item, row, column)?),
            multiple.then_some(HandlerKind::MultipleMatches),
        ),
        // The declared literal, or missing by default; counted either way.
        Choice::Absent => (
            item.no_match.clone().unwrap_or(Value::Missing),
            Some(HandlerKind::NoMatch),
        ),
    };
    if let Some(handler) = handler.filter(|_| observe) {
        run.fire_at(
            site.expr,
            &format!("{}.{}", site.path, handler.name()),
            handler,
        )?;
    }
    Ok(value)
}

/// Retain exact key values separately from output-row identity in join diagnostics.
fn matched_key(
    table: &dyn TableAccess<Error = impl Sized>,
    keys: &[(usize, Value)],
) -> Vec<(String, Value)> {
    keys.iter()
        .map(|(column, value)| {
            (
                table.schema().columns()[*column].name.clone(),
                value.clone(),
            )
        })
        .collect()
}

/// Resolve and cache the record choice for one read: match the records on
/// the resolved keys and range, narrow them by the filter, then select.
#[allow(clippy::too_many_arguments)]
fn resolve_choice<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    relation: RelationId,
    item: &Intermediate,
    table: Table<'_, E>,
    source_name: &str,
    row: usize,
    identity: Identity,
    keys: Option<Vec<(usize, Value)>>,
) -> Result<Choice, Fail<E>> {
    let plan = run.plan;
    let secondary = &plan.region(cx.region).secondary;
    let keys = match keys {
        Some(keys) => keys,
        None => match_values(run, cx, relation, item, row)?,
    };
    let range_value = match &item.range {
        Some(range) => Some(current_value(
            run,
            cx,
            site,
            range.current(),
            range.path(),
            Some(row),
        )?),
        None => None,
    };
    let probe_keys: Vec<(usize, ValueRef<'_>)> = keys
        .iter()
        .map(|(column, value)| (*column, ValueRef::from(value)))
        .collect();
    let kinds: Vec<Option<ValueType>> = keys
        .iter()
        .map(|(column, _)| item.source_kind(secondary, *column).map(Into::into))
        .collect();
    let probe = Probe {
        keys: &probe_keys,
        kinds: &kinds,
        range: item
            .range
            .as_ref()
            .zip(range_value.as_ref().map(ValueRef::from)),
    };
    let mut count = 0;
    let mut selected = None;
    if !probe.incomplete() {
        let use_index = table.row_count() >= MINIMUM_INDEXED_ROWS
            && !item.keys.is_empty()
            && !item.has_expression_keys()
            && !item.source.is_self()
            && item.range.is_none()
            && item.derivations.is_none()
            && item
                .filter
                .as_ref()
                .is_none_or(|filter| !filter.correlated())
            && keys.iter().all(|(column, expected)| {
                match (expected.value_type(), item.source_kind(secondary, *column)) {
                    (Some(expected), Some(kind)) => {
                        crate::value::comparable_types(expected, kind.into())
                    }
                    (Some(_), None) => true,
                    _ => false,
                }
            });
        let mut indexed = None;
        if use_index {
            if run.relations.named(relation).index.is_none() {
                let built = KeyIndex::build(table, item)?;
                run.relations.named(relation).index = Some(built);
            }
            if let Some(Some(index)) = &run.relations.named(relation).index {
                indexed = Some(index.matching(&keys).to_vec());
            }
        }
        let bucket = Probe {
            keys: &[],
            kinds: &[],
            range: None,
        };
        let (candidates_rows, probe_used): (Vec<usize>, &Probe<'_>) = match indexed {
            Some(rows) => (rows, &bucket),
            None => ((0..table.row_count()).collect(), &probe),
        };
        // A filter that reads only the donor records keeps one answer per
        // record, unless the records are derived for this current record.
        let reusable = item
            .derivations
            .as_ref()
            .is_none_or(|derived| derived.current.is_empty());
        let found = candidates(
            candidates_rows.into_iter(),
            probe_used,
            |source_row, column| cell(table, item, source_row, column),
            |source_row, run: &mut Run<'_, '_, E>| {
                let Some(filter) = &item.filter else {
                    return Ok(true);
                };
                if filter.correlated() {
                    return Ok(
                        correlated(run, cx, site, filter, table, item, source_row, row)?
                            == Truth::True,
                    );
                }
                if reusable {
                    if let Some(Some(kept)) = run
                        .relations
                        .named(relation)
                        .narrowed
                        .as_ref()
                        .and_then(|narrowed| narrowed.get(source_row))
                    {
                        return Ok(*kept);
                    }
                }
                let kept = keeps(table, filter, source_row)?;
                if reusable {
                    let rows = table.row_count();
                    let narrowed = run
                        .relations
                        .named(relation)
                        .narrowed
                        .get_or_insert_with(|| vec![None; rows]);
                    if let Some(slot) = narrowed.get_mut(source_row) {
                        *slot = Some(kept);
                    }
                }
                Ok(kept)
            },
            run,
        )
        .map_err(|found| refusal(found, item, table, &keys))?;
        count = found.rows.len();
        selected = found.rows.first().copied();
        if count > 1 {
            if let Some(selection) = &item.selection {
                selected = Some(select(&found.rows, selection, |source_row, column| {
                    cell(table, item, source_row, column)
                })?);
            } else {
                return Err(Box::new(ExecutionError::MultipleMatches {
                    path: item.path.clone(),
                    dataset: source_name.into(),
                    intermediate: item.identifier.clone(),
                    match_count: count,
                    matched_key: matched_key(&table, &keys),
                    identity: run.failure_identity(cx, row),
                }));
            }
        }
    }
    let choice = if let Some(row) = selected {
        Choice::Record {
            row,
            multiple: count > 1,
        }
    } else if !item.require_match {
        Choice::Absent
    } else {
        return Err(Box::new(ExecutionError::UnmatchedKey {
            path: item.path.clone(),
            dataset: source_name.into(),
            intermediate: item.identifier.clone(),
            matched_key: matched_key(&table, &keys),
            identity: run.failure_identity(cx, row),
        }));
    };
    run.relations
        .named(relation)
        .choices
        .insert(identity, choice);
    run.relations
        .journal
        .push(Entry::Choice(relation, identity));
    Ok(choice)
}

/// The current row's match values: the current references of expression
/// match values first, then every key.
pub(super) fn match_values<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    item: &Intermediate,
    row: usize,
) -> Result<MatchValues, Fail<E>> {
    if item.has_expression_keys() {
        let current = resolve_operands(run, cx, &item.path, &item.keys, row)?;
        check_deferred_filter_types(item)?;
        evaluate_keys(run, cx, relation, &item.keys, &current, row)
    } else {
        check_deferred_filter_types(item)?;
        ordinary_keys(run, cx, &item.path, &item.keys, row)
    }
}

/// The records of a named intermediate's source whose stored keys equal the
/// match values, in stored order. A key pair whose types cannot compare is
/// refused as the match would refuse it, on the same record.
pub(super) fn matched_records<E>(
    plan: &ResolvedPlan,
    cx: &Cx<'_, E>,
    item: &Intermediate,
    keys: &[(usize, Value)],
) -> Result<Vec<usize>, Fail<E>> {
    let secondary = &plan.region(cx.region).secondary;
    let table = source_table(item, cx);
    let probe_keys: Vec<(usize, ValueRef<'_>)> = keys
        .iter()
        .map(|(column, value)| (*column, ValueRef::from(value)))
        .collect();
    let kinds: Vec<Option<ValueType>> = keys
        .iter()
        .map(|(column, _)| item.source_kind(secondary, *column).map(Into::into))
        .collect();
    let probe = Probe {
        keys: &probe_keys,
        kinds: &kinds,
        range: None,
    };
    if probe.incomplete() {
        return Ok(Vec::new());
    }
    candidates(
        0..table.row_count(),
        &probe,
        |source_row, column| cell(table, item, source_row, column),
        |_, _: &mut ()| Ok(true),
        &mut (),
    )
    .map(|found| found.rows)
    .map_err(|found| refusal(found, item, table, keys))
}

/// The finding of a named intermediate's match that stopped before its
/// candidates were complete.
fn refusal<E>(
    refusal: Refusal<E>,
    item: &Intermediate,
    table: Table<'_, E>,
    keys: &[(usize, Value)],
) -> Fail<E> {
    match refusal {
        Refusal::Failed(error) => error,
        Refusal::Key {
            index: pair,
            current,
            donor,
        } => {
            let column = keys[pair].0;
            let key = item
                .keys
                .iter()
                .find(|key| key.source_column == column)
                .expect("resolved key belongs to the admitted selector");
            Box::new(ExecutionError::ScalarDiagnostic(
                crate::diagnostic::lookup::key_type(
                    &key.path,
                    crate::diagnostic::lookup::Owner::Intermediate(&item.identifier),
                    &key_source(key, &table.schema().columns()[column].name),
                    crate::result_type::ResultType::known(current),
                    crate::result_type::ResultType::known(donor),
                ),
            ))
        }
        Refusal::Range(types) => Box::new(ExecutionError::ScalarDiagnostic(types.diagnostic(
            item.range.as_ref().map_or("", |range| range.path()),
            &item.identifier,
        ))),
    }
}

/// A correlated filter over one donor record and the current row.
#[allow(clippy::too_many_arguments)]
fn correlated<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    filter: &RelationPredicate,
    table: Table<'_, E>,
    item: &Intermediate,
    source_row: usize,
    row: usize,
) -> Result<Truth, Fail<E>> {
    use crate::bound_expression::Read as BoundRead;
    use crate::expression::{ErrorKind, Resolver};
    struct Context<'r, 'p, 'a, 'c, 't, E> {
        run: &'r mut Run<'p, 'a, E>,
        cx: &'r Cx<'c, E>,
        site: Site<'r>,
        filter: &'r RelationPredicate,
        table: Table<'t, E>,
        item: &'r Intermediate,
        source_row: usize,
        row: usize,
    }
    impl<E> Resolver for Context<'_, '_, '_, '_, '_, E> {
        type Error = Fail<E>;
        fn resolve(&mut self, identifier: &str) -> Result<crate::value::Selection, Self::Error> {
            let binding = self
                .filter
                .bindings()
                .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
                .expect("all predicate names admitted before execution");
            let path = self.filter.plan().spec_path();
            let value = match self.filter.bindings()[binding].read {
                BoundRead::Source(column) => {
                    let value = self.table.get(self.source_row, column).map_err(|error| {
                        Box::new(match &self.item.source {
                            IntermediateSource::Primary { .. } => ExecutionError::Cell {
                                path: path.into(),
                                source_row: self.source_row,
                                error,
                            },
                            IntermediateSource::Input(source) => ExecutionError::SecondaryCell {
                                path: path.into(),
                                source: *source,
                                source_row: self.source_row,
                                error,
                            },
                            IntermediateSource::SelfRows { .. } => ExecutionError::SchemaMismatch,
                        })
                    })?;
                    owned(value)
                }
                BoundRead::Column(slot) => {
                    let field = &self.filter.current()[slot];
                    current_value(self.run, self.cx, self.site, field, path, Some(self.row))?
                }
                BoundRead::Intermediate { .. } | BoundRead::Joined(_) => {
                    unreachable!("relation predicate uses explicit current metadata")
                }
            };
            Ok(crate::value::Selection::Present(value))
        }
    }
    filter
        .plan()
        .evaluate_truth(&mut Context {
            run,
            cx,
            site,
            filter,
            table,
            item,
            source_row,
            row,
        })
        .map_err(|error| {
            if matches!(&error.kind, ErrorKind::Condition(_)) {
                return Box::new(
                    error
                        .diagnostic()
                        .map(ExecutionError::ScalarDiagnostic)
                        .unwrap_or(ExecutionError::Capacity),
                );
            }
            match error.kind {
                ErrorKind::Resolution { error, .. } => error,
                ErrorKind::Condition(_) => unreachable!("condition was projected above"),
            }
        })
}

/// One current field of the row: a record read collected over the row's
/// records, a completed value, or another intermediate's read. `row` is
/// none for a probe, whose named reads are refused. A named read made for
/// another relation is never a handler site.
#[allow(clippy::too_many_arguments)]
pub(super) fn current_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    field: &CurrentField,
    path: &str,
    row: Option<usize>,
) -> Result<Value, Fail<E>> {
    match field.read {
        CurrentRead::Record { source, column } => {
            let table = if source == 0 {
                cx.primary
            } else {
                cx.secondary[source - 1]
            };
            let Some(row) = row else {
                return Ok(Value::Missing);
            };
            let read = Cx {
                table,
                ..cx.clone()
            };
            super::predicates::collect_row(
                run,
                &read,
                super::predicates::Collection {
                    column,
                    identifier: &field.identifier,
                    filter: None,
                    selection: None,
                    path,
                    site: None,
                },
                row,
            )
            .map_err(|mut error| {
                if source > 0 && matches!(error.as_ref(), ExecutionError::Cell { .. }) {
                    if let ExecutionError::Cell {
                        path,
                        source_row,
                        error: original,
                    } = std::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
                    {
                        *error = ExecutionError::SecondaryCell {
                            path,
                            source: source - 1,
                            source_row,
                            error: original,
                        };
                    }
                }
                error
            })
        }
        CurrentRead::Output(column) => Ok(match row {
            Some(row) => cx.frame.value(row, column),
            None => Value::Missing,
        }),
        CurrentRead::Intermediate { index, column } => {
            let Some(row) = row else {
                // Admission forbids named reads during destination-free probes.
                return Err(Box::new(ExecutionError::SchemaMismatch));
            };
            let relation = run
                .plan
                .named(cx.region, index)
                .expect("a current field reads its region's intermediate");
            named_value(run, cx, site, relation, column, row, false)
        }
    }
}

/// The current row's match operands, ordinary reads first, then the reads of
/// expression keys; the first use of an identifier wins.
fn resolve_operands<'k, E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    path: &str,
    keys: &'k [crate::dataset::IntermediateKey],
    row: usize,
) -> Result<Vec<(&'k MatchRead, Value)>, Fail<E>> {
    let mut values: Vec<(&MatchRead, Value)> = Vec::new();
    for expression in [false, true] {
        for key in keys {
            if key.value.is_expression() != expression {
                continue;
            }
            for read in key.value.reads() {
                if values
                    .iter()
                    .any(|(bound, _)| bound.identifier() == read.identifier())
                {
                    continue;
                }
                let value = match_read(run, cx, path, read, row)?;
                values.push((read, value));
            }
        }
    }
    Ok(values)
}

/// One match operand of the current row.
fn match_read<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    path: &str,
    read: &MatchRead,
    row: usize,
) -> Result<Value, Fail<E>> {
    match read {
        MatchRead::Output { column, .. } => Ok(cx.frame.value(row, *column)),
        MatchRead::Record { column, identifier }
        | MatchRead::SecondaryRecord {
            column, identifier, ..
        } => {
            let table = if let MatchRead::SecondaryRecord { source, .. } = read {
                let current = cx
                    .frame
                    .template(row)
                    .map_or(0, |template| run.template_source(cx.region, template));
                if source + 1 != current {
                    return Err(Box::new(ExecutionError::SchemaMismatch));
                }
                *cx.secondary
                    .get(*source)
                    .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?
            } else {
                cx.table
            };
            let read_cx = Cx {
                table,
                ..cx.clone()
            };
            super::predicates::collect_row(
                run,
                &read_cx,
                super::predicates::Collection {
                    column: *column,
                    identifier,
                    filter: None,
                    selection: None,
                    path,
                    site: None,
                },
                row,
            )
        }
    }
}

/// An aggregate's match values: expression keys after their operands are
/// resolved, otherwise the ordinary traversal.
pub(super) fn match_keys<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    path: &str,
    keys: &[crate::dataset::IntermediateKey],
    row: usize,
) -> Result<Vec<(usize, Value)>, Fail<E>> {
    if keys.iter().any(|key| key.value.is_expression()) {
        let current = resolve_operands(run, cx, path, keys, row)?;
        evaluate_keys(run, cx, relation, keys, &current, row)
    } else {
        ordinary_keys(run, cx, path, keys, row)
    }
}

/// Ordinary selectors keep their output-then-record traversal.
fn ordinary_keys<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    path: &str,
    keys: &[crate::dataset::IntermediateKey],
    row: usize,
) -> Result<Vec<(usize, Value)>, Fail<E>> {
    let mut resolved = Vec::with_capacity(keys.len());
    for record in [false, true] {
        for key in keys {
            let MatchValue::Variable(read) = &key.value else {
                unreachable!("ordinary selector contains variables only");
            };
            if matches!(
                read,
                MatchRead::Record { .. } | MatchRead::SecondaryRecord { .. }
            ) == record
            {
                resolved.push((key.source_column, match_read(run, cx, path, read, row)?));
            }
        }
    }
    Ok(resolved)
}

/// Evaluate every key before considering missingness.
fn evaluate_keys<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    keys: &[crate::dataset::IntermediateKey],
    current: &[(&MatchRead, Value)],
    row: usize,
) -> Result<Vec<(usize, Value)>, Fail<E>> {
    let plan = run.plan;
    let value_of = |read: &MatchRead| -> Value {
        current
            .iter()
            .find(|(bound, _)| bound.identifier() == read.identifier())
            .expect("all actual references resolved once")
            .1
            .clone()
    };
    let lowered = &plan.relation(relation).keys;
    let mut resolved = Vec::with_capacity(keys.len());
    for (position, key) in keys.iter().enumerate() {
        let value = match &key.value {
            MatchValue::Expression(expression) => {
                let KeyValue::Expr(expr) = lowered[position].value else {
                    unreachable!("an expression key is lowered as a node")
                };
                if expression.windowed {
                    run.eval(cx, expr, &[row], false)?.value(0)
                } else {
                    // All authored current references were resolved with their
                    // actual source membership; the expression now sees one
                    // current record.
                    let operands: Vec<Value> = expression.reads.iter().map(&value_of).collect();
                    let scalar = Cx {
                        operands: Some(&operands),
                        ..cx.clone()
                    };
                    run.eval(&scalar, expr, &[row], false)?.value(0)
                }
            }
            MatchValue::Contextual(_) => return Err(Box::new(ExecutionError::SchemaMismatch)),
            MatchValue::Variable(read) | MatchValue::Source(read) => value_of(read),
            MatchValue::Literal(value) => value.clone(),
            MatchValue::StrPad { read, operation } => super::scalar::pad(
                run,
                cx,
                operation.as_ref().expect("padding declaration admitted"),
                &key.path,
                &value_of(read),
                row,
            )?,
        };
        resolved.push((key.source_column, value));
    }
    Ok(resolved)
}

/// The value of a match operand inside a match expression.
pub(super) fn operand_value(index: usize, operands: &[Value]) -> &Value {
    &operands[index]
}

/// The key pair's authored source in a key-type finding: the match variable,
/// or `key[FIELD]` for a match value written as an expression.
pub(super) fn key_source(key: &crate::dataset::IntermediateKey, field: &str) -> String {
    if key.value.is_expression() {
        format!("key[{field}]")
    } else {
        key.value
            .read()
            .map_or_else(|| field.into(), |read| read.identifier().into())
    }
}

/// The current row's side of one match, resolved before any record is read.
pub(super) struct Probe<'p> {
    pub keys: &'p [(usize, ValueRef<'p>)],
    pub kinds: &'p [Option<ValueType>],
    pub range: Option<(&'p crate::dataset::RelationRange, ValueRef<'p>)>,
}

impl Probe<'_> {
    /// A missing match value or range value seeks no record (LOOKUP-9).
    pub(super) fn incomplete(&self) -> bool {
        self.keys
            .iter()
            .any(|(_, value)| matches!(value, ValueRef::Missing))
            || self
                .range
                .is_some_and(|(_, value)| matches!(value, ValueRef::Missing))
    }
}

/// Why a match stopped before its candidates were complete.
pub(super) enum Refusal<E> {
    Failed(Fail<E>),
    Key {
        index: usize,
        current: Option<ValueType>,
        donor: Option<ValueType>,
    },
    Range(crate::dataset::RelationRangeTypes),
}

impl<E> From<Fail<E>> for Refusal<E> {
    fn from(error: Fail<E>) -> Self {
        Self::Failed(error)
    }
}

/// The candidates in offer order, and the first record whose keys matched
/// before the range and the filter.
pub(super) struct Candidates {
    pub rows: Vec<usize>,
    pub first_key_match: Option<usize>,
}

/// Exact key equality under VALUE-19, with no operand converted.
fn key_equal(donor: ValueRef<'_>, current: ValueRef<'_>) -> Result<bool, ()> {
    if let (ValueRef::Bool(left), ValueRef::Bool(right)) = (donor, current) {
        return Ok(left == right);
    }
    match crate::value::compare_present_refs(donor, current) {
        Ok(Ordering::Equal) => Ok(true),
        Err(crate::value::ComparisonError::IncompatibleTypes) => Err(()),
        Ok(_) | Err(crate::value::ComparisonError::MissingOperand) => Ok(false),
    }
}

/// Whether a key pair's types are known to compare before its record value is read.
fn known_comparable(current: ValueRef<'_>, donor: Option<ValueType>) -> bool {
    current
        .value_type()
        .zip(donor)
        .is_some_and(|(current, donor)| crate::value::comparable_types(current, donor))
}

/// Match and narrow for one current row. Every key pair is compared on each
/// offered record; the range is compared on the records whose keys match,
/// and `narrow` is asked only about matched records.
pub(super) fn candidates<'t, S, E>(
    rows: impl Iterator<Item = usize>,
    probe: &Probe<'_>,
    mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
    mut narrow: impl FnMut(usize, &mut S) -> Result<bool, Fail<E>>,
    state: &mut S,
) -> Result<Candidates, Refusal<E>> {
    let mut found = Candidates {
        rows: Vec::new(),
        first_key_match: None,
    };
    for row in rows {
        let mut matches = true;
        for (index, &(column, current)) in probe.keys.iter().enumerate() {
            if !matches && known_comparable(current, probe.kinds.get(index).copied().flatten()) {
                continue;
            }
            let donor = read(row, column)?;
            match key_equal(donor, current) {
                Ok(true) => (),
                Ok(false) => matches = false,
                Err(()) => {
                    return Err(Refusal::Key {
                        index,
                        current: current.value_type(),
                        donor: donor.value_type(),
                    })
                }
            }
        }
        if !matches {
            continue;
        }
        found.first_key_match.get_or_insert(row);
        if let Some((range, value)) = probe.range {
            let allowed = range
                .allows(value, |column| read(row, column))
                .map_err(|failure| match failure {
                    crate::dataset::RelationRangeFailure::Access(error) => Refusal::Failed(error),
                    crate::dataset::RelationRangeFailure::Incomparable(types) => {
                        Refusal::Range(types)
                    }
                })?;
            if !allowed {
                continue;
            }
        }
        if narrow(row, state)? {
            found.rows.push(row);
        }
    }
    Ok(found)
}

/// The first or last of several candidates by `order_by`; the earlier
/// stored record wins a tie under `first`, the later one under `last`.
pub(super) fn select<'t, E>(
    candidates: &[usize],
    selection: &SourceSelection,
    mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
) -> Result<usize, Fail<E>> {
    let mut best = candidates[0];
    for &row in &candidates[1..] {
        let mut order = Ordering::Equal;
        for term in &selection.order_by {
            order = compare(read(row, term.column)?, read(best, term.column)?, term);
            if order != Ordering::Equal {
                break;
            }
        }
        order = order.then_with(|| row.cmp(&best));
        if (selection.keep == Keep::First && order == Ordering::Less)
            || (selection.keep == Keep::Last && order == Ordering::Greater)
        {
            best = row;
        }
    }
    Ok(best)
}

/// Compare values within one normalized column; missing values sort by the
/// term's placement.
pub(super) fn compare(left: ValueRef<'_>, right: ValueRef<'_>, term: &OrderTerm) -> Ordering {
    match (left, right) {
        (ValueRef::Missing, ValueRef::Missing) => Ordering::Equal,
        (ValueRef::Missing, _) => {
            if term.nulls_first {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (_, ValueRef::Missing) => {
            if term.nulls_first {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        _ => {
            let order = match (left, right) {
                (ValueRef::Str(a), ValueRef::Str(b)) => a.cmp(b),
                (ValueRef::Int(a), ValueRef::Int(b)) => a.cmp(&b),
                (ValueRef::Float(a), ValueRef::Float(b)) => a
                    .get()
                    .partial_cmp(&b.get())
                    .expect("finite normalized values"),
                (ValueRef::Bool(a), ValueRef::Bool(b)) => a.cmp(&b),
                (ValueRef::Date(a), ValueRef::Date(b)) => a.cmp(&b),
                (ValueRef::DateTime(a), ValueRef::DateTime(b)) => a.cmp(&b),
                _ => unreachable!("completed columns have one admitted logical type"),
            };
            if term.descending {
                order.reverse()
            } else {
                order
            }
        }
    }
}

/// Avoid indexing small relations whose scan is already cheap.
const MINIMUM_INDEXED_ROWS: usize = 128;

/// An index over the full key tuple of a plain intermediate. Each key
/// component is encoded canonically, so values that compare equal share one
/// bucket: integer and float spellings of the same number, both float zero
/// signs, and temporal values whatever their precision. Buckets keep stored
/// record order.
#[derive(Default)]
struct KeyIndex {
    columns: Vec<usize>,
    buckets: BTreeMap<Vec<u8>, Vec<usize>>,
}

fn encode(key: &mut Vec<u8>, value: ValueRef<'_>) {
    match value {
        ValueRef::Missing => key.push(0),
        ValueRef::Str(value) => {
            key.push(1);
            key.extend_from_slice(&(value.len() as u64).to_be_bytes());
            key.extend_from_slice(value.as_bytes());
        }
        ValueRef::Int(value) => encode_int(key, value),
        ValueRef::Float(value) => {
            let value = value.get();
            if value == value.trunc()
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
            {
                encode_int(key, value as i64);
            } else {
                key.push(3);
                key.extend_from_slice(&value.to_bits().to_be_bytes());
            }
        }
        ValueRef::Bool(value) => {
            key.push(4);
            key.push(u8::from(value));
        }
        ValueRef::Date(value) => {
            let (year, month, day) = value.fields();
            key.push(5);
            key.extend_from_slice(&year.to_be_bytes());
            key.push(month);
            key.push(day);
        }
        ValueRef::DateTime(value) => {
            let (year, month, day, hour, minute, second) = value.fields();
            key.push(6);
            key.extend_from_slice(&year.to_be_bytes());
            key.push(month);
            key.push(day);
            key.push(hour);
            key.push(minute);
            key.push(second);
        }
    }
}

fn encode_int(key: &mut Vec<u8>, value: i64) {
    key.push(2);
    key.extend_from_slice(&value.to_be_bytes());
}

impl KeyIndex {
    /// Build over every record, or none when a donor cell's type does not
    /// fit its declared column type: the scan owns that shape's refusal.
    fn build<E>(table: Table<'_, E>, item: &Intermediate) -> Result<Option<Self>, Fail<E>> {
        let mut index = Self {
            columns: item.keys.iter().map(|key| key.source_column).collect(),
            buckets: BTreeMap::new(),
        };
        for row in 0..table.row_count() {
            let mut key = Vec::new();
            let mut missing = false;
            for &column in &index.columns {
                let value = cell(table, item, row, column)?;
                if matches!(value, ValueRef::Missing) {
                    missing = true;
                    break;
                }
                if let (Some(kind), Some(actual)) = (
                    table
                        .schema()
                        .columns()
                        .get(column)
                        .map(|column| column.kind),
                    value.value_type(),
                ) {
                    if !crate::value::comparable_types(actual, kind.into()) {
                        return Ok(None);
                    }
                }
                encode(&mut key, value);
            }
            if missing {
                continue;
            }
            index.buckets.entry(key).or_default().push(row);
        }
        Ok(Some(index))
    }

    fn matching(&self, keys: &[(usize, Value)]) -> &[usize] {
        let mut key = Vec::new();
        for &column in &self.columns {
            let Some((_, value)) = keys.iter().find(|(bound, _)| *bound == column) else {
                return &[];
            };
            encode(&mut key, ValueRef::from(value));
        }
        self.buckets.get(&key).map_or(&[], Vec::as_slice)
    }
}

impl<'p, 'a, E> Run<'p, 'a, E> {
    /// The source position of a template's driver, the primary being 0.
    pub(super) fn template_source(&self, region: RegionId, template: usize) -> usize {
        let id = self
            .plan
            .region_templates(region)
            .nth(template)
            .expect("a region template");
        let source = self.plan.template(id).driver.source;
        let first = self
            .plan
            .region_sources(region)
            .next()
            .expect("a region has a driver source");
        source.index() - first.index()
    }

    /// A destination-free context with no current row, for driver and
    /// verification materializations.
    pub(super) fn probe_cx<'f>(
        &self,
        frame: &'f Frame,
        region: RegionId,
        secondary: &'f [Table<'f, E>],
    ) -> Cx<'f, E>
    where
        'a: 'f,
        'p: 'f,
    {
        let mut cx = self.template_cx(frame, region, Table::Source(self.primary), secondary, 0);
        cx.output = 0..0;
        cx
    }
}
