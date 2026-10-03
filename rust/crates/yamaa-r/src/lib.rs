//! Optional R installation probe; all R interactions occur on the calling thread.
use extendr_api::prelude::*;

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
}
