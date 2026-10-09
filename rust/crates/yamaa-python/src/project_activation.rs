//! Installed Python metadata, normal callable resolution and exact scalar IO.
//! The shared engine owns activation order, defaults, tests and comparisons.
use pyo3::{
    exceptions::PyException,
    prelude::*,
    types::{PyBytes, PyDict, PyList, PyString, PyTuple},
};
use yamaa_adapters::{
    function_transport::CallbackError,
    project_lock::{Finding, Reason},
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::Language,
    value::Value,
};
use yamaa_engine::{
    function_invocation::{Argument, HostError},
    project_activation::ActivationPort,
};

#[derive(Debug)]
pub enum Error {
    Host(PyErr),
    Lock(Vec<Finding>),
    Result(CallbackError),
    Protocol,
}
impl Error {
    pub fn facts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let facts = PyDict::new(py);
        match self {
            Self::Host(error) => {
                facts.set_item("kind", "host")?;
                facts.set_item("exception", error.value(py))?;
            }
            Self::Lock(findings) => {
                facts.set_item("kind", "lock")?;
                let rows = findings
                    .iter()
                    .map(|finding| {
                        let row = PyDict::new(py);
                        row.set_item("package", &finding.package)?;
                        row.set_item("reason", finding.reason.as_str())?;
                        row.set_item("expected", &finding.expected)?;
                        row.set_item("actual", &finding.actual)?;
                        Ok(row)
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                facts.set_item("findings", PyList::new(py, rows)?)?;
            }
            Self::Result(error) => {
                facts.set_item("kind", "representation")?;
                match error {
                    CallbackError::Rejected { reason, returned } => {
                        facts.set_item("reason", reason)?;
                        facts.set_item("returned", returned)?;
                    }
                    CallbackError::Exception {
                        class,
                        message,
                        truncated,
                    } => {
                        facts.set_item("class", class)?;
                        facts.set_item("message", message)?;
                        facts.set_item("truncated", truncated)?;
                    }
                    CallbackError::Boundary(error) => facts.set_item(
                        "internal",
                        matches!(
                            error,
                            yamaa_adapters::function_transport::FunctionTransportError::Internal
                        ),
                    )?,
                }
            }
            Self::Protocol => facts.set_item("kind", "protocol")?,
        }
        Ok(facts)
    }
}
/// Borrow the exact captured lock snapshot for this activation. No construction
/// imports a helper or project module; empty selection has no host effects.
pub struct Port<'py, 'lock> {
    py: Python<'py>,
    lock: &'lock [u8],
}
impl<'py, 'lock> Port<'py, 'lock> {
    pub fn new(py: Python<'py>, lock: &'lock [u8]) -> Self {
        Self { py, lock }
    }
    fn helper(&self) -> PyResult<Bound<'py, PyModule>> {
        // A top-level private host module avoids legacy artifact/cache imports.
        self.py.import("yamaa._locked_functions")
    }
}
fn text(value: &Bound<'_, PyAny>, remaining: &mut usize) -> Result<String, Error> {
    let value = value.cast::<PyString>().map_err(|_| Error::Protocol)?;
    let value = value.to_str().map_err(Error::Host)?;
    if value.len() > 2048 {
        return Err(Error::Protocol);
    }
    *remaining = remaining.checked_sub(value.len()).ok_or(Error::Protocol)?;
    Ok(value.to_owned())
}
impl ActivationPort for Port<'_, '_> {
    type Handle = Py<PyAny>;
    type Error = Error;
    fn verify_lock(
        &mut self,
        language: Language,
        lock: &LockReference,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Error> {
        if language != Language::Python
            || lock.kind != LockKind::Uv
            || self.lock.len() > 16_777_216
            || functions.len() > 1024
        {
            return Err(Error::Protocol);
        }
        let calls = PyList::new(self.py, functions.iter().map(|f| &f.call)).map_err(Error::Host)?;
        let reply = self
            .helper()
            .map_err(Error::Host)?
            .getattr("verify_versions")
            .map_err(Error::Host)?
            .call1((PyBytes::new(self.py, self.lock), calls))
            .map_err(Error::Host)?;
        let rows = reply.cast::<PyTuple>().map_err(|_| Error::Protocol)?;
        if rows.len() > 2_049 {
            return Err(Error::Protocol);
        }
        let mut findings = Vec::with_capacity(rows.len());
        let mut remaining_text = 16_777_216;
        let mut remaining_versions: usize = 65_536;
        for row in rows {
            let expected = row.getattr("expected").map_err(Error::Host)?;
            let expected = expected.cast::<PyTuple>().map_err(|_| Error::Protocol)?;
            remaining_versions = remaining_versions
                .checked_sub(expected.len())
                .ok_or(Error::Protocol)?;
            let expected = expected
                .iter()
                .map(|value| text(&value, &mut remaining_text))
                .collect::<Result<Vec<_>, _>>()?;
            let actual = row.getattr("actual").map_err(Error::Host)?;
            let finding = Finding {
                package: text(
                    &row.getattr("package").map_err(Error::Host)?,
                    &mut remaining_text,
                )?,
                reason: Reason::parse(&text(
                    &row.getattr("reason").map_err(Error::Host)?,
                    &mut remaining_text,
                )?)
                .ok_or(Error::Protocol)?,
                expected,
                actual: if actual.is_none() {
                    None
                } else {
                    Some(text(&actual, &mut remaining_text)?)
                },
            };
            findings.push(finding);
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(Error::Lock(findings))
        }
    }
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<Py<PyAny>, Error> {
        let names = PyList::new(self.py, signature.parameters().iter().map(|p| &p.name))
            .map_err(Error::Host)?;
        self.helper()
            .map_err(Error::Host)?
            .getattr("resolve_callable")
            .map_err(Error::Host)?
            .call1((&identity.call, names))
            .map(Bound::unbind)
            .map_err(Error::Host)
    }
    fn is_interrupt(&self, error: &Error) -> bool {
        matches!(error, Error::Host(error) if !error.is_instance_of::<PyException>(self.py))
    }
    fn invoke(
        &mut self,
        handle: &Py<PyAny>,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Error>> {
        let kwargs = PyDict::new(self.py);
        for argument in arguments {
            let value = crate::function_callback::host_argument(self.py, argument.value)
                .map_err(|error| HostError::Raised(Error::Host(error)))?;
            kwargs
                .set_item(argument.name, value)
                .map_err(|error| HostError::Raised(Error::Host(error)))?;
        }
        let returned = handle
            .bind(self.py)
            .call((), Some(&kwargs))
            .map_err(|error| HostError::Raised(Error::Host(error)))?;
        crate::function_callback::host_result(&returned)
            .map_err(|error| HostError::InvalidResult(Error::Result(error)))
    }
}
