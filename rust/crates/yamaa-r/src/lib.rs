//! R composition roots and lossless public result conversion.
use extendr_api::prelude::*;
mod define_entry;
mod domain_entry;
mod file_domain;
mod file_producer;
mod file_project;
mod function_callback;
mod handle;
mod issue_frame;
mod project_activation;
mod project_result;
mod public_table;
mod report_fields;
mod scalars;

#[extendr]
fn core_version() -> &'static str {
    yamaa_core::VERSION
}

extendr_module! {
    mod yamaa;
    fn core_version;
    use domain_entry;
    use define_entry;
    use scalars;
}
