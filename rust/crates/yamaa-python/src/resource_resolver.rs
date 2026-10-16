//! Translate the host resolver protocol; source semantics stay in the engine.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyDict},
};
use std::sync::Arc;
use yamaa_adapters::host_resources::{Failure, Reply, Request, Resolver};
pub struct Port {
    callback: Py<PyAny>,
}
impl Port {
    pub fn from_callback(callback: Py<PyAny>) -> Arc<dyn Resolver> {
        Arc::new(Self { callback })
    }
}
impl Resolver for Port {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        Python::attach(|py| {
            let run = || -> PyResult<Result<Reply, Failure>> {
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
                    return Ok(Err(Failure::from_name(failure.extract::<&str>()?)));
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
                    Some(Arc::new(
                        yamaa_adapters::arrow_bridge::import_public_stream(
                            crate::arrow_bridge::take_stream(&frame)?,
                        )
                        .map_err(|_| {
                            pyo3::exceptions::PyValueError::new_err("unsupported host frame")
                        })?,
                    ))
                };
                Ok(Ok(Reply {
                    reference,
                    bytes,
                    frame,
                }))
            };
            match run() {
                Ok(reply) => reply,
                Err(error) => {
                    let failure = if error
                        .is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>(py)
                        || error.is_instance_of::<pyo3::exceptions::PySystemExit>(py)
                    {
                        Failure::Cancelled
                    } else {
                        Failure::Rejected
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
