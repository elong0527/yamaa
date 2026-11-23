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
//!
//! A counted relation (LOOKUP-29) has no right side to match: every left
//! record, in stored order, pairs with each whole number from 1 to the
//! count it gives, in ascending order, the number standing where a right
//! side's fields stand. Each count is read, and the rows it adds are
//! admitted, before any record of its left record is built.
use super::donor::{self, Built};
use super::frame::{Frame, Identity, NO_TEMPLATE};
use super::index::{self, Index};
use super::relations;
use super::{reserved, Cx, Fail, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{RegionId, RelationId};
use crate::dataset::{PairCount, PairJoin, PairRecords, PairSide, PairValue};
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
/// reads from the side's intermediate (ROW-2). None for a counted
/// relation's generated side, which stores no record (LOOKUP-29).
fn side<'t, E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'t, E>,
    side: &PairSide,
) -> Result<Option<Side<'t, E>>, Fail<E>> {
    match &side.records {
        PairRecords::Counted(_) => Ok(None),
        &PairRecords::Source(port) => {
            let table = match port {
                0 => cx.primary,
                port => *cx
                    .secondary
                    .get(port - 1)
                    .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?,
            };
            Ok(Some(Side {
                rows: (0..table.row_count()).collect(),
                table: SideTable::Port(table),
            }))
        }
        &PairRecords::Intermediate(index) => {
            let relation = run
                .plan
                .named(RegionId::DATASET, index)
                .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
            driver_records(run, cx, relation).map(Some)
        }
    }
}

/// The records a row template driving on the intermediate `relation` reads
/// (ROW-2): those its filter keeps, its derivations complete, in stored
/// order.
fn driver_records<'t, E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'t, E>,
    relation: RelationId,
) -> Result<Side<'t, E>, Fail<E>> {
    let item = relations::item(run.plan, relation);
    let built = donor::materialize(run, cx, relation, None, None)?;
    let table = match built {
        Some(built) => SideTable::Built(built),
        None => SideTable::Port(relations::source_table(item, cx)),
    };
    let mut side = Side {
        table,
        rows: Vec::new(),
    };
    side.rows = relations::eligibility(run.dates, side.table(), item)?;
    Ok(side)
}

/// The records of the record relation `relation`'s port (LOOKUP-3): the
/// records a row template driving on it reads, each with the fields the
/// port exposes, `columns` of its layout, in order.
/// A failed cell read names the relation and the source row.
pub(super) fn record_port<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    relation: RelationId,
    columns: &[usize],
) -> Result<Frame, Fail<E>> {
    let side = driver_records(run, cx, relation)?;
    let path = &relations::item(run.plan, relation).path;
    copy_records(side.table(), &side.rows, columns, columns.len(), Some(path))
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
    let mismatch = || Box::new(ExecutionError::SchemaMismatch);
    let left = side(run, cx, &join.left)?.ok_or_else(mismatch)?;
    let right = side(run, cx, &join.right)?;
    let values = values(run, cx, relation, join, &left)?;
    let pairing = Pairing {
        join,
        name: &item.identifier,
        path: &item.path,
        left: &left.rows,
        table: left.table(),
        values: values.as_ref(),
    };
    let joined = match (&join.right.records, &right) {
        (PairRecords::Counted(count), None) => counted_records(&pairing, count)?,
        (_, Some(right)) => join_records(&pairing, right)?,
        _ => return Err(mismatch()),
    };
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
            if !relations::keeps(run.dates, table, filter, row)? {
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
/// of `left`'s records; none when every key pair reads a field. A counted
/// relation's count, when an expression gives it, is evaluated the same way
/// (LOOKUP-29).
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

/// What pairing a relation's left records reads: its join, its name and
/// path, the left side's records, each the row of `table` that stores it,
/// and the values its left records give.
struct Pairing<'p, E> {
    join: &'p PairJoin,
    name: &'p str,
    path: &'p str,
    left: &'p [usize],
    table: Table<'p, E>,
    values: Option<&'p Frame>,
}

impl<'p, E> Pairing<'p, E> {
    /// The value the left record at `position` among the left side's
    /// records, stored at `row`, gives for `value`.
    fn value(
        &self,
        value: PairValue,
        position: usize,
        row: usize,
    ) -> Result<ValueRef<'p>, Fail<E>> {
        match value {
            PairValue::Field(field) => {
                read(self.table, self.path, row, self.join.left.columns[field])
            }
            PairValue::Value(value) => Ok(self
                .values
                .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?
                .get(position, value)),
        }
    }
    /// Append one joined record to `joined`: the fields of the left record
    /// stored at `row`, the right side's left for the caller to complete.
    fn push(&self, joined: &mut Frame, row: usize) -> Result<usize, Fail<E>> {
        let position = joined.rows();
        let record = joined.push(NO_TEMPLATE, Identity::Candidate(position), &[position]);
        for (offset, &column) in self.join.left.columns.iter().enumerate() {
            joined.complete(record, offset, read(self.table, self.path, row, column)?);
        }
        Ok(record)
    }
}

/// A counted relation's records (LOOKUP-29), as joined records: each left
/// record, in stored order, once for each whole number from 1 to its count,
/// in ascending order, the left side's fields and then the number. A count
/// of zero builds no record, and a missing or negative count is no count
/// and fails the run. Each count is read, and the rows it adds are
/// admitted, before any record of its left record is reserved or built.
fn counted_records<E>(pairing: &Pairing<'_, E>, count: &PairCount) -> Result<Frame, Fail<E>> {
    let join = pairing.join;
    let left_width = join.left_width();
    let mut joined = Frame::new(join.schema.columns().len());
    for (position, &left_row) in pairing.left.iter().enumerate() {
        let refused = |read: Option<i64>| -> Fail<E> {
            let finding = crate::diagnostic::DatasetFinding::RepeatCount {
                path: &count.path,
                dataset: &join.left.name,
                intermediate: pairing.name,
                record: position + 1,
                count: read,
            };
            Box::new(match finding.diagnostic() {
                Ok(diagnostic) => ExecutionError::ScalarDiagnostic(diagnostic),
                Err(_) => ExecutionError::SchemaMismatch,
            })
        };
        // Binding admits a count of type `int` alone.
        let records = match pairing.value(count.value, position, left_row)? {
            ValueRef::Int(records) if records < 0 => return Err(refused(Some(records))),
            ValueRef::Int(records) => records,
            ValueRef::Missing => return Err(refused(None)),
            _ => return Err(Box::new(ExecutionError::SchemaMismatch)),
        };
        // A count no row total holds is refused as memory refuses it.
        let added = usize::try_from(records).unwrap_or(usize::MAX);
        reserved(joined.try_reserve(added, added))?;
        for number in 1..=records {
            let record = pairing.push(&mut joined, left_row)?;
            joined.complete(record, left_width, ValueRef::Int(number));
        }
    }
    Ok(joined)
}

/// Every matching pair of `left` and `right`, as joined records: the left
/// side's fields, then the right side's, in left stored order and then
/// right stored order.
fn join_records<E>(pairing: &Pairing<'_, E>, right: &Side<'_, E>) -> Result<Frame, Fail<E>> {
    let &Pairing {
        join,
        name,
        path,
        left,
        ..
    } = pairing;
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
    let mut joined = Frame::new(width);
    let mut probe = Vec::with_capacity(join.keys.len());
    for (position, &left_row) in left.iter().enumerate() {
        probe.clear();
        for (key, &column) in join.keys.iter().zip(&right_keys) {
            probe.push((column, pairing.value(key.left, position, left_row)?));
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
        reserved(joined.try_reserve(matches.len(), matches.len()))?;
        for &right_row in matches {
            let row = pairing.push(&mut joined, left_row)?;
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
    let records = (0..table.row_count()).collect::<Vec<_>>();
    copy_records(table, &records, columns, fields.columns().len(), None)
}

/// A port's records, `width` fields wide: each of `records` of `table`, in
/// order, with its `columns`, in order. A failed read names `path` and the
/// source row when the records are read from a relation's source; a pair
/// relation's completed frame has no source row to name.
fn copy_records<E>(
    table: Table<'_, E>,
    records: &[usize],
    columns: &[usize],
    width: usize,
    path: Option<&str>,
) -> Result<Frame, Fail<E>> {
    let mut frame = Frame::new(width);
    reserved(frame.try_reserve(records.len(), records.len()))?;
    for &record in records {
        let row = frame.push(NO_TEMPLATE, Identity::Candidate(record), &[record]);
        for (offset, &column) in columns.iter().enumerate() {
            let value = match path {
                Some(path) => read(table, path, record, column)?,
                None => table
                    .get(record, column)
                    .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?,
            };
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
