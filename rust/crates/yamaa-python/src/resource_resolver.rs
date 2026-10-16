//! Translate the host resolver protocol; source semantics stay in the engine.
use pyo3::{prelude::*, types::{PyBytes, PyDict}};
use std::sync::Arc;
use yamaa_adapters::host_resources::{Failure, Reply, Request, Resolver};
pub struct Port { callback: Py<PyAny> }
impl Port {
    pub fn new(callback: Py<PyAny>) -> Arc<dyn Resolver> { Arc::new(Self { callback }) }
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
                    return Ok(Err(match failure.extract::<&str>()? {
                        "missing" => Failure::Missing, "changed" => Failure::Changed,
                        "cancelled" => Failure::Cancelled, "publication" => Failure::Publication,
                        "cleanup" => Failure::Cleanup, _ => Failure::Rejected,
                    }));
                }
                let reference = reply.get_item("reference")?.extract::<String>()?;
                let bytes = reply.get_item("data")?;
                let bytes = if bytes.is_none() { None } else { Some(Arc::from(bytes.cast::<PyBytes>()?.as_bytes())) };
                Ok(Ok(Reply { reference, bytes }))
            };
            run().unwrap_or(Err(Failure::Rejected))
        })
    }
}
