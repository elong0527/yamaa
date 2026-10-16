//! Python composition roots and lossless public result conversion.
use pyo3::prelude::*;
mod define_entry;
mod domain_entry;
mod file_domain;
mod file_producer;
mod file_project;
mod function_callback;
mod project_activation;
mod project_result;
mod report_fields;
mod resolved_artifact;
mod style;
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("_CORE_VERSION", yamaa_core::VERSION)?;
    module.add_class::<define_entry::Define>()?;
    module.add(
        "DefineError",
        module.py().get_type::<define_entry::DefineError>(),
    )?;
    module.add_function(wrap_pyfunction!(define_entry::_define_file, module)?)?;
    module.add_class::<domain_entry::Domain>()?;
    module.add(
        "DomainError",
        module.py().get_type::<domain_entry::DomainError>(),
    )?;
    module.add_function(wrap_pyfunction!(domain_entry::_domain_file, module)?)?;
    module.add_function(wrap_pyfunction!(domain_entry::_check_file, module)?)?;
    module.add_function(wrap_pyfunction!(
        resolved_artifact::_resolve_specification_file,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        resolved_artifact::_read_resolved_specification,
        module
    )?)?;
    module.add_class::<file_producer::Attempt>()?;
    module.add_function(wrap_pyfunction!(style::_style_same_value, module)?)?;
    Ok(())
}
