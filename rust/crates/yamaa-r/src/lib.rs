//! Optional R installation probe; all R interactions occur on the calling thread.
use extendr_api::prelude::*;

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
    fn engine_info;
    fn scalar_round_trip;
    fn evaluate_numeric;
}
