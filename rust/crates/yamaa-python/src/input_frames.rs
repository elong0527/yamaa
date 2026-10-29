//! Per-call Arrow inputs. No host object survives the owned frame import.
use pyo3::{
    prelude::*,
    types::{PyCapsule, PyDict, PyModule},
};
use std::collections::BTreeMap;

pub(crate) fn import(
    specification: &str,
    inputs: Option<Bound<'_, PyAny>>,
) -> PyResult<BTreeMap<String, yamaa::InputFrame>> {
    let Some(inputs) = inputs.filter(|value| !value.is_none()) else {
        return Ok(BTreeMap::new());
    };
    let py = inputs.py();
    let inputs = inputs.cast::<PyDict>().map_err(|_| {
        crate::errors::message(
            py,
            false,
            "inputs must be a dictionary of input aliases and frames",
        )
    })?;
    // A provider can mutate the caller's dictionary while exporting its
    // stream. Retain every entry before invoking Python code.
    let entries = inputs.iter().collect::<Vec<_>>();
    let mut frames = BTreeMap::new();
    for (name, frame) in entries {
        let alias = name
            .extract::<String>()
            .map_err(|_| crate::errors::message(py, false, "input aliases must be text"))?;
        let result = import_one(specification, &alias, &frame).map_err(|error| {
            if error.is_instance_of::<crate::errors::Error>(py) {
                return error;
            }
            let refusal = crate::errors::message(py, false, error.to_string());
            refusal.set_cause(py, Some(error));
            refusal
        });
        frames.insert(alias, result?);
    }
    Ok(frames)
}

fn import_one(
    specification: &str,
    alias: &str,
    frame: &Bound<'_, PyAny>,
) -> PyResult<yamaa::InputFrame> {
    let py = frame.py();
    let converted;
    let frame = if is_pandas_frame(frame)? {
        // pandas uses PyArrow's preserving conversion; index values are not
        // study columns. Other frames can export the capsule protocol directly.
        let kwargs = PyDict::new(py);
        kwargs.set_item("preserve_index", false)?;
        converted = PyModule::import(py, "pyarrow")?
            .getattr("Table")?
            .call_method("from_pandas", (frame,), Some(&kwargs))?;
        &converted
    } else if frame.hasattr("__arrow_c_stream__")? {
        frame
    } else {
        return Err(crate::errors::message(
            py,
            false,
            format!("input {alias} must export an Arrow stream (pandas requires pyarrow)"),
        ));
    };
    let capsule = frame.call_method0("__arrow_c_stream__")?;
    let capsule = capsule.cast::<PyCapsule>().map_err(|_| {
        crate::errors::message(
            py,
            false,
            "__arrow_c_stream__ must return an Arrow stream capsule",
        )
    })?;
    let pointer = capsule
        .pointer_checked(Some(c"arrow_array_stream"))?
        .cast::<yamaa::FFI_ArrowArrayStream>();
    // SAFETY: the Arrow PyCapsule protocol owns a live stream at this
    // checked pointer. from_raw moves it and leaves a released header, so
    // dropping the capsule cannot release the moved stream twice. The
    // capsule stays alive through import; the facade copies every value.
    let stream = unsafe { yamaa::FFI_ArrowArrayStream::from_raw(pointer.as_ptr()) };
    yamaa::InputFrame::import(stream)
        .map_err(|error| crate::errors::failure(py, error.at_input(specification, alias)))
}

/// pandas also exports Arrow streams, but their default conversion includes
/// non-RangeIndex values. Check the loaded class first, including subclasses,
/// without requiring pandas for other stream providers.
fn is_pandas_frame(frame: &Bound<'_, PyAny>) -> PyResult<bool> {
    let modules = PyModule::import(frame.py(), "sys")?
        .getattr("modules")?
        .cast_into::<PyDict>()?;
    match modules.get_item("pandas")? {
        Some(pandas) => frame.is_instance(&pandas.getattr("DataFrame")?),
        None => Ok(false),
    }
}
