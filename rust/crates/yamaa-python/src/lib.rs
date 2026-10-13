//! Python composition roots and lossless public result conversion.
use pyo3::prelude::*;
mod define_entry;
mod domain_entry;
#[cfg(any(unix, windows))]
mod file_producer;
#[cfg(any(unix, windows))]
mod file_project;
#[cfg(any(unix, windows))]
mod file_specification;
mod function_callback;
mod project_activation;
#[cfg(any(unix, windows))]
mod project_result;
mod specification_inheritance;
mod specification_result;
mod specification_service;
mod style;
mod temporal_result;
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("_CORE_VERSION", yamaa_engine::host_values::VERSION)?;
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
        domain_entry::_check_project_metadata,
        module
    )?)?;
    #[cfg(any(unix, windows))]
    {
        module.add_class::<file_specification::Specification>()?;
        module.add_class::<file_project::Specification>()?;
        module.add_class::<file_project::Attempt>()?;
        module.add_class::<file_producer::Specification>()?;
        module.add_class::<file_producer::Attempt>()?;
        module.add_class::<file_producer::Report>()?;
        module.add_function(wrap_pyfunction!(
            file_producer::_prepare_producer_file,
            module
        )?)?;
        module.add_class::<project_result::Result>()?;
        module.add_function(wrap_pyfunction!(
            file_project::_prepare_file_project,
            module
        )?)?;
        module.add_function(wrap_pyfunction!(
            file_specification::_prepare_file_specification,
            module
        )?)?;
    }
    module.add_function(wrap_pyfunction!(
        specification_inheritance::_prepare_document,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(temporal_result::_temporal_result, module)?)?;
    module.add_class::<specification_service::Specification>()?;
    module.add_class::<specification_result::BuildResult>()?;
    module.add_function(wrap_pyfunction!(
        specification_service::_prepare_specification,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        specification_inheritance::_prepare_inherited_specification,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(style::_style_same_value, module)?)?;
    Ok(())
}
