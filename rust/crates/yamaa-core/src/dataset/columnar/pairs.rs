//! The pair relation (LOOKUP-27): the matching pairs of a named
//! intermediate's two sides, materialized once a run, before any row is
//! built and before the intermediates' `unique` checks.
//!
//! Each side's records are an input's, or the records a row template
//! driving on the side's intermediate reads: those its filter keeps, its
//! derivations complete (ROW-2). Every left record, in stored order, pairs
//! with each right record whose key fields equal its own, in the right
//! side's stored order, under key equality (VALUE-19); a record missing a
//! key field pairs with none. The filter asks each joined record before any
//! derivation is evaluated, and the derivations complete the records it
//! keeps. Readers read the relation's exposed fields through its port.
use super::donor::{self, Built};
use super::frame::{Frame, Identity, NO_TEMPLATE};
use super::index::{self, Index};
use super::relations;
use super::{reserved, Cx, Fail, Run, Table};
use crate::dataset::execution::{ExecutionError, Resource};
use crate::dataset::resolved::{RegionId, RelationId};
use crate::dataset::{PairJoin, PairRecords, PairSide, PairValue};
use crate::limits::Limits;
use crate::result_type::ResultType;
use crate::table::{TableAccess, TableSchema, ValueRef};
use crate::value::ValueType;

/// The records of one side: the table they come from and those of its
/// records the side keeps, in stored order.
struct Side<'t, E> {
    table: SideTable<'t, E>,
    rows: Vec<usize>,
}

enum SideTable<'t, E> {
    /// A source port, or an intermediate's input when it derives nothing.
    Port(Table<'t, E>),
    /// An intermediate's donor records, its derivations complete.
    Built(Built),
}

impl<E> Side<'_, E> {
    fn table(&self) -> Table<'_, E> {
        match &self.table {
            SideTable::Port(table) => *table,
            SideTable::Built(built) => built.table(),
        }
    }
}

/// A side's records: every record of a port, or the records a row driver
/// reads from the side's intermediate (ROW-2).
fn side<'t, E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'t, E>,
    side: &PairSide,
) -> Result<Side<'t, E>, Fail<E>> {
    match side.records {
        PairRecords::Source(port) => {
            let table = match port {
                0 => cx.primary,
                port => *cx
                    .secondary
                    .get(port - 1)
                    .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
            };
            Ok(Side {
                rows: (0..table.row_count()).collect(),
                table: SideTable::Port(table),
            })
        }
        PairRecords::Intermediate(index) => {
            let plan = run.plan;
            let relation = plan
                .named(RegionId::DATASET, index)
                .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
            let item = relations::item(plan, relation);
            let built = donor::materialize(run, cx, relation, None, None)?;
            let table = match built {
                Some(built) => SideTable::Built(built),
                None => SideTable::Port(relations::source_table(item, cx)),
            };
            let mut side = Side {
                table,
                rows: Vec::new(),
            };
            side.rows = relations::eligibility(side.table(), item)?;
            Ok(side)
        }
    }
}

/// The pair relation `relation`'s records, built once a run: its joined
/// records that the filter keeps, each with its derivations complete.
pub(super) fn materialize<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
) -> Result<Built, Fail<E>> {
    if let Some(data) = run.relations.materialized(relation, None) {
        return Ok(data);
    }
    let plan = run.plan;
    let item = relations::item(plan, relation);
    let join = item
        .source
        .pairs()
        .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
    let left = side(run, cx, &join.left)?;
    let right = side(run, cx, &join.right)?;
    let values = values(run, cx, relation, join, &left)?;
    let joined = join_records(
        join,
        &item.identifier,
        &item.path,
        &left,
        values.as_ref(),
        &right,
    )?;
    let table = Table::Rows {
        frame: &joined,
        schema: &join.schema,
        start: 0,
        rows: joined.rows(),
    };
    let mut kept = Vec::new();
    reserved(kept.try_reserve_exact(joined.rows()))?;
    for row in 0..joined.rows() {
        if let Some(filter) = &join.filter {
            if !relations::keeps(table, filter, row)? {
                continue;
            }
        }
        kept.push(row);
    }
    let data = Built::whole(donor::build_pairs(run, cx, relation, table, &kept)?);
    run.relations
        .insert_materialized(relation, None, data.clone());
    Ok(data)
}

/// Each left record's key values (LOOKUP-6): the expressions of the key's
/// mapping evaluated over the left records, one row for each in the order
/// of `left`'s records; none when every key pair reads a field.
fn values<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    join: &PairJoin,
    left: &Side<'_, E>,
) -> Result<Option<Frame>, Fail<E>> {
    if join.values.is_none() {
        return Ok(None);
    }
    let plan = run.plan;
    let item = relations::item(plan, relation);
    let region = plan
        .relation(relation)
        .pair_values
        .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
    let input = plan
        .region_sources(region)
        .next()
        .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
    let input_schema = plan.sources()[input.index()].schema.clone();
    let table = left.table();
    let mut frame = Frame::new(plan.region(region).output.columns().len());
    reserved(frame.try_reserve(left.rows.len(), left.rows.len()))?;
    for (position, &source_row) in left.rows.iter().enumerate() {
        let row = frame.push(0, Identity::Candidate(source_row), &[position]);
        for (offset, &column) in join.left.columns.iter().enumerate() {
            frame.complete(row, offset, read(table, &item.path, source_row, column)?);
        }
    }
    let rows: Vec<usize> = (0..frame.rows()).collect();
    let saved_relations =
        std::mem::replace(&mut run.relations, relations::State::new(plan, region));
    let saved_windows = std::mem::take(&mut run.windows);
    let result = donor::derive(
        run,
        region,
        &mut frame,
        &rows,
        donor::DonorInput {
            current: &[],
            stride: 0,
            schema: &input_schema,
            secondary: &[],
        },
        item,
        cx.region,
    );
    run.windows = saved_windows;
    run.relations = saved_relations;
    result?;
    Ok(Some(frame))
}

/// Every matching pair of `left` and `right`, as joined records: the left
/// side's fields, then the right side's, in left stored order and then
/// right stored order.
fn join_records<E>(
    join: &PairJoin,
    name: &str,
    path: &str,
    left: &Side<'_, E>,
    values: Option<&Frame>,
    right: &Side<'_, E>,
) -> Result<Frame, Fail<E>> {
    let left_table = left.table();
    let right_table = right.table();
    let left_width = join.left_width();
    let width = join.schema.columns().len();
    let right_keys: Vec<usize> = join
        .keys
        .iter()
        .map(|key| join.right.columns[key.right - left_width])
        .collect();
    let kinds: Vec<Option<ValueType>> = right_keys
        .iter()
        .map(|&column| {
            right_table
                .schema()
                .columns()
                .get(column)
                .map(|field| field.kind.into())
        })
        .collect();
    // An index answers each probe whose values compare with their right
    // fields' types. A key the index cannot hold, such as a boolean, and a
    // value whose type is decided per value are scanned as a lookup's match
    // is, which fails where a value cannot compare (LOOKUP-6, LOOKUP-9).
    let index = Index::keys(
        &right_keys,
        &kinds,
        right.rows.iter().copied(),
        |row, column| read(right_table, path, row, column),
    )?;
    let limit = |rows: usize| {
        Limits::table(rows, width).map_err(|error| {
            Box::new(ExecutionError::ScopedLimit {
                resource: match error.resource {
                    "columns" => Resource::SourceColumns,
                    "table_cells" => Resource::SourceCells,
                    _ => Resource::SourceRows,
                },
                limit: error.limit,
                required: error.actual,
                spec_paths: vec![path.into()],
            })
        })
    };
    let mut joined = Frame::new(width);
    let mut probe = Vec::with_capacity(join.keys.len());
    for (position, &left_row) in left.rows.iter().enumerate() {
        probe.clear();
        for (key, &column) in join.keys.iter().zip(&right_keys) {
            let value = match key.left {
                PairValue::Field(field) => {
                    read(left_table, path, left_row, join.left.columns[field])?
                }
                PairValue::Value(value) => values
                    .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?
                    .get(position, value),
            };
            probe.push((column, value));
        }
        let candidates = relations::Probe {
            keys: &probe,
            kinds: &kinds,
            range: None,
        };
        if candidates.incomplete() {
            continue;
        }
        let scanned;
        let matches = match &index {
            Some(index) if index::exact(&probe, &kinds) => index.matching_pairs(&probe),
            _ => {
                scanned = relations::candidates(
                    right.rows.iter().copied(),
                    &candidates,
                    |row, column| read(right_table, path, row, column),
                    |_, _: &mut ()| Ok(true),
                    &mut (),
                )
                .map_err(|refusal| key_refusal(refusal, join, name))?
                .rows;
                scanned.as_slice()
            }
        };
        if matches.is_empty() {
            continue;
        }
        let rows = joined
            .rows()
            .checked_add(matches.len())
            .ok_or_else(|| Box::new(ExecutionError::Capacity))?;
        limit(rows)?;
        reserved(joined.try_reserve(matches.len(), matches.len()))?;
        for &right_row in matches {
            let position = joined.rows();
            let row = joined.push(NO_TEMPLATE, Identity::Candidate(position), &[position]);
            for (offset, &column) in join.left.columns.iter().enumerate() {
                joined.complete(row, offset, read(left_table, path, left_row, column)?);
            }
            for (offset, &column) in join.right.columns.iter().enumerate() {
                joined.complete(
                    row,
                    left_width + offset,
                    read(right_table, path, right_row, column)?,
                );
            }
        }
    }
    Ok(joined)
}

/// The finding of a scan that met a key value its right field cannot compare
/// with: the pair relation's key fails at the pair, as a lookup's does
/// (LOOKUP-6, LOOKUP-27).
fn key_refusal<E>(refusal: relations::Refusal<E>, join: &PairJoin, name: &str) -> Fail<E> {
    match refusal {
        relations::Refusal::Failed(error) => error,
        relations::Refusal::Key {
            index,
            current,
            donor,
        } => match join.keys.get(index) {
            Some(key) => Box::new(ExecutionError::ScalarDiagnostic(
                crate::diagnostic::lookup::pair_key_type(
                    &key.path,
                    name,
                    &key.source,
                    ResultType::known(current),
                    ResultType::known(donor),
                ),
            )),
            None => Box::new(ExecutionError::SchemaMismatch),
        },
        relations::Refusal::Range(_) => Box::new(ExecutionError::SchemaMismatch),
    }
}

/// The records readers read through a pair relation's port: each of its
/// records' exposed fields, in the order the relation exposes them.
pub(super) fn port_records<E>(
    built: &Built,
    columns: &[usize],
    fields: &TableSchema,
) -> Result<Frame, Fail<E>> {
    let table: Table<'_, E> = built.table();
    let mut frame = Frame::new(fields.columns().len());
    reserved(frame.try_reserve(table.row_count(), table.row_count()))?;
    for record in 0..table.row_count() {
        let row = frame.push(NO_TEMPLATE, Identity::Candidate(record), &[record]);
        for (offset, &column) in columns.iter().enumerate() {
            let value = table
                .get(record, column)
                .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
            frame.complete(row, offset, value);
        }
    }
    Ok(frame)
}

/// One cell of a side's records, a failed read named at the pair relation.
fn read<'t, E>(
    table: Table<'t, E>,
    path: &str,
    row: usize,
    column: usize,
) -> Result<ValueRef<'t>, Fail<E>> {
    table.get(row, column).map_err(|error| {
        Box::new(ExecutionError::Cell {
            path: path.into(),
            source_row: row,
            error,
        })
    })
}
