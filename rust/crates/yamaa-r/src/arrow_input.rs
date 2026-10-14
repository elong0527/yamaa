//! Export into Rust-owned stream storage; never dereference a supplied R pointer.
use extendr_api::prelude::*;
use yamaa_adapters::c_data_table::{self, FFI_ArrowArrayStream, ImportBudget};

#[extendr]
fn execute_specification_arrow(handle: Robj, export: Function, count: i32) -> List {
    crate::specification_service::boundary(|| {
        let run = crate::specification_service::resolve(&handle)?;
        let count = usize::try_from(count).map_err(|_| "invalid Arrow source count")?;
        c_data_table::admit_sources(&run, count).map_err(|error| error.to_string())?;
        let mut budget = ImportBudget::default();
        let mut tables = Vec::with_capacity(count);
        for index in 0..count {
            let mut stream = FFI_ArrowArrayStream::empty();
            // Pass an exact decimal address, never an R binary64 pointer value.
            // The exporter may write only during this synchronous callback.
            let destination = (&raw mut stream as usize).to_string();
            let reply = export
                .call(pairlist!(index as i32, destination))
                .map_err(|_| "Arrow stream export failed")?;
            if reply.as_bool() != Some(true) {
                return Err("Arrow stream export failed".into());
            }
            // SAFETY: the nanoarrow exporter fills our initialized, aligned stream
            // storage using the C Stream contract. R and the source stay protected
            // on this thread until execution and all foreign releases finish.
            tables.push(unsafe { budget.import(stream) }.map_err(|error| error.to_string())?);
        }
        let result = c_data_table::execute(&run, tables).map_err(|error| error.to_string())?;
        let table = result
            .table
            .map_or_else(|| r!(NULL), |bytes| Raw::from_bytes(&bytes).into_robj());
        Ok(list!(table = table, outcome = result.outcome).into_robj())
    })
}
extendr_module! {mod arrow_input; fn execute_specification_arrow;}
