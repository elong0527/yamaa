//! R composition roots and lossless public result conversion.
use extendr_api::prelude::*;
mod arrow_input;
mod define_entry;
mod domain_entry;
mod file_domain;
mod file_producer;
mod file_project;
mod file_publication;
mod file_resources;
mod file_specification;
mod function_callback;
mod issue_frame;
mod project_activation;
mod project_result;
mod public_table;
mod scalars;
mod specification_inheritance;
mod specification_result;
mod specification_service;

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
    use specification_service;
    use arrow_input;
    use specification_result;
    use project_result;
    use specification_inheritance;
    use file_publication;
    use file_resources;
    use file_specification;
    use file_project;
    use file_producer;
}
