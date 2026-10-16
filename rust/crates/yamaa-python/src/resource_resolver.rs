//! Translate the host resolver protocol; source semantics stay in the engine.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyDict},
};
use std::sync::Arc;
use yamaa::{InputFrame, ResolverFailure, ResolverReply, ResolverRequest, ResourceResolver};
pub struct Port {
    callback: Py<PyAny>,
}
impl Port {
    pub fn from_callback(callback: Py<PyAny>) -> Arc<dyn ResourceResolver> {
        Arc::new(Self { callback })
    }
}
impl ResourceResolver for Port {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        Python::attach(|py| {
            let run = || -> PyResult<Result<ResolverReply, ResolverFailure>> {
                let args = PyDict::new(py);
                args.set_item("operation", request.operation.name())?;
                args.set_item("kind", request.kind.name())?;
                args.set_item("declaring", request.declaring)?;
                args.set_item("written", request.written)?;
                args.set_item("reference", request.reference)?;
                args.set_item("data", request.bytes.map(|bytes| PyBytes::new(py, bytes)))?;
                let reply = self.callback.bind(py).call1((args,))?;
                let failure = reply.get_item("failure")?;
                if !failure.is_none() {
                    return Ok(Err(ResolverFailure::from_code(
                        failure.extract::<&str>().unwrap_or("rejected"),
                    )));
                }
                let reference = reply.get_item("reference")?.extract::<String>()?;
                let bytes = reply.get_item("data")?;
                let bytes = if bytes.is_none() {
                    None
                } else {
                    Some(Arc::from(bytes.cast::<PyBytes>()?.as_bytes()))
                };
                let frame = reply.get_item("frame")?;
                let frame = if frame.is_none() {
                    None
                } else {
                    let stream = crate::arrow_bridge::take_stream(&frame)?;
                    Some(InputFrame::import(stream).map_err(|error| {
                        pyo3::exceptions::PyValueError::new_err(error.message().to_owned())
                    })?)
                };
                Ok(Ok(ResolverReply::new(reference, bytes, frame)))
            };
            match run() {
                Ok(reply) => reply,
                Err(error) => {
                    let failure = if error
                        .is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py)
                        || error.is_instance_of::<pyo3::exceptions::PySystemExit>(py)
                    {
                        ResolverFailure::from_code("cancelled")
                    } else {
                        ResolverFailure::from_code("rejected")
                    };
                    // Exporters and native representation admission run after
                    // Python dispatch. Retain that exact error on the same
                    // per-attempt owner before the engine projects the failure.
                    let _ = self
                        .callback
                        .bind(py)
                        .getattr("__self__")
                        .and_then(|owner| owner.call_method1("_retain_error", (error.value(py),)));
                    Err(failure)
                }
            }
        })
    }
}
