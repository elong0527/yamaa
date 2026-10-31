//! Donor materialization: a derived intermediate's records, each stored
//! field copied and every derivation evaluated in declaration order over the
//! donor records, on the same units as the dataset. Derivations that read
//! the current record rebuild the donor records for each current row and
//! augment only the records whose stored keys match it (LOOKUP-15); every
//! donor handler counts each time the records are built.
use super::buffer::Values;
use super::frame::{Frame, Identity};
use super::index::Index;
use super::relations::{self, Site};
use super::{reserved, Cx, Fail, Footprint, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{RegionId, RelationId};
use crate::dataset::{Intermediate, IntermediateDerivations, IntermediateSource};
use crate::limits::Limits;
use crate::table::{TableAccess, TableSchema};
use crate::value::Value;
use std::cell::OnceCell;
use std::rc::Rc;

/// A materialized donor: its completed records, and the key index over
/// them once a match builds it.
pub(super) struct Donor {
    frame: Frame,
    schema: TableSchema,
    index: OnceCell<Option<Index>>,
}

/// The records a read selects among: a donor's, or the range of a shared
/// donor that holds the matched records of several current rows.
#[derive(Clone)]
pub(super) struct Built {
    donor: Rc<Donor>,
    start: usize,
    rows: usize,
}

impl Built {
    /// Records `start..start + rows` of a shared donor.
    pub(super) fn part(donor: &Rc<Donor>, start: usize, rows: usize) -> Self {
        Self {
            donor: Rc::clone(donor),
            start,
            rows,
        }
    }

    pub(super) fn whole(donor: Rc<Donor>) -> Self {
        let rows = donor.frame.rows();
        Self {
            donor,
            start: 0,
            rows,
        }
    }

    pub(super) fn table<E>(&self) -> Table<'_, E> {
        Table::Rows {
            frame: &self.donor.frame,
            schema: &self.donor.schema,
            start: self.start,
            rows: self.rows,
        }
    }

    pub(super) fn donor(&self) -> &Donor {
        &self.donor
    }
}

impl Donor {
    /// The key index over the records, built on first use; none when a
    /// record's key cannot be indexed exactly.
    pub(super) fn index<E>(
        &self,
        build: impl FnOnce() -> Result<Option<Index>, Fail<E>>,
    ) -> Result<Option<&Index>, Fail<E>> {
        if self.index.get().is_none() {
            let built = build()?;
            let _ = self.index.set(built);
        }
        Ok(self.index.get().and_then(Option::as_ref))
    }
}

/// The donor records of a derived intermediate, shared only across contexts
/// supplying the same current row. `row` is none for a destination-free
/// probe; `site` is the read whose path a nested named read reports under.
pub(super) fn materialize<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    site: Option<Site<'_>>,
    row: Option<usize>,
) -> Result<Option<Built>, Fail<E>> {
    let plan = run.plan;
    let item = relations::item(plan, relation);
    // A pair relation's records are its matching pairs, built once a run
    // from its two sides (LOOKUP-27).
    if item.source.pairs().is_some() {
        return super::pairs::materialize(run, cx, relation).map(Some);
    }
    let Some(derived) = &item.derivations else {
        return Ok(None);
    };
    let key = (!derived.current.is_empty()).then(|| match row {
        Some(row) => cx.frame.identity(row),
        None => Identity::Free,
    });
    if let Some(data) = run.relations.materialized(relation, key) {
        return Ok(Some(data));
    }
    let current = current_fields(run, cx, item, derived, site, row)?;
    let table = input_table(item, cx);
    let data = Built::whole(build(run, cx, relation, table, &current, 0, Records::All)?);
    run.relations
        .insert_materialized(relation, key, data.clone());
    Ok(Some(data))
}

/// LOOKUP-15: the donor records that derivations reading the current record
/// augment for one current row, with the row's match values. The current
/// fields resolve first, as for every build, then the match values, and only
/// the records whose stored keys match them are augmented. The others keep
/// no derived value: no later step of the match reads one.
///
/// `compact` builds the matched records alone, at positions of their own;
/// otherwise every record keeps its source position and only the matched
/// ones are augmented, so a failure names the source row.
pub(super) fn materialize_matched<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    site: Site<'_>,
    row: usize,
    compact: bool,
) -> Result<(Built, relations::MatchValues), Fail<E>> {
    let plan = run.plan;
    let item = relations::item(plan, relation);
    let derived = item
        .derivations
        .as_ref()
        .expect("an intermediate deriving matched records has derivations");
    let current = current_fields(run, cx, item, derived, Some(site), Some(row))?;
    let keys = relations::match_values(run, cx, relation, item, row)?;
    let mut matched = Vec::new();
    relations::matched_records(run, cx, relation, item, &keys, &mut matched)?;
    let records = if compact {
        Records::Compact(&matched)
    } else {
        Records::Matched(&matched)
    };
    let table = input_table(item, cx);
    let data = Built::whole(build(run, cx, relation, table, &current, 0, records)?);
    run.relations
        .insert_materialized(relation, Some(cx.frame.identity(row)), data.clone());
    Ok((data, keys))
}

/// The current record's fields a derived intermediate's derivations read.
pub(super) fn current_fields<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    item: &Intermediate,
    derived: &IntermediateDerivations,
    site: Option<Site<'_>>,
    row: Option<usize>,
) -> Result<Vec<Value>, Fail<E>> {
    let mut current = Vec::with_capacity(derived.current.len());
    let reading = Cx {
        table: cx.primary,
        ..cx.clone()
    };
    for field in &derived.current {
        let site = site.unwrap_or(Site {
            expr: None,
            path: &item.path,
        });
        current.push(relations::current_value(
            run, &reading, site, field, site.path, row,
        )?);
    }
    Ok(current)
}

/// The donor records a build augments.
#[derive(Clone, Copy)]
enum Records<'r> {
    /// Every record.
    All,
    /// Every record at its source position, only these augmented.
    Matched(&'r [usize]),
    /// Only these records, at positions of their own.
    Compact(&'r [usize]),
}

/// The matched records of several current rows in one compact build, each
/// record with its own row's current values, `stride` per record.
pub(super) fn build_matched<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    current: &[Value],
    stride: usize,
    records: &[usize],
) -> Result<Rc<Donor>, Fail<E>> {
    let table = input_table(relations::item(run.plan, relation), cx);
    build(
        run,
        cx,
        relation,
        table,
        current,
        stride,
        Records::Compact(records),
    )
}

/// A pair relation's records (LOOKUP-27): `records` of its joined
/// records `joined`, the matching pairs its filter keeps, each at a position
/// of its own, with every derivation evaluated over them.
pub(super) fn build_pairs<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    joined: Table<'_, E>,
    records: &[usize],
) -> Result<Rc<Donor>, Fail<E>> {
    build(run, cx, relation, joined, &[], 0, Records::Compact(records))
}

/// The table a derived intermediate's stored fields come from: its input,
/// the driver, or the completed rows of earlier templates. A pair relation
/// builds its joined records itself ([`build_pairs`]).
fn input_table<'t, E>(item: &Intermediate, cx: &Cx<'t, E>) -> Table<'t, E> {
    match &item.source {
        IntermediateSource::Input(index) => cx.secondary[*index],
        IntermediateSource::Primary { .. } => cx.primary,
        IntermediateSource::SelfRows { .. } => Table::Rows {
            frame: cx.frame,
            schema: cx.schema,
            start: 0,
            rows: cx.self_rows,
        },
        IntermediateSource::Pairs(join) => cx.secondary[join.port],
    }
}

/// A derived intermediate's donor records: their stored fields copied, then
/// each derivation evaluated in declaration order for the records augmented.
/// `current` is one set of current values, or one set per record when
/// `stride` is its width.
fn build<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    table: Table<'_, E>,
    current: &[Value],
    stride: usize,
    records: Records<'_>,
) -> Result<Rc<Donor>, Fail<E>> {
    let plan = run.plan;
    let item = relations::item(plan, relation);
    let derived = item
        .derivations
        .as_ref()
        .expect("a derived intermediate has derivations");
    let region = plan
        .relation(relation)
        .derivations
        .expect("a derived intermediate has a donor region");
    let input_schema = plan.sources()[plan
        .region_sources(region)
        .next()
        .expect("a donor region has an input")
        .index()]
    .schema
    .clone();
    let output = plan.region(region).output.clone();
    Limits::table(table.row_count(), input_schema.columns().len()).map_err(|error| {
        Box::new(ExecutionError::ScopedLimit {
            resource: match error.resource {
                "columns" => crate::dataset::execution::Resource::SourceColumns,
                "table_rows" => crate::dataset::execution::Resource::SourceRows,
                "table_cells" => crate::dataset::execution::Resource::SourceCells,
                _ => unreachable!("table boundary resource"),
            },
            limit: error.limit,
            required: error.actual,
            spec_paths: vec![item.path.clone()],
        })
    })?;
    let mut frame = Frame::new(output.columns().len());
    let all: Vec<usize>;
    let stored = match records {
        Records::Compact(rows) => rows,
        Records::All | Records::Matched(_) => {
            all = (0..table.row_count()).collect();
            &all
        }
    };
    reserved(frame.try_reserve(stored.len(), stored.len()))?;
    // A record's member is its position in the donor's input; only a
    // compact build moves it from the source row.
    frame.extend(0, stored.len(), |position, records| {
        records.push(position);
        Identity::Candidate(stored[position])
    });
    // A stored column the input holds as typed storage is gathered whole
    // (#4208), and cannot fail; the others are read a cell at a time in
    // record order, so the first failing cell is the same.
    let every: Vec<usize> = (0..stored.len()).collect();
    let mut cells = Vec::new();
    for column in 0..derived.stored_columns {
        match table
            .storage(column)
            .and_then(|storage| Values::from_column(storage, stored))
        {
            Some(values) => frame.complete_rows(&every, column, reserved(values)?),
            None => cells.push(column),
        }
    }
    if !cells.is_empty() {
        for (row, &source_row) in stored.iter().enumerate() {
            for &column in &cells {
                let value = relations::cell(table, item, source_row, column)?;
                frame.complete(row, column, value);
            }
        }
    }
    // A donor plan's inputs are the outer inputs of the same name, or the primary.
    let outer = &plan.region(cx.region).secondary;
    let secondary: Vec<Table<'_, E>> = plan
        .region(region)
        .secondary
        .iter()
        .map(|source| {
            outer
                .iter()
                .position(|candidate| candidate.name == source.name)
                .map_or(cx.primary, |index| cx.secondary[index])
        })
        .collect();
    let rows = match records {
        Records::Matched(rows) => rows,
        Records::All | Records::Compact(_) => &every,
    };
    let saved_relations =
        std::mem::replace(&mut run.relations, relations::State::new(plan, region));
    let saved_windows = std::mem::take(&mut run.windows);
    let result = derive(
        run,
        region,
        &mut frame,
        rows,
        DonorInput {
            current,
            stride,
            schema: &input_schema,
            secondary: &secondary,
        },
        item,
        cx.region,
    );
    run.windows = saved_windows;
    run.relations = saved_relations;
    result?;
    Ok(Rc::new(Donor {
        frame,
        schema: output,
        index: OnceCell::new(),
    }))
}

/// What a donor's bindings read besides its frame: the current values its
/// records read (`current`, one set for every record, or one set per record
/// when `stride` is its width), the donor's input schema, and the secondary
/// ports.
pub(super) struct DonorInput<'c, 't, E> {
    pub(super) current: &'c [Value],
    pub(super) stride: usize,
    pub(super) schema: &'c TableSchema,
    pub(super) secondary: &'c [Table<'t, E>],
}

/// Evaluate every derivation over the donor records `rows`, in declaration
/// order, reading `input`.
pub(super) fn derive<E>(
    run: &mut Run<'_, '_, E>,
    region: RegionId,
    frame: &mut Frame,
    rows: &[usize],
    input: DonorInput<'_, '_, E>,
    item: &Intermediate,
    outer: RegionId,
) -> Result<(), Fail<E>> {
    let DonorInput {
        current,
        stride,
        schema: input_schema,
        secondary,
    } = input;
    let plan = run.plan;
    for &column in plan.column_order(region) {
        let binding = plan
            .column_bindings(column)
            .next()
            .expect("a donor column has a binding");
        let local = run.local_column(region, column);
        let callbacks = run.step_callbacks(region, crate::dataset::resolved::Step::Column(column));
        let columns = [local];
        let footprint = Footprint::Cells {
            columns: &columns,
            rows,
        };
        run.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
            run.windows = Default::default();
            let evaluate = |run: &mut Run<'_, '_, E>,
                            frame: &mut Frame,
                            rows: &[usize],
                            prepare: bool|
             -> Result<(), Fail<E>> {
                let values = {
                    let input = Table::Input {
                        frame,
                        current,
                        stride,
                        schema: input_schema,
                    };
                    let cx = Cx {
                        region,
                        schema: &plan.region(region).output,
                        frame,
                        table: input,
                        primary: input,
                        secondary,
                        self_rows: 0,
                        output: 0..frame.rows(),
                        operands: None,
                        record: None,
                    };
                    if prepare {
                        // A direct window over every donor record is built first;
                        // its failure is not a donor record's.
                        super::windows::prepare(run, &cx, plan.binding(binding).expr)?;
                    }
                    if rows.is_empty() {
                        return Ok(());
                    }
                    run.evaluate_binding(&cx, binding, rows)
                        .map_err(|error| failure(plan, error, item, outer, region))?
                };
                frame.complete_rows(rows, local, values);
                Ok(())
            };
            if row_mode {
                evaluate(run, frame, &[], true)?;
                for &row in rows {
                    evaluate(run, frame, &[row], false)?;
                }
                Ok(())
            } else {
                evaluate(run, frame, rows, true)
            }
        })?;
    }
    Ok(())
}

/// A raw donor failure has donor provenance, rather than a destination identity.
fn failure<E>(
    plan: &crate::dataset::resolved::ResolvedPlan,
    mut error: Fail<E>,
    item: &Intermediate,
    outer: RegionId,
    region: RegionId,
) -> Fail<E> {
    match error.as_mut() {
        ExecutionError::Conversion { identity, .. }
        | ExecutionError::Numeric { identity, .. }
        | ExecutionError::MultipleValues { identity, .. }
        | ExecutionError::MultipleMatches { identity, .. }
        | ExecutionError::UnmatchedKey { identity, .. }
        | ExecutionError::Reduction { identity, .. }
        | ExecutionError::SemanticDiagnostic { identity, .. } => *identity = None,
        ExecutionError::SecondaryCell { source, .. } => {
            let name = &plan.region(region).secondary[*source].name;
            if let Some(index) = plan
                .region(outer)
                .secondary
                .iter()
                .position(|source| source.name == *name)
            {
                *source = index;
            } else if let ExecutionError::SecondaryCell {
                path,
                source_row,
                error: original,
                ..
            } = std::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
            {
                *error = ExecutionError::Cell {
                    path,
                    source_row,
                    error: original,
                };
            }
        }
        ExecutionError::Cell { .. } => {
            if let IntermediateSource::Input(source) = item.source {
                if let ExecutionError::Cell {
                    path,
                    source_row,
                    error: original,
                } = std::mem::replace(error.as_mut(), ExecutionError::SchemaMismatch)
                {
                    *error = ExecutionError::SecondaryCell {
                        source,
                        path,
                        source_row,
                        error: original,
                    };
                }
            }
        }
        _ => (),
    }
    error
}
