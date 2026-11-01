//! Implicit joins (LOOKUP-1): a read through a dataset-qualified source
//! matches, narrows and selects on its declared port and answers a row
//! without a record (LOOKUP-24).
//!
//! A read over several rows visits them in order, and matches each
//! distinct combination of match values once: a row whose match values
//! equal an earlier row's takes that row's record, since the match reads
//! nothing else of the row. Each row is then answered as one-row reads
//! answer it: its handler counted, or its failure raised, in row order, so
//! the first failure and the counts before it are those of one-row reads.
//! A read without a range or a filter keeps the records it chose for its
//! selection, and the next read of the same match over the same rows, such
//! as another field of the same donor, answers from them.
use super::buffer::Values;
use super::eval::owned;
use super::frame::Identity;
use super::index::{exact, Combinations};
use super::relations::{candidates, select, Probe, Refusal, Site};
use super::{Cx, Fail, Run, Table};
use crate::bound_expression::BoundPredicate;
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, Op};
use crate::dataset::{AggregateSource, JoinAbsence, JoinedRead, RelationRange, SourceSelection};
use crate::numeric_lifecycle::HandlerKind;
use crate::table::{TableAccess, ValueRef};
use crate::value::{Value, ValueType};

/// Where a read through an implicit join is made, and so what answers a row
/// the join finds no record for.
#[derive(Clone, Copy)]
pub(super) enum JoinSite<'s> {
    /// A source binding or a bound operand: its own answer, counted at its
    /// path unless it fails.
    Read(&'s JoinAbsence),
    /// A formula operand read at this path: missing, counted under `absent`.
    Operand(&'s str),
    /// A read made while a predicate is evaluated, or a plan-level lookup:
    /// missing, uncounted; the path locates an `unmatched_key` failure.
    Uncounted(&'s str),
}

impl<'s> JoinSite<'s> {
    fn path(self) -> &'s str {
        match self {
            Self::Read(absence) => &absence.path,
            Self::Operand(path) | Self::Uncounted(path) => path,
        }
    }
}

/// A source binding's options on its implicit join.
#[derive(Clone, Copy, Default)]
pub(super) struct Options<'o> {
    pub range: Option<&'o RelationRange>,
    pub filter: Option<&'o BoundPredicate>,
    pub selection: Option<&'o SourceSelection>,
}

/// Where one match value of a row comes from: a completed output column,
/// or a raw field of the row's driver record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum From {
    Output(usize),
    Record(usize),
}

/// One join read: its donor, the donor field it answers, its key pairs
/// (each a donor column and where the row's match value comes from), the
/// range, filter and selection that narrow and choose the record, and the
/// site that answers a row without one.
struct Shape<'s, 't, E> {
    donor: Donor<'t, 's, E>,
    column: usize,
    keys: Vec<(usize, From)>,
    /// The donor columns of the key pairs, in order.
    columns: Vec<usize>,
    options: Options<'s>,
    site: JoinSite<'s>,
}

impl<'s, 't, E> Shape<'s, 't, E> {
    /// The read of a qualified implicit join: row templates match raw
    /// driver fields, column joins match completed output keys.
    fn joined(
        cx: &Cx<'t, E>,
        joined: &'s JoinedRead,
        options: Options<'s>,
        site: JoinSite<'s>,
    ) -> Self {
        let (source, table) = match joined.source {
            AggregateSource::Primary => (usize::MAX, cx.primary),
            AggregateSource::Secondary(index) => (index, cx.secondary[index]),
        };
        let keys: Vec<(usize, From)> = joined
            .keys
            .iter()
            .enumerate()
            .map(|(index, key)| {
                let from = match &joined.record_keys {
                    Some(records) => From::Record(records[index]),
                    None => From::Output(key.output_column),
                };
                (key.source_column, from)
            })
            .collect();
        Self {
            donor: Donor {
                table,
                source,
                name: Some(&joined.name),
            },
            column: joined.column,
            columns: keys.iter().map(|&(column, _)| column).collect(),
            keys,
            options,
            site,
        }
    }

    /// The read a join node makes.
    fn node(cx: &Cx<'t, E>, op: &'s Op, path: &'s str) -> Self {
        let site = |absence: &'s Option<JoinAbsence>| {
            absence
                .as_ref()
                .map_or(JoinSite::Uncounted(path), JoinSite::Read)
        };
        match op {
            Op::Joined(read) => Self::joined(cx, read, Options::default(), JoinSite::Operand(path)),
            Op::JoinedSource {
                read,
                range,
                filter,
                selection,
                absence,
            } => Self::joined(
                cx,
                read,
                Options {
                    range: range.as_ref(),
                    filter: filter.as_ref(),
                    selection: selection.as_ref(),
                },
                site(absence),
            ),
            Op::SourceLookup {
                lookup,
                range,
                filter,
                selection,
                absence,
            } => Self {
                donor: Donor {
                    table: cx.secondary[lookup.source],
                    source: lookup.source,
                    name: None,
                },
                column: lookup.column,
                keys: lookup
                    .keys
                    .iter()
                    .map(|key| (key.source_column, From::Output(key.output_column)))
                    .collect(),
                columns: lookup.keys.iter().map(|key| key.source_column).collect(),
                options: Options {
                    range: range.as_ref(),
                    filter: filter.as_ref(),
                    selection: selection.as_ref(),
                },
                site: site(absence),
            },
            _ => unreachable!("a join node holds its read"),
        }
    }

    /// What another read must match to share this one's records: the same
    /// donor, key pairs and selection, with no range or filter, whose
    /// answers are the read's own.
    fn signature(&self) -> Option<Signature> {
        (self.options.range.is_none() && self.options.filter.is_none()).then(|| Signature {
            source: self.donor.source,
            keys: self.keys.clone(),
            selection: self.options.selection.cloned(),
        })
    }
}

/// The record a row's match chose: none, the one it matched, or the one
/// the selection chose among several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pick {
    Absent,
    One(usize),
    Several(usize),
}

/// The match a shared selection's records answer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Signature {
    source: usize,
    keys: Vec<(usize, From)>,
    selection: Option<SourceSelection>,
}

/// The records the last shareable join read chose for its selection, by
/// the rows' identities, kept in the region's relation state and dropped
/// with its other choices.
pub(super) struct Shared {
    signature: Signature,
    identities: Vec<Identity>,
    picks: Vec<Pick>,
}

/// An implicit join node over rows.
pub(super) fn join<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let node = plan.expr(expr);
    let shape = Shape::node(cx, &node.op, &node.path);
    read_rows(run, cx, Site::node(plan, expr), &shape, rows)
}

/// One row's read through a qualified implicit join: row templates match
/// raw driver fields, column joins match completed output keys. `owner` is
/// the node whose path names the read.
pub(super) fn joined_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    joined: &JoinedRead,
    options: Option<Options<'_>>,
    site: JoinSite<'_>,
    row: usize,
) -> Result<Value, Fail<E>> {
    let shape = Shape::joined(cx, joined, options.unwrap_or_default(), site);
    Ok(read_rows(run, cx, owner, &shape, &[row])?.value(0))
}

/// A join read over `rows`, in order: each row's record, matched once per
/// distinct combination of match values or taken from a shared selection,
/// then its answer, counted or refused. The handler counts gathered before
/// a failure are counted before it is raised.
fn read_rows<'c, E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'c, E>,
    owner: Site<'_>,
    shape: &Shape<'_, 'c, E>,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let signature = match rows {
        [_, _, ..] if cx.record.is_none() && cx.operands.is_none() => shape.signature(),
        _ => None,
    };
    let identities: Vec<Identity> = match signature {
        Some(_) => rows.iter().map(|&row| cx.frame.identity(row)).collect(),
        None => Vec::new(),
    };
    let shared = signature.as_ref().and_then(|signature| {
        run.relations
            .take_joined(|held| held.signature == *signature && held.identities == identities)
    });
    let mut answers = Answers::new(run, cx, owner, shape, rows.len())?;
    let outcome = match &shared {
        Some(held) => answers.all(run, cx, rows, |_, _, position, _| Ok(held.picks[position])),
        None => {
            let mut matcher = Matcher::new(cx, shape, rows)?;
            answers.all(run, cx, rows, |run, cx, position, row| {
                matcher.pick(run, cx, owner, shape, (position, row))
            })
        }
    };
    let fired = answers.fire(run, owner);
    outcome?;
    fired?;
    let (values, picks) = answers.finish()?;
    match (shared, signature) {
        (Some(held), _) => run.relations.keep_joined(held),
        (None, Some(signature)) => run.relations.keep_joined(Shared {
            signature,
            identities,
            picks,
        }),
        (None, None) => (),
    }
    Ok(values)
}

/// Each distinct combination of a read's match values matched once: the
/// previous row's combination, then every earlier one, before a match.
struct Matcher<'k> {
    kinds: Vec<Option<ValueType>>,
    probe: Vec<(usize, ValueRef<'k>)>,
    /// The record of each combination of match values met, when rows may
    /// share a match: several rows and no range, whose value is the row's
    /// own.
    combinations: Option<Combinations<'k, Pick>>,
    /// Per row, whether every match value, each read from a completed
    /// output column, repeats the previous row's: such a row takes the
    /// previous row's record without reading them.
    repeats: Vec<bool>,
}

impl<'k> Matcher<'k> {
    fn new<E>(cx: &Cx<'k, E>, shape: &Shape<'_, '_, E>, rows: &[usize]) -> Result<Self, Fail<E>> {
        let schema = shape.donor.table.schema().columns();
        let remembers = rows.len() > 1 && shape.options.range.is_none();
        let mut repeats = Vec::new();
        let outputs = shape
            .keys
            .iter()
            .all(|(_, from)| matches!(from, From::Output(_)));
        if remembers && outputs {
            for &(_, from) in &shape.keys {
                let From::Output(column) = from else {
                    continue;
                };
                let column = super::reserved(cx.frame.column(column).repeats(rows))?;
                if repeats.is_empty() {
                    repeats = column;
                } else {
                    repeats
                        .iter_mut()
                        .zip(column)
                        .for_each(|(all, repeat)| *all &= repeat);
                }
            }
        }
        Ok(Self {
            kinds: shape
                .keys
                .iter()
                .map(|&(column, _)| schema.get(column).map(|field| field.kind.into()))
                .collect(),
            probe: Vec::with_capacity(shape.keys.len()),
            combinations: remembers.then(|| Combinations::new(shape.keys.len())),
            repeats,
        })
    }

    /// One row's record: its range value, then its match values, then the
    /// match, as a one-row read resolves them.
    fn pick<E>(
        &mut self,
        run: &mut Run<'_, '_, E>,
        cx: &Cx<'k, E>,
        owner: Site<'_>,
        shape: &Shape<'_, 'k, E>,
        (position, row): (usize, usize),
    ) -> Result<Pick, Fail<E>> {
        if self.repeats.get(position).copied().unwrap_or(false) {
            if let Some(pick) = self.combinations.as_ref().and_then(Combinations::again) {
                return Ok(pick);
            }
        }
        let range = match shape.options.range {
            Some(range) => Some((
                range,
                super::relations::current_value(
                    run,
                    cx,
                    owner,
                    range.current(),
                    range.path(),
                    Some(row),
                )?,
            )),
            None => None,
        };
        self.probe.clear();
        for &(column, from) in &shape.keys {
            self.probe
                .push((column, match_value(cx, owner.path, from, row)?));
        }
        if let Some(combinations) = &mut self.combinations {
            if let Some(pick) = combinations.find(self.probe.iter().map(|&(_, value)| value)) {
                return Ok(pick);
            }
        }
        let probe = Probe {
            keys: &self.probe,
            kinds: &self.kinds,
            range: range
                .as_ref()
                .map(|(range, value)| (*range, ValueRef::from(value))),
        };
        let pick = resolve(run, cx, owner, shape, &probe, row)?;
        if let Some(combinations) = &mut self.combinations {
            combinations.remember(pick)?;
        }
        Ok(pick)
    }
}

/// One match value of a row, lent by the frame or the row's driver record.
fn match_value<'c, E>(
    cx: &Cx<'c, E>,
    path: &str,
    from: From,
    row: usize,
) -> Result<ValueRef<'c>, Fail<E>> {
    match from {
        From::Output(column) => Ok(cx.frame.column(column).get(row)),
        From::Record(column) => {
            let source_row = cx.frame.members(row)[0];
            cx.table.get(source_row, column).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: path.to_string(),
                    source_row,
                    error,
                })
            })
        }
    }
}

/// The rows' answers under construction, in row order: each row's chosen
/// record, the values read from them, and the handlers counted.
struct Answers<'a, 's, 't, E> {
    shape: &'a Shape<'s, 't, E>,
    /// The path of the read's node, which counts `multiple_matches` and
    /// names a failing cell.
    path: &'a str,
    /// The donor field's typed storage, gathered once every row is
    /// answered; otherwise each row's value is read as it is answered.
    storage: Option<crate::table::ColumnRef<'t>>,
    values: Values,
    /// With typed storage, each row's chosen record, or `usize::MAX`.
    sources: Vec<usize>,
    picks: Vec<Pick>,
    /// The answer of a row without a record, and whether it fails instead.
    absent: Option<Value>,
    required: bool,
    dataset: String,
    /// Whether the read counts `absent`, and the site paths it counts at.
    counts: bool,
    absent_path: &'a str,
    /// Firings gathered so far, of each kind, in the order each first fired.
    fired: Vec<(HandlerKind, u64)>,
}

impl<'a, 's, 't, E> Answers<'a, 's, 't, E> {
    fn new(
        run: &Run<'_, '_, E>,
        cx: &Cx<'_, E>,
        owner: Site<'a>,
        shape: &'a Shape<'s, 't, E>,
        rows: usize,
    ) -> Result<Self, Fail<E>>
    where
        's: 'a,
    {
        let plan = run.plan;
        let dataset = donor_name(plan, cx, &shape.donor);
        let input = plan.region(cx.region).required_matches.contains(&dataset);
        let (absent_path, absent, required, counts) = match shape.site {
            JoinSite::Read(absence) => (
                absence.path.as_str(),
                absence.absent.clone(),
                absence.requires_match(input),
                true,
            ),
            JoinSite::Operand(path) => (path, None, input, true),
            JoinSite::Uncounted(path) => (path, None, input, false),
        };
        let storage = shape.donor.table.lent(shape.column, rows);
        let mut sources = Vec::new();
        let mut picks = Vec::new();
        super::reserved(picks.try_reserve_exact(rows))?;
        if storage.is_some() {
            super::reserved(sources.try_reserve_exact(rows))?;
        }
        Ok(Self {
            shape,
            path: owner.path,
            storage,
            values: Values::missing(0),
            sources,
            picks,
            absent,
            required,
            dataset,
            counts,
            absent_path,
            fired: Vec::new(),
        })
    }

    /// Answer every row in order from the record `pick` chooses for it.
    fn all<'c>(
        &mut self,
        run: &mut Run<'_, '_, E>,
        cx: &Cx<'c, E>,
        rows: &[usize],
        mut pick: impl FnMut(&mut Run<'_, '_, E>, &Cx<'c, E>, usize, usize) -> Result<Pick, Fail<E>>,
    ) -> Result<(), Fail<E>> {
        for (position, &row) in rows.iter().enumerate() {
            let chosen = pick(run, cx, position, row)?;
            self.answer(run, cx, row, chosen)?;
        }
        Ok(())
    }

    fn count(&mut self, kind: HandlerKind) {
        match self.fired.iter_mut().find(|(fired, _)| *fired == kind) {
            Some((_, count)) => *count += 1,
            None => self.fired.push((kind, 1)),
        }
    }

    /// One row's answer (LOOKUP-24): the chosen record's field, counted
    /// under `multiple_matches` when the selection chose it among several;
    /// or the declared `absent` value, missing by default, counted at the
    /// read's path except during a predicate; or `unmatched_key` when the
    /// read, or its input, requires a match and the read declares no
    /// `absent`.
    fn answer(
        &mut self,
        run: &Run<'_, '_, E>,
        cx: &Cx<'_, E>,
        row: usize,
        pick: Pick,
    ) -> Result<(), Fail<E>> {
        let source = match pick {
            Pick::Absent => {
                if self.required {
                    return Err(self.unmatched(run, cx, row));
                }
                if self.counts {
                    self.count(HandlerKind::Absent);
                }
                None
            }
            Pick::One(source) => Some(source),
            Pick::Several(source) => {
                self.count(HandlerKind::MultipleMatches);
                Some(source)
            }
        };
        match (self.storage.is_some(), source) {
            (true, source) => self.sources.push(source.unwrap_or(usize::MAX)),
            (false, Some(source)) => {
                let shape = self.shape;
                let value = cell(&shape.donor, self.path, source, shape.column)?;
                self.values.push(value);
            }
            (false, None) => self.values.push(
                self.absent
                    .as_ref()
                    .map_or(ValueRef::Missing, ValueRef::from),
            ),
        }
        self.picks.push(pick);
        Ok(())
    }

    /// The `unmatched_key` failure of a row, with its match values.
    fn unmatched(&self, run: &Run<'_, '_, E>, cx: &Cx<'_, E>, row: usize) -> Fail<E> {
        let shape = self.shape;
        let keys: Vec<(usize, ValueRef<'_>)> = shape
            .keys
            .iter()
            .map(|&(column, from)| {
                let value = match from {
                    From::Output(output) => cx.frame.column(output).get(row),
                    // Read already for this row's match.
                    From::Record(field) => cx
                        .table
                        .get(cx.frame.members(row)[0], field)
                        .unwrap_or(ValueRef::Missing),
                };
                (column, value)
            })
            .collect();
        let dataset = self.dataset.clone();
        Box::new(ExecutionError::UnmatchedKey {
            path: self.absent_path.into(),
            intermediate: format!("intermediate({dataset})"),
            dataset,
            matched_key: matched_key(shape.donor.table, &keys),
            identity: run.failure_identity(cx, row),
        })
    }

    /// Count the firings gathered so far at the read's sites.
    fn fire(&mut self, run: &mut Run<'_, '_, E>, owner: Site<'_>) -> Result<(), Fail<E>> {
        for (kind, count) in std::mem::take(&mut self.fired) {
            let path = match kind {
                HandlerKind::Absent => format!("{}.absent", self.absent_path),
                _ => format!("{}.multiple_matches", self.path),
            };
            run.fire_many(owner.expr, &path, kind, count)?;
        }
        Ok(())
    }

    /// The rows' values, gathered from the donor's typed storage when it
    /// holds them, and the rows' picks.
    fn finish(self) -> Result<(Values, Vec<Pick>), Fail<E>> {
        let Some(storage) = self.storage else {
            return Ok((self.values, self.picks));
        };
        if !self.sources.contains(&usize::MAX) {
            if let Some(values) = Values::from_column(storage, &self.sources) {
                return Ok((super::reserved(values)?, self.picks));
            }
        }
        let absent = self
            .absent
            .as_ref()
            .map_or(ValueRef::Missing, ValueRef::from);
        let mut values = Values::missing(0);
        for &source in &self.sources {
            values.push(if source == usize::MAX {
                absent
            } else {
                storage.get(source)
            });
        }
        Ok((values, self.picks))
    }
}

/// The donor of one implicit join: its port, its position (`usize::MAX`
/// for the primary) and its name when the read declares one.
struct Donor<'t, 'n, E> {
    table: Table<'t, E>,
    source: usize,
    name: Option<&'n str>,
}

/// Retain the donor and its row coordinates on opaque port failures.
fn cell<'t, E>(
    donor: &Donor<'t, '_, E>,
    path: &str,
    source_row: usize,
    column: usize,
) -> Result<ValueRef<'t>, Fail<E>> {
    donor.table.get(source_row, column).map_err(|error| {
        Box::new(if donor.source == usize::MAX {
            ExecutionError::Cell {
                path: path.into(),
                source_row,
                error,
            }
        } else {
            ExecutionError::SecondaryCell {
                path: path.into(),
                source: donor.source,
                source_row,
                error,
            }
        })
    })
}

/// Match, narrow and select on the implicit join (LOOKUP-25) for one row's
/// match values `keys` and range value, preserving raw match-key evidence.
fn resolve<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    shape: &Shape<'_, '_, E>,
    probe: &Probe<'_>,
    row: usize,
) -> Result<Pick, Fail<E>> {
    let Probe { keys, kinds, range } = *probe;
    let donor = &shape.donor;
    let filter = shape.options.filter;
    let path = owner.path;
    let table = donor.table;
    if probe.incomplete() {
        return Ok(Pick::Absent);
    }
    // A bucket's records are the candidates when no range or filter narrows
    // them: one or none is the pick, read from the index where it lies.
    if range.is_none() && filter.is_none() && !keys.is_empty() && exact(keys, kinds) {
        // An index built now reads each key column from the port's storage
        // of it, lent once, where the port holds one.
        let mut lent: Option<Vec<Option<crate::table::ColumnRef<'_>>>> = None;
        let read = |source_row: usize, field: usize| {
            let lent = lent.get_or_insert_with(|| {
                shape
                    .columns
                    .iter()
                    .map(|&column| table.storage(column))
                    .collect()
            });
            let storage = shape
                .columns
                .iter()
                .position(|&column| column == field)
                .and_then(|position| lent[position]);
            match storage {
                Some(storage) if source_row < storage.len() => Ok(storage.get(source_row)),
                _ => cell(donor, path, source_row, field),
            }
        };
        let held = run.ports.keys(table, &shape.columns, read)?.map(|index| {
            match index.matching_pairs(keys) {
                [] => Err(Pick::Absent),
                [one] => Err(Pick::One(*one)),
                rows => Ok(rows.to_vec()),
            }
        });
        match held {
            Some(Err(found)) => return Ok(found),
            Some(Ok(rows)) => return pick(run, cx, owner, shape, probe, rows, row),
            None => (),
        }
    }
    // The join operator (#2842): the records whose keys equal the match
    // values come from the port's key index when every pair can be answered
    // exactly; the range and the filter then run on them in stored order.
    let bucket = bucket(run, table, keys, kinds, |source_row, field| {
        cell(donor, path, source_row, field)
    })?;
    let indexed = Probe {
        keys: &[],
        kinds: &[],
        range: probe.range,
    };
    let (rows, probe) = match &bucket {
        Some(rows) => (Rows::Bucket(rows.iter()), &indexed),
        None => (Rows::Scan(0..table.row_count()), probe),
    };
    let expr = owner.expr;
    let found = candidates(
        rows,
        probe,
        |source_row, field| cell(donor, path, source_row, field),
        |source_row, run: &mut Run<'_, '_, E>| match filter {
            None => Ok(true),
            Some(filter) => {
                let evaluate = || {
                    super::predicates::row_truth(filter, table, source_row, |_| ValueRef::Missing)
                        .map(|truth| truth == crate::expression::Truth::True)
                        .map_err(|error| {
                            Box::new(ExecutionError::Predicate {
                                source_row,
                                error: *error,
                            })
                        })
                };
                match expr {
                    // The filter reads only the record: one answer each.
                    Some(expr) => run.ports.kept(expr, table, source_row, evaluate),
                    None => evaluate(),
                }
            }
        },
        run,
    )
    .map_err(|refusal| match refusal {
        Refusal::Failed(error) => error,
        Refusal::Key {
            index,
            current,
            donor,
        } => Box::new(ExecutionError::ScalarDiagnostic(
            crate::diagnostic::lookup::key_type(
                shape.site.path(),
                crate::diagnostic::lookup::Owner::Unnamed,
                &table.schema().columns()[keys[index].0].name,
                crate::result_type::ResultType::known(current),
                crate::result_type::ResultType::known(donor),
            ),
        )),
        Refusal::Range(types) => Box::new(ExecutionError::ScalarDiagnostic(
            types.diagnostic_for(range.map_or("", |(range, _)| range.path()), None),
        )),
    })?;
    pick(run, cx, owner, shape, probe, found.rows, row)
}

/// The record a row's candidates give (LOOKUP-25): none, the one, or the
/// one the selection chooses among several; several without a selection
/// fail as `multiple_matches`.
fn pick<E>(
    run: &Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    shape: &Shape<'_, '_, E>,
    probe: &Probe<'_>,
    candidates: Vec<usize>,
    row: usize,
) -> Result<Pick, Fail<E>> {
    let donor = &shape.donor;
    let path = owner.path;
    let count = candidates.len();
    if count == 0 {
        return Ok(Pick::Absent);
    }
    if count == 1 {
        return Ok(Pick::One(candidates[0]));
    }
    let Some(selection) = shape.options.selection else {
        let name = donor_name(run.plan, cx, donor);
        return Err(Box::new(ExecutionError::MultipleMatches {
            path: path.to_string(),
            dataset: name.clone(),
            intermediate: format!("intermediate({name})"),
            match_count: count,
            matched_key: matched_key(donor.table, probe.keys),
            identity: run.failure_identity(cx, row),
        }));
    };
    let chosen = select(&candidates, selection, |source_row, field| {
        cell(donor, path, source_row, field)
    })?;
    Ok(Pick::Several(chosen))
}

/// A failure's evidence of the match: each key pair's donor column and the
/// row's match value.
fn matched_key<E>(table: Table<'_, E>, keys: &[(usize, ValueRef<'_>)]) -> Vec<(String, Value)> {
    keys.iter()
        .map(|&(column, value)| (table.schema().columns()[column].name.clone(), owned(value)))
        .collect()
}

/// The records a match reads, in stored order: an index bucket, or every
/// record for the scan.
pub(super) enum Rows<'b> {
    Bucket(std::slice::Iter<'b, usize>),
    Scan(std::ops::Range<usize>),
}

impl Iterator for Rows<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        match self {
            Self::Bucket(rows) => rows.next().copied(),
            Self::Scan(rows) => rows.next(),
        }
    }
}

/// The records of a port whose keys equal the match values, from the port's
/// key index when every pair can be answered exactly, or none for the scan.
pub(super) fn bucket<'t, E>(
    run: &mut Run<'_, '_, E>,
    table: Table<'t, E>,
    keys: &[(usize, ValueRef<'_>)],
    kinds: &[Option<ValueType>],
    read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
) -> Result<Option<Vec<usize>>, Fail<E>> {
    if keys.is_empty() || !exact(keys, kinds) {
        return Ok(None);
    }
    let columns: Vec<usize> = keys.iter().map(|(column, _)| *column).collect();
    Ok(run
        .ports
        .keys(table, &columns, read)?
        .map(|index| index.matching_pairs(keys).to_vec()))
}

/// The name a join's donor is known by in diagnostics.
fn donor_name<E>(
    plan: &crate::dataset::resolved::ResolvedPlan,
    cx: &Cx<'_, E>,
    donor: &Donor<'_, '_, E>,
) -> String {
    donor
        .name
        .map(String::from)
        .unwrap_or_else(|| plan.region(cx.region).secondary[donor.source].name.clone())
}
