//! Bounded closed metadata conversion; all fixture admission belongs to engine.
use pyo3::{
    prelude::*,
    types::{PyList, PyString, PyTuple},
};
use yamaa_adapters::source_metadata::{Budget, Limits, Source};

fn invalid() -> PyErr {
    pyo3::exceptions::PyValueError::new_err("invalid source metadata carrier")
}

pub fn convert(value: &Bound<'_, PyList>) -> PyResult<Vec<Source>> {
    let mut budget = Budget::new(value.len(), Limits::default()).map_err(|_| invalid())?;
    for source in value.iter() {
        let source = source.cast::<PyTuple>()?;
        if !matches!(source.len(), 2 | 3) {
            return Err(invalid());
        }
        let offset = source.len() - 2;
        if offset == 1 {
            budget
                .text(source.get_item(0)?.cast::<PyString>()?.to_str()?.len())
                .map_err(|_| invalid())?;
        }
        let dataset = source.get_item(offset)?;
        let name = dataset.cast::<PyString>()?.to_str()?;
        budget.text(name.len()).map_err(|_| invalid())?;
        let columns = source.get_item(offset + 1)?;
        let columns = columns.cast::<PyList>()?;
        budget.columns(columns.len()).map_err(|_| invalid())?;
        for column in columns.iter() {
            let column = column.cast::<PyTuple>()?;
            if column.len() != 2 {
                return Err(invalid());
            }
            for index in 0..2 {
                let item = column.get_item(index)?;
                budget
                    .text(item.cast::<PyString>()?.to_str()?.len())
                    .map_err(|_| invalid())?;
            }
        }
    }
    value
        .iter()
        .map(|source| {
            let source = source.cast::<PyTuple>()?;
            let offset = source.len() - 2;
            let owner = if offset == 1 {
                Some(source.get_item(0)?.extract::<String>()?)
            } else {
                None
            };
            let dataset = source.get_item(offset)?.extract::<String>()?;
            let columns = source.get_item(offset + 1)?;
            let columns = columns
                .cast::<PyList>()?
                .iter()
                .map(|column| {
                    let column = column.cast::<PyTuple>()?;
                    Ok((
                        column.get_item(0)?.extract::<String>()?,
                        column.get_item(1)?.extract::<String>()?,
                    ))
                })
                .collect::<PyResult<Vec<_>>>()?;
            Ok(Source {
                owner,
                dataset,
                columns,
            })
        })
        .collect()
}

pub fn rows(
    run: &dyn yamaa_adapters::specification_run_view::RunView,
    sources: &Bound<'_, PyList>,
) -> PyResult<Vec<crate::specification_result::IssueRow>> {
    let sources = convert(sources)?;
    let issues = yamaa_adapters::source_metadata::check(run, &sources)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))?;
    Ok(crate::specification_result::issue_rows(&issues))
}

pub type InputRow = (String, String, String, String, &'static str);

pub fn input_rows(inputs: Vec<yamaa_adapters::source_metadata::InputMetadata>) -> Vec<InputRow> {
    inputs
        .into_iter()
        .map(|input| {
            let profile = match input.profile {
                yamaa_adapters::source_metadata::Profile::Csv => "csv",
                yamaa_adapters::source_metadata::Profile::Parquet => "parquet",
            };
            (
                input.owner,
                input.dataset,
                input.declaring_source,
                input.written,
                profile,
            )
        })
        .collect()
}

pub fn inputs(
    run: &dyn yamaa_adapters::specification_run_view::RunView,
) -> PyResult<Vec<InputRow>> {
    yamaa_adapters::source_metadata::inputs(run)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))
        .map(input_rows)
}
