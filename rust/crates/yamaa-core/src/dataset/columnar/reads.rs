//! A named intermediate's column read over a selection (LOOKUP-25): each
//! row's record choice, then the column's value from the chosen record.
//!
//! A choice is resolved once per row identity and cached for every read of
//! the relation, a candidate's by its number, so a read over any rows, such
//! as a `case` branch's or another column's, finds the choices it holds
//! without hashing. When every row chooses among the same records, the
//! donor is prepared once, the rows still to resolve match each distinct
//! combination of their match values once, and the column is gathered from
//! the records' storage. Rows are visited in order and each is answered as a
//! one-row read answers it, so the first failure, and the handler counts
//! before it, are those of one-row reads.
use super::buffer::Values;
use super::donor::{self, Built, Donor};
use super::index::Combinations;
use super::relations::{
    check_deferred_filter_types, choose, chosen, current_value, enclosed, item, prepare_matched,
    reference_value, resolve_choice, resolve_sought, source_name, source_table, Choice, Reading,
    Site, Sought,
};
use super::{Cx, Fail, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, Read, RelationId};
use crate::dataset::{Intermediate, MatchRead, MatchValue};
use crate::numeric_lifecycle::HandlerKind;
use crate::table::ValueRef;

/// An intermediate read node over rows, as one column.
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
    let site = Site::node(plan, expr);
    prepare_matched(run, cx, site, relation, rows);
    let enclosing;
    let cx = match enclosed(cx) {
        Some(outer) => {
            enclosing = outer;
            &enclosing
        }
        None => cx,
    };
    let item = item(plan, relation);
    let mut gather = Gather::new(item, column as usize, rows.len(), site, observe);
    let outcome = if rows.len() > 1 && shared_records(item) {
        shared(run, cx, site, relation, rows, &mut gather)
    } else {
        rows.iter().try_for_each(|&row| {
            let (prepared, choice) = choose(run, cx, site, relation, row)?;
            let table = prepared
                .as_ref()
                .map_or_else(|| source_table(item, cx), Built::table);
            gather.push(table, choice)
        })
    };
    let fired = gather.fire(run);
    outcome?;
    fired?;
    gather.finish()
}

/// Whether every row of a relation chooses among the same records: those of
/// its source, of its pairs, or of a donor built once. Derivations that read
/// the current record build them per row instead.
fn shared_records(item: &Intermediate) -> bool {
    item.source.pairs().is_some()
        || item
            .derivations
            .as_ref()
            .is_none_or(|derived| derived.current.is_empty())
}

/// The rows of a read whose rows all choose among the same records: the
/// donor prepared once, as the first row's read prepares it, then each row
/// in order, its choice cached or resolved now.
fn shared<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    site: Site<'_>,
    relation: RelationId,
    rows: &[usize],
    gather: &mut Gather<'_, '_>,
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let item = gather.item;
    let prepared = donor::materialize(run, cx, relation, Some(site), Some(rows[0]))?;
    let table = prepared
        .as_ref()
        .map_or_else(|| source_table(item, cx), Built::table);
    let lent = storage(table, gather.column, rows.len());
    gather.lending = lent.is_some();
    let mut matcher = Matcher::new(item, cx, rows.len());
    let donor = prepared.as_ref().map(Built::donor);
    let name = source_name(item, &plan.region(cx.region).secondary);
    for &row in rows {
        let identity = cx.frame.identity(row);
        let choice = match run.relations.choice(relation, identity) {
            Some(choice) => choice,
            None => {
                let reading = Reading {
                    site,
                    relation,
                    item,
                    name,
                    row,
                    identity,
                };
                matcher.choose(run, cx, &reading, table, donor)?
            }
        };
        gather.push(table, choice)?;
    }
    gather.gathered(lent)
}

/// How the rows still to resolve read their match values, and the choice of
/// each distinct combination of them.
struct Matcher<'k> {
    /// The key pairs in the order the match values resolve (outputs, then
    /// record fields), each a donor column and where its value is lent
    /// from; none when a key is an expression or another input's field,
    /// whose rows resolve alone.
    keys: Option<Vec<(usize, Lent<'k>)>>,
    probe: Vec<(usize, ValueRef<'k>)>,
    /// The choice of each combination of match values met, when only they
    /// decide the choice: no range, nearest selection or filter reading the
    /// current row.
    combinations: Option<Combinations<'k, Choice>>,
}

/// Where a match value is lent from: a completed output column, or a field
/// of the row's one driver record, from its storage when the driver holds
/// the field whole.
#[derive(Clone, Copy)]
enum Lent<'k> {
    Output(usize),
    Record(usize, Option<crate::table::ColumnRef<'k>>),
}

impl<'k> Matcher<'k> {
    fn new<E>(item: &Intermediate, cx: &Cx<'k, E>, rows: usize) -> Self {
        let mut keys = Some(Vec::with_capacity(item.keys.len()));
        for record in [false, true] {
            for key in &item.keys {
                let lent = match &key.value {
                    MatchValue::Variable(MatchRead::Output { column, .. }) => Lent::Output(*column),
                    MatchValue::Variable(MatchRead::Record { column, .. }) => {
                        Lent::Record(*column, cx.table.lent(*column, rows))
                    }
                    _ => {
                        keys = None;
                        continue;
                    }
                };
                if matches!(lent, Lent::Record(..)) == record {
                    if let Some(keys) = keys.as_mut() {
                        keys.push((key.source_column, lent));
                    }
                }
            }
        }
        let filter = item
            .filter
            .as_ref()
            .is_some_and(|filter| filter.correlated());
        let decides = item.range.is_none() && item.nearest.is_none() && !filter;
        Self {
            combinations: (keys.is_some() && decides).then(|| Combinations::new(item.keys.len())),
            keys,
            probe: Vec::with_capacity(item.keys.len()),
        }
    }

    /// One row's match values, lent as a one-row read resolves them; false
    /// when its keys resolve otherwise.
    fn lend<E>(&mut self, cx: &Cx<'k, E>, path: &str, row: usize) -> Result<bool, Fail<E>> {
        let Some(keys) = &self.keys else {
            return Ok(false);
        };
        self.probe.clear();
        for &(column, lent) in keys {
            let value = match lent {
                Lent::Output(output) => cx.frame.column(output).get(row),
                Lent::Record(field, storage) => {
                    let [source_row] = *cx.frame.members(row) else {
                        return Ok(false);
                    };
                    match storage {
                        Some(storage) if source_row < storage.len() => storage.get(source_row),
                        _ => cx.table.get(source_row, field).map_err(|error| {
                            Box::new(ExecutionError::Cell {
                                path: path.into(),
                                source_row,
                                error,
                            })
                        })?,
                    }
                }
            };
            self.probe.push((column, value));
        }
        Ok(true)
    }

    /// The choice of a row not yet chosen, resolved as `resolve_choice`
    /// resolves it, or taken from an earlier row with the same match values
    /// when only they decide, and cached under the row's identity.
    fn choose<E>(
        &mut self,
        run: &mut Run<'_, '_, E>,
        cx: &Cx<'k, E>,
        reading: &Reading<'_>,
        table: Table<'_, E>,
        donor: Option<&Donor>,
    ) -> Result<Choice, Fail<E>> {
        let Reading {
            site,
            relation,
            item,
            row,
            identity,
            ..
        } = *reading;
        if self.keys.is_some() {
            check_deferred_filter_types(item)?;
        }
        if !self.lend(cx, &item.path, row)? {
            return resolve_choice(run, cx, *reading, table, None, donor);
        }
        let held = self
            .combinations
            .as_mut()
            .and_then(|combinations| combinations.find(self.probe.iter().map(|&(_, value)| value)));
        let choice = match held {
            Some(choice) => choice,
            None => {
                let range = match &item.range {
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
                let reference = reference_value(run, cx, relation, item, row)?;
                let sought = Sought {
                    keys: &self.probe,
                    range: range.as_ref().map(ValueRef::from),
                    reference: reference.as_ref(),
                };
                let choice = resolve_sought(run, cx, reading, table, donor, &sought)?;
                if let Some(combinations) = &mut self.combinations {
                    combinations.remember(choice)?;
                }
                choice
            }
        };
        run.relations.insert_choice(relation, identity, choice)?;
        Ok(choice)
    }
}

/// One read column under construction: each row's value read from its
/// chosen record, or the absent value, and each row's handler counted under
/// the read's site, in row order. Rows that all read one table lend it, and
/// their values are gathered from its storage once every row is chosen.
struct Gather<'i, 's> {
    item: &'i Intermediate,
    column: usize,
    rows: usize,
    values: Values,
    reserved: bool,
    /// Whether the rows' table lends its storage of the column, and each
    /// row's chosen record, or `usize::MAX` for the absent value.
    lending: bool,
    sources: Vec<usize>,
    site: Site<'s>,
    observe: bool,
    /// Firings counted so far, of each kind, in the order each first fired.
    fired: Vec<(HandlerKind, u64)>,
}

/// A column's values as the records hold them.
enum Storage<'t> {
    Column(crate::table::ColumnRef<'t>),
    Frame { values: &'t Values, start: usize },
}

impl<'i, 's> Gather<'i, 's> {
    fn new(
        item: &'i Intermediate,
        column: usize,
        rows: usize,
        site: Site<'s>,
        observe: bool,
    ) -> Self {
        Self {
            item,
            column,
            rows,
            values: Values::missing(0),
            reserved: false,
            lending: false,
            sources: Vec::new(),
            site,
            observe,
            fired: Vec::new(),
        }
    }

    /// The next row's value, read from `table` by its choice.
    fn push<E>(&mut self, table: Table<'_, E>, choice: Choice) -> Result<(), Fail<E>> {
        if self.lending {
            if self.sources.is_empty() {
                super::reserved(self.sources.try_reserve_exact(self.rows))?;
            }
            self.sources.push(match choice {
                Choice::Record { row, .. } => row,
                Choice::Absent => usize::MAX,
            });
        } else {
            let (value, _) = chosen(table, self.item, self.column, choice)?;
            self.values.push(value);
            if !self.reserved && !matches!(self.values, Values::Missing(_)) {
                let rest = self.rows.saturating_sub(self.values.len());
                super::reserved(self.values.try_reserve(rest))?;
                self.reserved = true;
            }
        }
        // A selected cell that refuses access has not answered the read:
        // as on the row path, only a successful read counts its handler.
        if self.observe {
            let kind = match choice {
                Choice::Absent => Some(HandlerKind::Absent),
                Choice::Record { multiple: true, .. } => Some(HandlerKind::MultipleMatches),
                Choice::Record { .. } => None,
            };
            if let Some(kind) = kind {
                match self.fired.iter_mut().find(|(fired, _)| *fired == kind) {
                    Some((_, count)) => *count += 1,
                    None => self.fired.push((kind, 1)),
                }
            }
        }
        Ok(())
    }

    /// Count the firings gathered so far at the read's sites.
    fn fire<E>(&mut self, run: &mut Run<'_, '_, E>) -> Result<(), Fail<E>> {
        for (kind, count) in std::mem::take(&mut self.fired) {
            let path = format!("{}.{}", self.site.path, kind.name());
            run.fire_many(self.site.expr, &path, kind, count)?;
        }
        Ok(())
    }

    /// The column.
    fn finish<E>(self) -> Result<Values, Fail<E>> {
        Ok(self.values)
    }

    /// The column gathered from `table`'s storage of it once every row is
    /// chosen, when the table lends it, each value the one a one-row read
    /// pushes.
    fn gathered<E>(&mut self, lent: Option<Storage<'_>>) -> Result<(), Fail<E>> {
        if !std::mem::take(&mut self.lending) {
            return Ok(());
        }
        let Some(lent) = lent else {
            return Ok(());
        };
        let absent = self
            .item
            .no_match
            .as_ref()
            .map_or(ValueRef::Missing, ValueRef::from);
        let every = !self.sources.contains(&usize::MAX);
        match lent {
            Storage::Column(column) => {
                if every {
                    if let Some(values) = Values::from_column(column, &self.sources) {
                        self.values = super::reserved(values)?;
                        return Ok(());
                    }
                }
                self.values = pushed(&self.sources, absent, |row| column.get(row));
            }
            Storage::Frame { values, start } => {
                self.values = if every {
                    let rows: Vec<usize> = self.sources.iter().map(|&row| start + row).collect();
                    super::reserved(values.gather(&rows))?
                } else {
                    pushed(&self.sources, absent, |row| values.get(start + row))
                };
            }
        }
        Ok(())
    }
}

/// A table's storage of one column for a read of `rows` of its records:
/// a source port's when lending it repays (`Table::lent`), a frame's always.
fn storage<E>(table: Table<'_, E>, column: usize, rows: usize) -> Option<Storage<'_>> {
    match table {
        Table::Source(_) => table.lent(column, rows).map(Storage::Column),
        Table::Rows { frame, start, .. } if column < frame.width() => Some(Storage::Frame {
            values: frame.column(column),
            start,
        }),
        Table::Rows { .. } | Table::Input { .. } => None,
    }
}

/// The values of `sources` pushed in order, `usize::MAX` the absent value.
fn pushed<'v>(
    sources: &[usize],
    absent: ValueRef<'_>,
    value: impl Fn(usize) -> ValueRef<'v>,
) -> Values {
    let mut values = Values::missing(0);
    for &source in sources {
        values.push(if source == usize::MAX {
            absent
        } else {
            value(source)
        });
    }
    values
}

#[cfg(test)]
mod tests {
    // The seam is a named read's handler accounting when the selected donor
    // cell refuses access, which authored data fixtures cannot make a port
    // do. LOOKUP-24 counts a successful multiple-match read; the failed cell
    // contributes no firing, while earlier successful reads keep theirs.
    use super::*;
    use crate::dataset::IntermediateSource;
    use crate::table::{CellError, Column, TableAccess, TableSchema};
    use crate::value::ColumnType;

    struct RefusingCell(TableSchema);
    impl TableAccess for RefusingCell {
        type Error = &'static str;
        fn schema(&self) -> &TableSchema {
            &self.0
        }
        fn row_count(&self) -> usize {
            2
        }
        fn cell(&self, row: usize, _column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
            match row {
                0 => Ok(ValueRef::Int(7)),
                _ => Err(CellError::Access("refused selected cell")),
            }
        }
    }

    #[test]
    fn a_failed_selected_cell_counts_no_multiple_matches_firing() {
        let table = RefusingCell(
            TableSchema::new(vec![Column {
                name: "VALUE".into(),
                kind: ColumnType::Int,
            }])
            .unwrap(),
        );
        let item = Intermediate {
            identifier: "L".into(),
            path: "intermediates[0]".into(),
            source: IntermediateSource::Input(0),
            keys: Vec::new(),
            filter: None,
            selection: None,
            nearest: None,
            range: None,
            columns: None,
            no_match: None,
            require_match: false,
            unique: Vec::new(),
            derivations: None,
            pending_schema: None,
        };
        let site = Site {
            expr: None,
            path: "columns[OUT]",
        };
        let mut gather = Gather::new(&item, 0, 2, site, true);
        gather
            .push(
                Table::Source(&table),
                Choice::Record {
                    row: 0,
                    multiple: true,
                },
            )
            .unwrap();
        match *gather
            .push(
                Table::Source(&table),
                Choice::Record {
                    row: 1,
                    multiple: true,
                },
            )
            .unwrap_err()
        {
            ExecutionError::SecondaryCell {
                source: 0,
                source_row: 1,
                error: CellError::Access("refused selected cell"),
                ..
            } => {}
            other => panic!("selected cell failure was not retained: {other:?}"),
        }
        assert_eq!(
            gather.fired,
            vec![(HandlerKind::MultipleMatches, 1)],
            "only the successful read fires"
        );
        assert_eq!(
            (gather.values.len(), gather.values.get(0)),
            (1, ValueRef::Int(7)),
            "the successful read stays"
        );
    }
}
