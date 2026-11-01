//! Named intermediates under the lookup model (LOOKUP-25): match the records
//! whose keys equal the current row's match values and whose bounds hold its
//! range value, narrow them by the filter, select one, and answer absence.
//! One choice per row and intermediate is cached for every read of the row.
//!
//! The records whose keys equal the match values come from a key index
//! (#2842) when every key pair can be answered exactly, and otherwise from a
//! scan that compares every pair on every record; the range, the filter and
//! the selection then run on those records in stored order. An index over a
//! SELF relation is dropped at each template barrier (LOOKUP-12).
use super::donor::{self, Built, Donor};
use super::eval::owned;
use super::frame::{Frame, Identity, IdentityMap};
use super::index::{self, Index};
use super::{Cx, Fail, Run, Table};
use crate::dataset::execution::{CheckRecord, ExecutionError, RowIdentity};
use crate::dataset::resolved::{ExprId, KeyValue, RegionId, RelationId, RelationKey, ResolvedPlan};
use crate::dataset::{
    CurrentField, CurrentRead, Intermediate, IntermediateSource, Keep, MatchRead, MatchValue,
    Nearest, OrderTerm, RelationPredicate, SourceSelection, VerificationSeverity,
};
use crate::expression::Truth;
use crate::numeric_lifecycle::HandlerKind;
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::value::{Value, ValueType};
use std::cmp::Ordering;

pub(super) use super::aggregates::{aggregate, Record};
pub(super) use super::joins::{join, joined_value, JoinSite};
pub(super) use super::odm::odm;
pub(super) use super::reads::read;

/// A row's match values: each key's donor column and the value it must equal.
pub(super) type MatchValues = Vec<(usize, Value)>;

/// One row's record choice: immutable donor coordinates and whether
/// several records competed.
#[derive(Clone, Copy, Debug)]
pub(super) enum Choice {
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
    /// The key index over the records the match reads, when they are the
    /// source's: none for a derived intermediate, whose donor owns it.
    index: Option<Option<Index>>,
    /// The key index over the source's stored keys that selects the records
    /// derivations reading the current record augment (LOOKUP-15).
    source_index: Option<Option<Index>>,
    choices: Choices,
    materialized: IdentityMap<Option<Identity>, Built>,
    /// Whether derivations reading the current record augment only the
    /// matched records (LOOKUP-15), decided on first use.
    matched: Option<bool>,
}

/// Each row identity's record choice: a candidate's by its number, so a
/// read over any rows finds theirs without hashing, and any other identity's
/// in a map.
#[derive(Default)]
struct Choices {
    candidates: Vec<Option<Choice>>,
    others: IdentityMap<Identity, Choice>,
}

impl Choices {
    fn get(&self, identity: Identity) -> Option<Choice> {
        match identity {
            Identity::Candidate(number) => self.candidates.get(number).copied().flatten(),
            other => self.others.get(&other).copied(),
        }
    }

    fn insert<E>(&mut self, identity: Identity, choice: Choice) -> Result<(), Fail<E>> {
        match identity {
            Identity::Candidate(number) => {
                if number >= self.candidates.len() {
                    let more = number + 1 - self.candidates.len();
                    super::reserved(self.candidates.try_reserve(more))?;
                    self.candidates.resize(number + 1, None);
                }
                self.candidates[number] = Some(choice);
            }
            other => {
                super::reserved(self.others.try_reserve(1))?;
                self.others.insert(other, choice);
            }
        }
        Ok(())
    }

    fn remove(&mut self, identity: Identity) {
        match identity {
            Identity::Candidate(number) => {
                if let Some(slot) = self.candidates.get_mut(number) {
                    *slot = None;
                }
            }
            other => {
                self.others.remove(&other);
            }
        }
    }

    fn clear(&mut self) {
        self.candidates.clear();
        self.others.clear();
    }
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
    /// The records the last shareable join reads chose for their
    /// selections, the latest last.
    joined: Vec<super::joins::Shared>,
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
            joined: Vec::new(),
        }
    }

    /// The records a recent shareable join read chose that `matches`, taken
    /// out until they are kept again.
    pub(super) fn take_joined(
        &mut self,
        matches: impl Fn(&super::joins::Shared) -> bool,
    ) -> Option<super::joins::Shared> {
        let position = self.joined.iter().position(matches)?;
        Some(self.joined.remove(position))
    }

    /// Keep the records a join read chose, dropping the oldest beyond the
    /// few selections a column's bindings or templates read in turn.
    pub(super) fn keep_joined(&mut self, shared: super::joins::Shared) {
        const KEPT: usize = 8;
        if self.joined.len() == KEPT {
            self.joined.remove(0);
        }
        self.joined.push(shared);
    }

    fn named(&mut self, relation: RelationId) -> &mut Named {
        &mut self.named[relation.index() - self.first]
    }

    /// Whether a relation's derivations augment only the records its match
    /// values select (LOOKUP-15).
    fn derives_matched(&mut self, plan: &ResolvedPlan, relation: RelationId) -> bool {
        *self
            .named(relation)
            .matched
            .get_or_insert_with(|| item(plan, relation).derives_matched_records())
    }

    pub(super) fn mark(&self) -> Mark {
        Mark(self.journal.len())
    }

    pub(super) fn restore(&mut self, mark: Mark) {
        self.joined.clear();
        while self.journal.len() > mark.0 {
            match self.journal.pop().expect("a journal entry") {
                Entry::Choice(relation, identity) => {
                    self.named(relation).choices.remove(identity);
                }
                Entry::Materialized(relation, key) => {
                    self.named(relation).materialized.remove(&key);
                }
            }
        }
    }

    /// A row identity's cached record choice of a relation, if any.
    pub(super) fn choice(&mut self, relation: RelationId, identity: Identity) -> Option<Choice> {
        self.named(relation).choices.get(identity)
    }

    /// Cache a row identity's record choice, removed if the unit replays.
    pub(super) fn insert_choice<E>(
        &mut self,
        relation: RelationId,
        identity: Identity,
        choice: Choice,
    ) -> Result<(), Fail<E>> {
        super::reserved(self.journal.try_reserve(1))?;
        self.named(relation).choices.insert(identity, choice)?;
        self.journal.push(Entry::Choice(relation, identity));
        Ok(())
    }

    /// A donor materialized for a key, if any.
    pub(super) fn materialized(
        &mut self,
        relation: RelationId,
        key: Option<Identity>,
    ) -> Option<Built> {
        self.named(relation).materialized.get(&key).cloned()
    }

    pub(super) fn insert_materialized(
        &mut self,
        relation: RelationId,
        key: Option<Identity>,
        donor: Built,
    ) {
        self.named(relation).materialized.insert(key, donor);
        self.journal.push(Entry::Materialized(relation, key));
    }

    /// A key window observes a different relation after row templates
    /// finish: choices and correlated materializations from template-local
    /// windows cannot identify a choice in the complete output relation.
    pub(super) fn begin_columns(&mut self, plan: &ResolvedPlan) {
        self.joined.clear();
        let items: Vec<&Intermediate> = (0..self.named.len())
            .map(|index| {
                plan.relation(RelationId::try_from(self.first + index).expect("a slot"))
                    .named
                    .as_deref()
                    .expect("a named relation")
            })
            .collect();
        // A nearest selection's reference is read as a key's match value is
        // (LOOKUP-26).
        let windowed_key = |item: &Intermediate| {
            let reference = item.nearest.iter().map(|nearest| &nearest.reference);
            item.keys.iter().chain(reference).any(
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
        self.joined.clear();
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
                named.index = None;
                named.source_index = None;
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
pub(super) fn source_name<'i>(
    item: &'i Intermediate,
    secondary: &'i [crate::dataset::SecondarySource],
) -> &'i str {
    match &item.source {
        IntermediateSource::Input(index) => &secondary[*index].name,
        IntermediateSource::Primary { name, .. } => name,
        IntermediateSource::SelfRows { .. } => "SELF",
        // A pair relation's records are its own (LOOKUP-27).
        IntermediateSource::Pairs(_) => &item.identifier,
    }
}

/// The table a named intermediate's records come from.
pub(super) fn source_table<'t, E>(item: &Intermediate, cx: &Cx<'t, E>) -> Table<'t, E> {
    match &item.source {
        IntermediateSource::Input(index) => cx.secondary[*index],
        IntermediateSource::Primary { .. } => cx.primary,
        IntermediateSource::SelfRows { .. } => Table::Rows {
            frame: cx.frame,
            schema: cx.schema,
            start: 0,
            rows: cx.self_rows,
        },
        // Readers read a pair relation through its port (LOOKUP-27).
        IntermediateSource::Pairs(join) => cx.secondary[join.port],
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
pub(super) fn keeps<E>(
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
pub(super) fn eligibility<E>(
    table: Table<'_, E>,
    item: &Intermediate,
) -> Result<Vec<usize>, Fail<E>> {
    check_deferred_filter_types(item)?;
    let mut eligible = Vec::new();
    super::reserved(eligible.try_reserve_exact(table.row_count()))?;
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
    pub materialized: Option<Built>,
    pub eligible: Option<Vec<usize>>,
}

/// Row drivers consume the complete source-only donor set without executing a match.
pub(super) fn driver<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    template: crate::dataset::resolved::TemplateId,
    frame: &Frame,
    secondary: &[Table<'s, E>],
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
            IntermediateSource::SelfRows { .. } | IntermediateSource::Pairs(_) => {
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
/// record, and a later match reuses each answer. A violated `error` check
/// fails the run; each `warning` check, held or violated, is a record in
/// `records`, those of one pass in declaration order, and a `SELF` check's
/// one record holds its last evaluation (LOOKUP-17, CHECK-23).
pub(super) fn verify<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    frame: &Frame,
    secondary: &[Table<'s, E>],
    self_only: bool,
    records: &mut Vec<CheckRecord>,
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let region = RegionId::DATASET;
    let mut failures = Vec::new();
    let mut warnings = Vec::new();
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
        for (position, check) in item.unique.iter().enumerate() {
            let mut columns = Vec::new();
            for &column in &check.columns {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
            // The records a key would match are one combination, and two
            // missing values are one too (LOOKUP-17).
            let groups = crate::table_grouping::partition_matching(&filtered, &columns)
                .map_err(|error| Box::new(ExecutionError::Grouping(error)))?;
            let repeated: Vec<&Vec<usize>> =
                groups.iter().filter(|group| group.len() > 1).collect();
            match check.severity {
                VerificationSeverity::Error => {
                    if !repeated.is_empty() {
                        failures.push(check.diagnostic(item, table.schema(), name, repeated.len()));
                    }
                }
                VerificationSeverity::Warning => {
                    // Each repeated combination offends once, as its first
                    // donor record holds it, in donor order (CHECK-24).
                    let mut offending_rows = Vec::new();
                    for members in &repeated {
                        let source_row = eligible[members[0]];
                        let mut values = Vec::new();
                        for &column in &check.columns {
                            values.push(owned(cell(table, item, source_row, column)?));
                        }
                        offending_rows.push(RowIdentity {
                            position: source_row,
                            values,
                        });
                    }
                    warnings.push((
                        (check.declared, position),
                        CheckRecord {
                            path: check.path.clone(),
                            code: crate::diagnostic::ConditionCode::IntermediateDuplicateRecords,
                            evaluated_count: groups.len(),
                            failed_count: repeated.len(),
                            output_rows: frame.rows(),
                            offending_rows,
                            codelist: None,
                            inventory: None,
                            row_count: None,
                            entries: None,
                            denominator: None,
                        },
                    ));
                }
            }
        }
        let mut narrowed = vec![Some(false); table.row_count()];
        for row in eligible {
            narrowed[row] = Some(true);
        }
        run.relations.named(relation).narrowed = Some(narrowed);
    }
    warnings.sort_by_key(|(order, _)| *order);
    for (_, record) in warnings {
        match records.iter_mut().find(|held| held.path == record.path) {
            Some(held) => *held = record,
            None => records.push(record),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Box::new(ExecutionError::IntermediateVerifications(
            failures,
        )))
    }
}

/// An aggregate derivation keeps its donor record apart from the current
/// row: a named, kept selection reads the enclosing row on the primary.
pub(super) fn enclosed<'c, E>(cx: &Cx<'c, E>) -> Option<Cx<'c, E>> {
    cx.record.is_some().then(|| Cx {
        record: None,
        table: cx.primary,
        ..cx.clone()
    })
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
    if let Some(enclosing) = enclosed(cx) {
        return named_value(run, &enclosing, site, relation, column, row, observe);
    }
    let item = item(run.plan, relation);
    let (prepared, choice) = choose(run, cx, site, relation, row)?;
    let table = prepared
        .as_ref()
        .map_or_else(|| source_table(item, cx), Built::table);
    let (value, handler) = chosen(table, item, column, choice)?;
    let value = owned(value);
    if let Some(handler) = handler.filter(|_| observe) {
        run.fire_at(
            site.expr,
            &format!("{}.{}", site.path, handler.name()),
            handler,
        )?;
    }
    Ok(value)
}

/// A row's value of a named intermediate's column by its choice, read from
/// the records it chose among, and the handler the read fires.
pub(super) fn chosen<'v, E>(
    table: Table<'v, E>,
    item: &'v Intermediate,
    column: usize,
    choice: Choice,
) -> Result<(ValueRef<'v>, Option<HandlerKind>), Fail<E>> {
    Ok(match choice {
        Choice::Record { row, multiple } => (
            cell(table, item, row, column)?,
            multiple.then_some(HandlerKind::MultipleMatches),
        ),
        // The declared literal, or missing by default; counted either way,
        // under `absent` (D202).
        Choice::Absent => (
            item.no_match
                .as_ref()
                .map_or(ValueRef::Missing, ValueRef::from),
            Some(HandlerKind::Absent),
        ),
    })
}

/// One row's record choice, from the relation's cache or resolved now and
/// cached under the row's identity, with the records it chose among: a
/// materialized donor's, or none for the source's own.
pub(super) fn choose<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    relation: RelationId,
    row: usize,
) -> Result<(Option<Built>, Choice), Fail<E>> {
    let plan = run.plan;
    let item = item(plan, relation);
    let identity = cx.frame.identity(row);
    let cached = run.relations.choice(relation, identity);
    let secondary = &plan.region(cx.region).secondary;
    let name = source_name(item, secondary);
    let reading = Reading {
        site,
        relation,
        item,
        name,
        row,
        identity,
    };
    Ok(match cached {
        Some(choice) => (
            donor::materialize(run, cx, relation, Some(site), Some(row))?,
            choice,
        ),
        // Derivations that read the current record augment only the records
        // the match values select (LOOKUP-15), so those values resolve first.
        None if run.relations.derives_matched(plan, relation) => {
            let (data, choice) = matched_choice(run, cx, reading)?;
            (Some(data), choice)
        }
        None => {
            let prepared = donor::materialize(run, cx, relation, Some(site), Some(row))?;
            let table = match &prepared {
                Some(data) => data.table(),
                None => source_table(item, cx),
            };
            let choice = resolve_choice(
                run,
                cx,
                reading,
                table,
                None,
                prepared.as_ref().map(Built::donor),
            )?;
            (prepared, choice)
        }
    })
}

/// LOOKUP-15: the choice of a derived intermediate whose derivations read
/// the current record, over the records its match values select. When the
/// derivations read only their own record and the current one, the records
/// are built alone first; a relation in the donor plan reads the other
/// records too, so it keeps them all. A failure there is undone and the
/// records built again at their source positions, so the diagnostic names
/// the source rows; a failure stops the run, so this happens once. A unit
/// that may call the host builds at source positions from the start: a call
/// cannot be undone.
fn matched_choice<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    reading: Reading<'_>,
) -> Result<(Built, Choice), Fail<E>> {
    let relation = reading.relation;
    let plan = run.plan;
    let donor = plan
        .relation(relation)
        .derivations
        .expect("a derived intermediate has a donor region");
    if !run.calls && plan.region_relations(donor).next().is_none() {
        let checkpoint = run.checkpoint();
        match matched_attempt(run, cx, reading, true) {
            Ok(found) => return Ok(found),
            Err(_) => run.undo(checkpoint),
        }
    }
    matched_attempt(run, cx, reading, false)
}

fn matched_attempt<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    reading: Reading<'_>,
    compact: bool,
) -> Result<(Built, Choice), Fail<E>> {
    let Reading {
        site,
        relation,
        row,
        ..
    } = reading;
    let (data, keys) = donor::materialize_matched(run, cx, relation, site, row, compact)?;
    let choice = resolve_choice(
        run,
        cx,
        reading,
        data.table(),
        Some(keys),
        Some(data.donor()),
    )?;
    Ok((data, choice))
}

/// LOOKUP-15 for a read over several rows: the choices of the rows not yet
/// chosen, resolved together over one donor of every row's matched records.
/// Each row's current fields, match values and matched records resolve in
/// row order, every matched record is augmented in one build with its own
/// row's current values, and then each row selects among its own records.
/// Any failure undoes all of it and leaves the rows to one-row resolution,
/// which finds the first failure in row order where the build would find it
/// column by column. As for one row, only derivations that read nothing but
/// their record and the current one, in a unit that cannot call the host,
/// are resolved this way.
pub(super) fn prepare_matched<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    relation: RelationId,
    rows: &[usize],
) {
    let plan = run.plan;
    if rows.len() < 2
        || run.calls
        || cx.record.is_some()
        || !run.relations.derives_matched(plan, relation)
    {
        return;
    }
    let item = item(plan, relation);
    let donor = plan
        .relation(relation)
        .derivations
        .expect("a derived intermediate has a donor region");
    if plan.region_relations(donor).next().is_some() {
        return;
    }
    let checkpoint = run.checkpoint();
    if prepare_rows(run, cx, site, relation, item, rows).is_err() {
        run.undo(checkpoint);
    }
}

fn prepare_rows<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    relation: RelationId,
    item: &Intermediate,
    rows: &[usize],
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let derived = item
        .derivations
        .as_ref()
        .expect("an intermediate deriving matched records has derivations");
    let mut seen = std::collections::HashSet::<
        _,
        std::hash::BuildHasherDefault<super::frame::IdentityHasher>,
    >::default();
    let pending: Vec<(usize, Identity)> = rows
        .iter()
        .map(|&row| (row, cx.frame.identity(row)))
        .filter(|(_, identity)| {
            run.relations.choice(relation, *identity).is_none() && seen.insert(*identity)
        })
        .collect();
    if pending.len() < 2 {
        return Ok(());
    }
    let width = derived.current.len();
    let mut current = Vec::new();
    super::reserved(current.try_reserve_exact(pending.len().saturating_mul(width)))?;
    let mut keys = Vec::new();
    super::reserved(keys.try_reserve_exact(pending.len()))?;
    let mut records = Vec::new();
    super::reserved(records.try_reserve(pending.len()))?;
    let mut spans = Vec::new();
    super::reserved(spans.try_reserve_exact(pending.len()))?;
    for &(row, _) in &pending {
        current.extend(donor::current_fields(
            run,
            cx,
            item,
            derived,
            Some(site),
            Some(row),
        )?);
        let row_keys = match_values(run, cx, relation, item, row)?;
        let start = records.len();
        matched_records(run, cx, relation, item, &row_keys, &mut records)?;
        spans.push((start, records.len() - start));
        keys.push(row_keys);
    }
    let mut per_record = Vec::new();
    super::reserved(per_record.try_reserve_exact(records.len().saturating_mul(width)))?;
    for (&(_, rows), values) in spans.iter().zip(current.chunks(width)) {
        for _ in 0..rows {
            per_record.extend_from_slice(values);
        }
    }
    let shared = donor::build_matched(run, cx, relation, &per_record, width, &records)?;
    let name = source_name(item, &plan.region(cx.region).secondary);
    for ((&(row, identity), &(start, rows)), row_keys) in pending.iter().zip(&spans).zip(keys) {
        let built = Built::part(&shared, start, rows);
        run.relations
            .insert_materialized(relation, Some(identity), built.clone());
        resolve_choice(
            run,
            cx,
            Reading {
                site,
                relation,
                item,
                name,
                row,
                identity,
            },
            built.table(),
            Some(row_keys),
            None,
        )?;
    }
    Ok(())
}

/// Retain exact key values separately from output-row identity in join diagnostics.
fn matched_key(
    table: &dyn TableAccess<Error = impl Sized>,
    keys: &[(usize, ValueRef<'_>)],
) -> Vec<(String, Value)> {
    keys.iter()
        .map(|&(column, value)| (table.schema().columns()[column].name.clone(), owned(value)))
        .collect()
}

/// One row's read of a named intermediate, as its record is chosen: the
/// node whose path counts the read's handlers, the relation and its
/// intermediate, the name its dataset is known by, and the row with its
/// identity, under which the choice is kept.
#[derive(Clone, Copy)]
pub(super) struct Reading<'r> {
    pub site: Site<'r>,
    pub relation: RelationId,
    pub item: &'r Intermediate,
    pub name: &'r str,
    pub row: usize,
    pub identity: Identity,
}

/// Resolve and cache the record choice for one read: match the records on
/// the resolved keys and range, narrow them by the filter, then select.
pub(super) fn resolve_choice<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    reading: Reading<'_>,
    table: Table<'_, E>,
    keys: Option<MatchValues>,
    donor: Option<&Donor>,
) -> Result<Choice, Fail<E>> {
    let Reading {
        site,
        relation,
        item,
        row,
        identity,
        ..
    } = reading;
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
    // The reference a nearest selection measures to is read with the match
    // values; a missing one seeks no record, as a missing match value does
    // (LOOKUP-26).
    let reference = reference_value(run, cx, relation, item, row)?;
    let probe_keys: Vec<(usize, ValueRef<'_>)> = keys
        .iter()
        .map(|(column, value)| (*column, ValueRef::from(value)))
        .collect();
    let sought = Sought {
        keys: &probe_keys,
        range: range_value.as_ref().map(ValueRef::from),
        reference: reference.as_ref(),
    };
    let choice = resolve_sought(run, cx, &reading, table, donor, &sought)?;
    run.relations.insert_choice(relation, identity, choice)?;
    Ok(choice)
}

/// One row's side of a named intermediate's match: its match values, each
/// a donor column and the value it must equal, its range value and the
/// reference of a nearest selection.
pub(super) struct Sought<'v> {
    pub keys: &'v [(usize, ValueRef<'v>)],
    pub range: Option<ValueRef<'v>>,
    pub reference: Option<&'v Value>,
}

/// Match the records on a row's sought values, narrow them by the filter,
/// then select (LOOKUP-25): the row's record choice, not yet cached.
pub(super) fn resolve_sought<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    reading: &Reading<'_>,
    table: Table<'_, E>,
    donor: Option<&Donor>,
    sought: &Sought<'_>,
) -> Result<Choice, Fail<E>> {
    let Reading {
        relation,
        item,
        name: source_name,
        row,
        ..
    } = *reading;
    let plan = run.plan;
    let secondary = &plan.region(cx.region).secondary;
    let probe_keys = sought.keys;
    let reference = sought.reference;
    let kinds: Vec<Option<ValueType>> = probe_keys
        .iter()
        .map(|(column, _)| item.source_kind(secondary, *column).map(Into::into))
        .collect();
    let probe = Probe {
        keys: probe_keys,
        kinds: &kinds,
        range: item.range.as_ref().zip(sought.range),
    };
    let mut count = 0;
    let mut selected = None;
    if !probe.incomplete() && !matches!(reference, Some(Value::Missing)) {
        let indexed = key_bucket(run, relation, item, table, donor, probe_keys)?;
        // A bucket's records already equal the match values on every key
        // pair; the range still holds or not per record.
        let bucket = Probe {
            keys: &[],
            kinds: &[],
            range: probe.range,
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
        if reusable {
            narrow_all(run, relation, item, table)?;
        }
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
                        correlated(run, cx, reading, filter, table, source_row)? == Truth::True
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
                    let named = run.relations.named(relation);
                    if named.narrowed.is_none() {
                        named.narrowed = Some(super::filled(table.row_count(), None)?);
                    }
                    if let Some(slot) = named
                        .narrowed
                        .as_mut()
                        .and_then(|narrowed| narrowed.get_mut(source_row))
                    {
                        *slot = Some(kept);
                    }
                }
                Ok(kept)
            },
            run,
        )
        .map_err(|found| refusal(found, item, table, probe_keys))?;
        count = found.rows.len();
        selected = found.rows.first().copied();
        // A nearest selection chooses among the candidates with a distance,
        // and one candidate without a distance is not chosen either.
        if let (Some(choice), Some(reference)) = (&item.nearest, reference) {
            (selected, count) = nearest(&found.rows, choice, reference, |source_row, column| {
                cell(table, item, source_row, column)
            })?;
        } else if count > 1 {
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
                    matched_key: matched_key(&table, probe_keys),
                    identity: run.failure_identity(cx, row),
                }));
            }
        }
    }
    if let Some(row) = selected {
        Ok(Choice::Record {
            row,
            multiple: count > 1,
        })
    } else if !item.require_match {
        Ok(Choice::Absent)
    } else {
        Err(Box::new(ExecutionError::UnmatchedKey {
            path: item.path.clone(),
            dataset: source_name.into(),
            intermediate: item.identifier.clone(),
            matched_key: matched_key(&table, probe_keys),
            identity: run.failure_identity(cx, row),
        }))
    }
}

/// LOOKUP-8 over every record at once, the first time a match narrows: a
/// filter that reads only the donor records keeps each record or not, as
/// the column program (#4199) answers for all of them. A program that does
/// not answer every record leaves each to be decided when it first matches,
/// where its failure, if any, is reported.
fn narrow_all<E>(
    run: &mut Run<'_, '_, E>,
    relation: RelationId,
    item: &Intermediate,
    table: Table<'_, E>,
) -> Result<(), Fail<E>> {
    let Some(filter) = item.filter.as_ref().filter(|filter| !filter.correlated()) else {
        return Ok(());
    };
    if run.relations.named(relation).narrowed.is_some() {
        return Ok(());
    }
    let count = table.row_count();
    let mut records = Vec::new();
    super::reserved(records.try_reserve_exact(count))?;
    records.extend(0..count);
    let truths = super::predicates::record_truths(filter.predicate(), table, &records, |_| {
        super::predicates::Operand::Values(super::buffer::Values::missing(count))
    });
    if let Some(truths) = truths {
        let mut narrowed = Vec::new();
        super::reserved(narrowed.try_reserve_exact(count))?;
        narrowed.extend(truths.iter().map(|&truth| Some(truth == Truth::True)));
        run.relations.named(relation).narrowed = Some(narrowed);
    }
    Ok(())
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
        let lowered = &run.plan.relation(relation).keys;
        evaluate_keys(run, cx, lowered, &item.keys, &current, row)
    } else {
        check_deferred_filter_types(item)?;
        ordinary_keys(run, cx, &item.path, &item.keys, row)
    }
}

/// The current row's reference for the nearest selection of `item`, read as
/// a key's match value is, or none without one (LOOKUP-26). A value whose
/// type is not its source's fails, as a key pair whose types cannot compare
/// does.
pub(super) fn reference_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    item: &Intermediate,
    row: usize,
) -> Result<Option<Value>, Fail<E>> {
    let Some(nearest) = &item.nearest else {
        return Ok(None);
    };
    let keys = std::slice::from_ref(&nearest.reference);
    let values = if nearest.reference.value.is_expression() {
        let current = resolve_operands(run, cx, &item.path, keys, row)?;
        let lowered = run.plan.relation(relation).nearest.as_slice();
        evaluate_keys(run, cx, lowered, keys, &current, row)?
    } else {
        ordinary_keys(run, cx, &item.path, keys, row)?
    };
    let value = values
        .into_iter()
        .next()
        .map_or(Value::Missing, |(_, value)| value);
    let expected = nearest.source_type();
    match value.value_type() {
        Some(actual) if actual != expected => Err(Box::new(ExecutionError::ScalarDiagnostic(
            crate::diagnostic::lookup::nearest_reference_type(
                &nearest.reference.path,
                crate::diagnostic::lookup::Owner::Intermediate(&item.identifier),
                expected.into(),
                actual.into(),
            ),
        ))),
        _ => Ok(Some(value)),
    }
}

/// The records of a named intermediate's source whose stored keys equal the
/// match values, in stored order: from the source's key index when every
/// pair can be answered exactly, otherwise from a scan that refuses a key
/// pair whose types cannot compare as the match would, on the same record.
pub(super) fn matched_records<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    item: &Intermediate,
    keys: &[(usize, Value)],
    records: &mut Vec<usize>,
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let secondary = &plan.region(cx.region).secondary;
    let table = source_table(item, cx);
    let probe_keys: Vec<(usize, ValueRef<'_>)> = keys
        .iter()
        .map(|(column, value)| (*column, ValueRef::from(value)))
        .collect();
    if probe_keys
        .iter()
        .any(|(_, value)| matches!(value, ValueRef::Missing))
    {
        return Ok(());
    }
    let exact = probe_keys.iter().all(|&(column, value)| {
        index::exact_pair(
            value,
            table
                .schema()
                .columns()
                .get(column)
                .map(|field| field.kind.into()),
        )
    });
    if exact {
        if run.relations.named(relation).source_index.is_none() {
            let columns: Vec<usize> = item.keys.iter().map(|key| key.source_column).collect();
            let kinds = schema_kinds(table, columns.iter().copied());
            let built = Index::keys(&columns, &kinds, 0..table.row_count(), |row, column| {
                cell(table, item, row, column)
            })?;
            run.relations.named(relation).source_index = Some(built);
        }
        if let Some(Some(index)) = &run.relations.named(relation).source_index {
            records.extend_from_slice(index.matching_pairs(&probe_keys));
            return Ok(());
        }
    }
    let kinds: Vec<Option<ValueType>> = keys
        .iter()
        .map(|(column, _)| item.source_kind(secondary, *column).map(Into::into))
        .collect();
    let probe = Probe {
        keys: &probe_keys,
        kinds: &kinds,
        range: None,
    };
    let found = candidates(
        0..table.row_count(),
        &probe,
        |source_row, column| cell(table, item, source_row, column),
        |_, _: &mut ()| Ok(true),
        &mut (),
    )
    .map_err(|found| refusal(found, item, table, &probe_keys))?;
    records.extend(found.rows);
    Ok(())
}

/// The finding of a named intermediate's match that stopped before its
/// candidates were complete.
fn refusal<E>(
    refusal: Refusal<E>,
    item: &Intermediate,
    table: Table<'_, E>,
    keys: &[(usize, ValueRef<'_>)],
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

/// The declared types of the donor columns that key pairs compare with.
fn schema_kinds<E>(
    table: Table<'_, E>,
    columns: impl Iterator<Item = usize>,
) -> Vec<Option<ValueType>> {
    columns
        .map(|column| {
            table
                .schema()
                .columns()
                .get(column)
                .map(|field| field.kind.into())
        })
        .collect()
}

/// The records whose keys equal the match values, from a key index when
/// every pair can be answered exactly, or none for the scan to find. An
/// input, primary or SELF relation's index is the relation's; a derived
/// relation's records are built once and own theirs. Derivations that read
/// the current record build records per row, which are scanned.
fn key_bucket<E>(
    run: &mut Run<'_, '_, E>,
    relation: RelationId,
    item: &Intermediate,
    table: Table<'_, E>,
    donor: Option<&Donor>,
    keys: &[(usize, ValueRef<'_>)],
) -> Result<Option<Vec<usize>>, Fail<E>> {
    let schema = table.schema().columns();
    if item.keys.is_empty()
        || !keys.iter().all(|&(column, value)| {
            index::exact_pair(value, schema.get(column).map(|field| field.kind.into()))
        })
    {
        return Ok(None);
    }
    let build = || {
        let columns: Vec<usize> = item.keys.iter().map(|key| key.source_column).collect();
        let kinds = schema_kinds(table, columns.iter().copied());
        Index::keys(&columns, &kinds, 0..table.row_count(), |row, column| {
            cell(table, item, row, column)
        })
    };
    match (&item.derivations, donor) {
        (None, _) => {
            if run.relations.named(relation).index.is_none() {
                let built = build()?;
                run.relations.named(relation).index = Some(built);
            }
            Ok(run
                .relations
                .named(relation)
                .index
                .as_ref()
                .and_then(Option::as_ref)
                .map(|index| index.matching_pairs(keys).to_vec()))
        }
        (Some(derived), Some(donor)) if derived.current.is_empty() => Ok(donor
            .index(build)?
            .map(|index| index.matching_pairs(keys).to_vec())),
        _ => Ok(None),
    }
}

/// A correlated filter over one donor record and the current row.
fn correlated<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    reading: &Reading<'_>,
    filter: &RelationPredicate,
    table: Table<'_, E>,
    source_row: usize,
) -> Result<Truth, Fail<E>> {
    let Reading {
        site, item, row, ..
    } = *reading;
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
                            IntermediateSource::SelfRows { .. } | IntermediateSource::Pairs(_) => {
                                ExecutionError::SchemaMismatch
                            }
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
        // Binding refuses every row's read of it, and a donor plan rebases it.
        CurrentRead::Contextual => Err(Box::new(ExecutionError::SchemaMismatch)),
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
        let lowered = &run.plan.relation(relation).keys;
        evaluate_keys(run, cx, lowered, keys, &current, row)
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

/// Evaluate every key before considering missingness, each expression key by
/// its node in `lowered`, which holds the keys' lowered values in order.
fn evaluate_keys<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    lowered: &[RelationKey],
    keys: &[crate::dataset::IntermediateKey],
    current: &[(&MatchRead, Value)],
    row: usize,
) -> Result<Vec<(usize, Value)>, Fail<E>> {
    let value_of = |read: &MatchRead| -> Value {
        current
            .iter()
            .find(|(bound, _)| bound.identifier() == read.identifier())
            .expect("all actual references resolved once")
            .1
            .clone()
    };
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
            order = compare(read(row, term.column)?, read(best, term.column)?, term)
                .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
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

/// The candidate nearest the reference (LOOKUP-26), and how many candidates
/// had a distance: one whose distance is missing is never chosen, and a tie
/// goes to the earlier `source`, then to the earlier stored record.
pub(super) fn nearest<'t, E>(
    candidates: &[usize],
    nearest: &Nearest,
    reference: &Value,
    mut read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
) -> Result<(Option<usize>, usize), Fail<E>> {
    // Distances share one unit and sources one type, so both compare.
    let order = |left: &Value, right: &Value| {
        crate::value::compare_present(left, right)
            .map_err(|_| Box::new(ExecutionError::SchemaMismatch))
    };
    let mut best: Option<(usize, Value, Value)> = None;
    let mut measured = 0;
    for &row in candidates {
        let source = owned(read(row, nearest.source)?);
        let distance = nearest
            .distance(&source, reference)
            .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)))?;
        if matches!(distance, Value::Missing) {
            continue;
        }
        measured += 1;
        let nearer = match &best {
            None => true,
            Some((chosen, chosen_distance, chosen_source)) => {
                order(&distance, chosen_distance)?
                    .then(order(&source, chosen_source)?)
                    .then_with(|| row.cmp(chosen))
                    == Ordering::Less
            }
        };
        if nearer {
            best = Some((row, distance, source));
        }
    }
    Ok((best.map(|(row, ..)| row), measured))
}

/// Compare two values of one order term (VALUE-22): missing values by the
/// term's placement, and present ones in the term's direction under VALUE-19,
/// whose one comparison (`value::compare_present_refs`) orders a mixed `int`
/// and `float` pair by exact value. Binding refuses a term whose type holds
/// two types that do not compare (`ResultType::unordered`); a pair whose
/// types no source decided before the run fails here, and is never ordered.
pub(super) fn compare(
    left: ValueRef<'_>,
    right: ValueRef<'_>,
    term: &OrderTerm,
) -> Result<Ordering, crate::value::ComparisonError> {
    Ok(match (left, right) {
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
            let order = crate::value::compare_present_refs(left, right)?;
            if term.descending {
                order.reverse()
            } else {
                order
            }
        }
    })
}

/// Sort `items` by the order terms (VALUE-22), `value` reading an item's
/// value of the term at an index; items equal on every term keep their
/// order (VALUE-23). Every two present values of a term must compare
/// ([`comparable`]), or the sort fails before any item moves, so the order
/// the sort reads is total.
pub(super) fn sort<'v, T>(
    items: &mut [T],
    terms: &[OrderTerm],
    value: impl Fn(&T, usize) -> ValueRef<'v>,
) -> Result<(), crate::value::ComparisonError> {
    comparable(items, terms.len(), &value)?;
    items.sort_by(|left, right| {
        terms
            .iter()
            .enumerate()
            .map(|(index, term)| {
                // Every pair compares: `comparable` admitted them all.
                compare(value(left, index), value(right, index), term).unwrap_or(Ordering::Equal)
            })
            .find(|order| order.is_ne())
            .unwrap_or(Ordering::Equal)
    });
    Ok(())
}

/// Fails unless every two present values of each of the `terms` order terms
/// compare under VALUE-19 (`value::comparable_types`). Comparability holds
/// within `int` and `float` and otherwise within one type, `bool` aside, so
/// each value is checked against the term's first present value.
pub(super) fn comparable<'v, T>(
    items: &[T],
    terms: usize,
    value: impl Fn(&T, usize) -> ValueRef<'v>,
) -> Result<(), crate::value::ComparisonError> {
    for index in 0..terms {
        let mut first = None;
        for item in items {
            if let Some(kind) = value(item, index).value_type() {
                if !crate::value::comparable_types(*first.get_or_insert(kind), kind) {
                    return Err(crate::value::ComparisonError::IncompatibleTypes);
                }
            }
        }
    }
    Ok(())
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
