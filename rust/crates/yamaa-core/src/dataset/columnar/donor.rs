//! Donor materialization: a derived intermediate's records, each stored
//! field copied and every derivation evaluated in declaration order over all
//! donor records, on the same units as the dataset. A derivation that reads
//! the current record rebuilds the donor records for each current row
//! (LOOKUP-15); every donor handler counts each time they are built.
use super::frame::{Frame, Identity};
use super::relations::{self, Site};
use super::{Cx, Fail, Footprint, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{RegionId, RelationId};
use crate::dataset::{Intermediate, IntermediateSource};
use crate::limits::Limits;
use crate::table::{TableAccess, TableSchema};
use crate::value::Value;
use std::rc::Rc;

/// A materialized donor: its completed records.
pub(super) struct Donor {
    frame: Frame,
    schema: TableSchema,
}

impl Donor {
    pub(super) fn table<E>(&self) -> Table<'_, E> {
        Table::Rows {
            frame: &self.frame,
            schema: &self.schema,
            start: 0,
            rows: self.frame.rows(),
        }
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
) -> Result<Option<Rc<Donor>>, Fail<E>> {
    let plan = run.plan;
    let item = relations::item(plan, relation);
    let Some(derived) = &item.derivations else {
        return Ok(None);
    };
    let region = plan
        .relation(relation)
        .derivations
        .expect("a derived intermediate has a donor region");
    let key = (!derived.current.is_empty()).then(|| match row {
        Some(row) => cx.frame.identity(row),
        None => Identity::Free,
    });
    if let Some(data) = run.relations.materialized(relation, key) {
        return Ok(Some(data));
    }
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
    let table = match &item.source {
        IntermediateSource::Input(index) => cx.secondary[*index],
        IntermediateSource::Primary { .. } => cx.primary,
        IntermediateSource::SelfRows { .. } => Table::Rows {
            frame: cx.frame,
            schema: cx.schema,
            start: 0,
            rows: cx.self_rows,
        },
    };
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
    for source_row in 0..table.row_count() {
        let row = frame.push(0, Identity::Candidate(source_row), &[source_row]);
        for column in 0..derived.stored_columns {
            let value = relations::cell(table, item, source_row, column)?;
            frame.complete(row, column, value);
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
    let saved_relations =
        std::mem::replace(&mut run.relations, relations::State::new(plan, region));
    let saved_windows = std::mem::take(&mut run.windows);
    let result = derive(
        run,
        region,
        &mut frame,
        &current,
        &input_schema,
        &secondary,
        item,
        cx.region,
    );
    run.windows = saved_windows;
    run.relations = saved_relations;
    result?;
    let data = Rc::new(Donor {
        frame,
        schema: output,
    });
    run.relations
        .insert_materialized(relation, key, Rc::clone(&data));
    Ok(Some(data))
}

/// Evaluate every derivation over every donor record, in declaration order.
#[allow(clippy::too_many_arguments)]
fn derive<E>(
    run: &mut Run<'_, '_, E>,
    region: RegionId,
    frame: &mut Frame,
    current: &[Value],
    input_schema: &TableSchema,
    secondary: &[Table<'_, E>],
    item: &Intermediate,
    outer: RegionId,
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let rows: Vec<usize> = (0..frame.rows()).collect();
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
            rows: &rows,
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
                for &row in &rows {
                    evaluate(run, frame, &[row], false)?;
                }
                Ok(())
            } else {
                evaluate(run, frame, &rows, true)
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
