//! Budget-aware typed-plan execution over copied canonical IPC snapshots.
//! All source framing and combined shapes are admitted before Arrow decoding.
use crate::ipc_table::{self, TableError};
use std::{convert::Infallible, panic::catch_unwind};
use yamaa_core::table::TableAccess;
use yamaa_engine::dataset::{DatasetExecution, DatasetPlan, ExecutionAttempt, ExecutionBudget};

/// Execute a prebound plan with caller-selected transport and evaluation quotas.
/// Semantic failures retain the engine's observations and never expose a dataset.
/// Accepted output can be exported with `ipc_table::dataset_batches` or
/// `ipc_table::write_dataset_with_budget` without building one large Arrow batch.
pub fn execute_dataset_with_budget(
    plan: &DatasetPlan,
    primary: &[u8],
    secondary: &[&[u8]],
    budget: ExecutionBudget,
) -> Result<ExecutionAttempt<Infallible>, TableError> {
    if secondary.len() != plan.secondary().len() {
        return Err(TableError::InvalidTable);
    }
    if secondary
        .len()
        .checked_add(1)
        .is_none_or(|count| count > budget.max_sources)
        || plan.output().columns().len() > budget.max_columns
    {
        return Err(TableError::ShapeLimit);
    }
    catch_unwind(|| {
        let inputs = || std::iter::once(primary).chain(secondary.iter().copied());
        let schemas = std::iter::once(plan.source())
            .chain(plan.secondary().iter().map(|source| &source.schema));
        let mut bytes_left = budget.max_ipc_bytes;
        let mut rows_left = budget.execution.source_rows;
        let mut cells_left = budget.max_decoded_cells;
        let mut batches_left = budget.max_batches;
        // Admission of every source precedes allocation by any Arrow decoder.
        let mut shapes = Vec::with_capacity(secondary.len() + 1);
        for (input, expected) in inputs().zip(schemas) {
            bytes_left = bytes_left
                .checked_sub(input.len())
                .ok_or(TableError::InputLimit)?;
            let shape = ipc_table::preflight(input, budget)?;
            if &shape.schema != expected {
                return Err(TableError::InvalidTable);
            }
            rows_left = rows_left
                .checked_sub(shape.rows)
                .ok_or(TableError::ShapeLimit)?;
            cells_left = cells_left
                .checked_sub(shape.cells)
                .ok_or(TableError::ShapeLimit)?;
            batches_left = batches_left
                .checked_sub(shape.batches)
                .ok_or(TableError::ShapeLimit)?;
            shapes.push(shape);
        }
        let mut text_left = budget.max_ipc_bytes;
        let mut tables = Vec::with_capacity(shapes.len());
        for (input, shape) in inputs().zip(shapes) {
            let (table, text) = ipc_table::decode_preflighted(input, shape, budget, text_left)?;
            text_left -= text;
            tables.push(table);
        }
        let secondary = tables[1..]
            .iter()
            .map(|table| table as &dyn TableAccess<Error = Infallible>)
            .collect::<Vec<_>>();
        Ok(plan.execute_with_budget(&tables[0], &secondary, budget))
    })
    .map_err(|_| TableError::Internal)?
}
