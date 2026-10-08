//! Optional R installation probe; all R interactions occur on the calling thread.
use extendr_api::prelude::*;
mod domain_entry;
mod file_publication;
mod file_resources;
mod file_specification;
mod function_callback;
mod inheritance_callback;
mod issue_frame;
mod public_table;
mod scalars;
mod specification_inheritance;
mod specification_result;
mod specification_service;

/// Validate raw request bytes before any Rust string is constructed. R character
/// encoding marks are not proof that the held bytes satisfy UTF-8 invariants.
fn json_request(
    request: Raw,
    limit_error: String,
    run: impl FnOnce(&str) -> std::result::Result<String, String>,
) -> List {
    bounded_json_request(
        request,
        yamaa_adapters::scalar_transport::MAX_REQUEST_BYTES,
        limit_error,
        run,
    )
}

fn bounded_json_request(
    request: Raw,
    limit: usize,
    limit_error: String,
    run: impl FnOnce(&str) -> std::result::Result<String, String>,
) -> List {
    // Both installed JSON transports share this byte limit. Enforce it before
    // scanning UTF-8, and preserve each transport's existing limit diagnostic.
    if request.len() > limit {
        return list!(value = NULL, error = limit_error);
    }
    match std::str::from_utf8(request.as_slice()) {
        Ok(text) => match run(text) {
            Ok(value) => list!(value = value, error = NULL),
            Err(error) => list!(value = NULL, error = error),
        },
        Err(_) => list!(value = NULL, error = "invalid UTF-8 JSON request"),
    }
}

/// Round-trip owned JSON text on the calling R thread without narrowing integers.
#[extendr]
fn scalar_round_trip(request: Raw) -> List {
    // Return normally before the R facade raises a condition. extendr's default
    // Result conversion uses panic for Err, which is unnecessary for input errors.
    json_request(
        request,
        yamaa_adapters::scalar_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::scalar_transport::scalar_round_trip(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Compile and analyze a bounded metadata batch without retaining R objects.
#[extendr]
fn analyze_references(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::reference_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::reference_transport::analyze_references(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Return shared graph analysis before the R facade raises any transport condition.
#[extendr]
fn analyze_dependencies(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::dependency_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::dependency_transport::analyze_dependencies(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Return shared column dependency analysis before the R facade raises any transport condition.
#[extendr]
fn analyze_column_dependencies(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::column_dependency_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::column_dependency_transport::analyze_column_dependencies(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Parse numeric syntax and return normally before the R facade raises errors.
#[extendr]
fn analyze_numeric(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::numeric_syntax::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::numeric_syntax::analyze_numeric(text).map_err(|error| error.to_string())
        },
    )
}

/// Parse predicate syntax and return normally before the R facade raises errors.
#[extendr]
fn analyze_predicate(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::predicate_syntax::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::predicate_syntax::analyze_predicate(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Interpret decoded schema snapshots without YAML, filesystem or host callback authority.
#[extendr]
fn interpret_schema(request: Raw) -> List {
    bounded_json_request(
        request,
        yamaa_adapters::schema_transport::MAX_REQUEST_BYTES,
        yamaa_adapters::schema_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::schema_transport::interpret_schema(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Decode raw source directly; return normally before the R facade raises any
/// transport error. No R character/numeric conversion participates in decoding.
#[extendr]
fn decode_yaml(source: Raw) -> List {
    match yamaa_adapters::yaml_transport::decode_yaml_bytes(source.as_slice()) {
        Ok(value) => list!(value = value, error = NULL),
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

/// Compile or match regex before returning normally to the calling R facade.
#[extendr]
fn evaluate_regex(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::regex_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::regex_transport::evaluate_regex(text).map_err(|error| error.to_string())
        },
    )
}

/// Parse aggregate syntax and return normally before the R facade raises errors.
#[extendr]
fn analyze_aggregate(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::aggregate_transport::TransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::aggregate_transport::analyze_aggregate(text)
                .map_err(|error| error.to_string())
        },
    )
}

/// Return numeric outcome JSON normally before the R facade raises transport errors.
#[extendr]
fn evaluate_numeric(request: Raw) -> List {
    json_request(
        request,
        yamaa_adapters::numeric_transport::NumericTransportError::RequestLimit.to_string(),
        |text| {
            yamaa_adapters::numeric_transport::evaluate_numeric(text)
                .map_err(|error| error.to_string())
        },
    )
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

/// Validate raw JSON before constructing a string; run the shared typed dataset service.
#[extendr]
fn execute_dataset(request: Raw, source: Raw) -> List {
    execute_dataset_sources(request, source, list!())
}

/// Keep secondary raw vectors rooted while the shared bridge copies their snapshots.
#[extendr]
fn execute_dataset_sources(request: Raw, source: Raw, secondary: List) -> List {
    if secondary.len() >= yamaa_adapters::dataset_transport::MAX_SOURCES {
        return list!(value = NULL, error = "too many secondary dataset sources");
    }
    let buffers: Option<Vec<Raw>> = secondary.iter().map(|(_, value)| value.as_raw()).collect();
    let Some(buffers) = buffers else {
        return list!(
            value = NULL,
            error = "secondary sources must be raw Arrow IPC vectors"
        );
    };
    let slices: Vec<&[u8]> = buffers.iter().map(|bytes| bytes.as_slice()).collect();
    if request.len() > yamaa_adapters::scalar_transport::MAX_REQUEST_BYTES {
        return list!(value = NULL, error = "dataset plan exceeds resource limit");
    }
    let text = match std::str::from_utf8(request.as_slice()) {
        Ok(text) => text,
        Err(_) => return list!(value = NULL, error = "invalid UTF-8 JSON request"),
    };
    match yamaa_adapters::dataset_transport::execute_dataset_sources(
        text,
        source.as_slice(),
        &slices,
    ) {
        Ok(result) => {
            let table = result
                .table
                .map_or_else(|| r!(NULL), |bytes| Raw::from_bytes(&bytes).into_robj());
            list!(
                value = list!(table = table, outcome = result.outcome),
                error = NULL
            )
        }
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

/// Discover shared typed dataset features without reading any source bytes.
#[extendr]
fn dataset_capabilities() -> &'static str {
    yamaa_adapters::dataset_transport::capabilities()
}

/// Report the shared metadata-query contract without preparing a catalog.
#[extendr]
fn reference_capabilities() -> &'static str {
    yamaa_adapters::reference_transport::capabilities()
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
    mod yamaa;
    use domain_entry;
    use scalars;
    use specification_service;
    use specification_result;
    use specification_inheritance;
    use function_callback;
    use file_publication;
use file_resources;
use file_specification;
    use inheritance_callback;
    fn engine_info;
    fn analyze_dependencies;
    fn analyze_references;
    fn reference_capabilities;
    fn analyze_column_dependencies;
    fn scalar_round_trip;
    fn evaluate_numeric;
    fn analyze_aggregate;
    fn analyze_numeric;
    fn analyze_predicate;
    fn interpret_schema;
    fn decode_yaml;
    fn evaluate_regex;
    fn table_round_trip;
    fn table_snapshot;
    fn execute_dataset;
    fn execute_dataset_sources;
    fn dataset_capabilities;
}
