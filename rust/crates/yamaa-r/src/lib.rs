//! Optional R installation probe; all R interactions occur on the calling thread.
use extendr_api::prelude::*;
mod scalars;

/// Round-trip owned JSON text on the calling R thread without narrowing integers.
#[extendr]
fn scalar_round_trip(request: &str) -> List {
    // Return normally before the R facade raises a condition. extendr's default
    // Result conversion uses panic for Err, which is unnecessary for input errors.
    match yamaa_adapters::scalar_transport::scalar_round_trip(request) {
        Ok(value) => list!(value = value, error = NULL),
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

/// Return numeric outcome JSON normally before the R facade raises transport errors.
#[extendr]
fn evaluate_numeric(request: &str) -> List {
    match yamaa_adapters::numeric_transport::evaluate_numeric(request) {
        Ok(value) => list!(value = value, error = NULL),
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

/// Copy IPC bytes on the R thread, returning normally before facade errors.
#[extendr]
fn table_round_trip(request: Raw) -> List {
    match yamaa_adapters::table_transport::table_round_trip(request.as_slice()) {
        Ok(value) => list!(value = Raw::from_bytes(&value), error = NULL),
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

/// Inspect lossless table values as owned JSON without an R Arrow dependency.
#[extendr]
fn table_snapshot(request: Raw) -> List {
    match yamaa_adapters::table_transport::table_snapshot(request.as_slice()) {
        Ok(value) => list!(value = value, error = NULL),
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

#[extendr]
fn engine_info() -> List {
    let info = yamaa_engine::engine_info();
    list!(
        core_version = info.core_version,
        protocol_version = info.protocol_version,
        execution_supported = info.execution_supported,
        installation_resource = yamaa_adapters::installation_resource()
    )
}

extendr_module! {
    mod yamaanative;
    use scalars;
    fn engine_info;
    fn scalar_round_trip;
    fn evaluate_numeric;
    fn table_round_trip;
    fn table_snapshot;
}
