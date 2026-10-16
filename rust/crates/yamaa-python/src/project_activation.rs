//! Installed Python metadata, normal callable resolution and exact scalar IO.
//! The shared engine owns activation order, defaults, tests and comparisons.
use pyo3::{
    exceptions::PyException,
    prelude::*,
    types::{PyBytes, PyDict, PyList, PyString, PyTuple},
};
use yamaa_adapters::{
    callback::CallbackError,
    project_lock::{Finding, Reason},
};
use yamaa_core::{
    function_batch::{BatchArgument, BatchFunctionPort},
    function_invocation::{Argument, HostError},
    project_activation::ActivationPort,
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::Language,
    value::Value,
};

#[derive(Debug)]
pub enum Error {
    Host(PyErr),
    Lock(Vec<Finding>),
    Result(CallbackError),
    Protocol,
    /// A caller-supplied registry has no binding for the requested call.
    #[allow(dead_code)]
    MissingBinding(String),
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
fn text(value: &Bound<'_, PyAny>) -> Result<String, Error> {
    let value = value.cast::<PyString>().map_err(|_| Error::Protocol)?;
    let value = value.to_str().map_err(Error::Host)?;
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
        if language != Language::Python || lock.kind != LockKind::Uv {
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
        let mut findings = Vec::with_capacity(rows.len());
        for row in rows {
            let expected = row.getattr("expected").map_err(Error::Host)?;
            let expected = expected.cast::<PyTuple>().map_err(|_| Error::Protocol)?;
            let expected = expected
                .iter()
                .map(|value| text(&value))
                .collect::<Result<Vec<_>, _>>()?;
            let actual = row.getattr("actual").map_err(Error::Host)?;
            let finding = Finding {
                package: text(&row.getattr("package").map_err(Error::Host)?)?,
                reason: Reason::parse(&text(&row.getattr("reason").map_err(Error::Host)?)?)
                    .ok_or(Error::Protocol)?,
                expected,
                actual: if actual.is_none() {
                    None
                } else {
                    Some(text(&actual)?)
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

/// Caller-supplied registry port: bindings come from an explicit mapping
/// (staged for the registry activation path; dataset execution still uses
/// the locked port, so this surface is intentionally not yet constructed
/// by an entry point in this crate).
/// of logical call strings to Python callables, never from importing a
/// module. Declared-lock auditing stays separate: this port refuses lock
/// verification instead of silently skipping it, because a registry
/// activation that requested a lock must use the locked port above.
#[allow(dead_code)]
pub struct RegistryPort<'py> {
    py: Python<'py>,
    registry: std::collections::HashMap<String, Py<PyAny>>,
}
#[allow(dead_code)]
impl<'py> RegistryPort<'py> {
    pub fn new(py: Python<'py>, registry: std::collections::HashMap<String, Py<PyAny>>) -> Self {
        Self { py, registry }
    }
    fn lookup(&self, call: &str) -> Result<Py<PyAny>, Error> {
        self.registry
            .get(call)
            .map(|handle| handle.clone_ref(self.py))
            .ok_or_else(|| Error::MissingBinding(call.to_owned()))
    }
}
#[allow(dead_code)]
impl ActivationPort for RegistryPort<'_> {
    type Handle = Py<PyAny>;
    type Error = Error;
    fn verify_lock(
        &mut self,
        _language: Language,
        _lock: &LockReference,
        _functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Error> {
        Err(Error::Protocol)
    }
    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<Py<PyAny>, Error> {
        let handle = self.lookup(&identity.call)?;
        // The registry callable must declare exactly the logical parameter
        // names, checked with the same concrete-signature rules as an
        // installed callable, minus the module import.
        let names = PyList::new(self.py, signature.parameters().iter().map(|p| &p.name))
            .map_err(Error::Host)?;
        self.helper_registry_check(&handle, &identity.call, names)?;
        Ok(handle)
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
#[allow(dead_code)]
impl RegistryPort<'_> {
    fn helper_registry_check(
        &self,
        handle: &Py<PyAny>,
        call: &str,
        names: Bound<'_, PyList>,
    ) -> Result<(), Error> {
        self.py
            .import("yamaa._locked_functions")
            .map_err(Error::Host)?
            .getattr("check_registry_callable")
            .map_err(Error::Host)?
            .call1((handle.bind(self.py), call, names))
            .map_err(Error::Host)?;
        Ok(())
    }
}
#[allow(dead_code)]
impl BatchFunctionPort for RegistryPort<'_> {
    type Handle = Py<PyAny>;
    type Error = Error;
    fn is_interrupt(&self, error: &Error) -> bool {
        ActivationPort::is_interrupt(self, error)
    }
    /// Vectorized Python contract: the callable receives one list per
    /// named parameter (declaration order is the core's, names are the
    /// keyword arguments) and must return a list or tuple of exactly the
    /// batch length. Every element is admitted through the same scalar
    /// result boundary as `invoke`; a non-sequence return, a wrong
    /// length, or one inadmissible element fails the whole call here and
    /// the core re-validates length and types before accepting output.
    fn call_batch(
        &mut self,
        handle: &Py<PyAny>,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Value>, HostError<Error>> {
        let rows = arguments.first().map_or(0, |a| a.values.len());
        let kwargs = PyDict::new(self.py);
        for argument in arguments {
            let column = PyList::empty(self.py);
            for value in &argument.values {
                let cell = crate::function_callback::host_argument(self.py, value.into())
                    .map_err(|error| HostError::Raised(Error::Host(error)))?;
                column
                    .append(cell)
                    .map_err(|error| HostError::Raised(Error::Host(error)))?;
            }
            kwargs
                .set_item(argument.name.as_str(), column)
                .map_err(|error| HostError::Raised(Error::Host(error)))?;
        }
        let returned = handle
            .bind(self.py)
            .call((), Some(&kwargs))
            .map_err(|error| HostError::Raised(Error::Host(error)))?;
        let items: Vec<Bound<'_, PyAny>> = if let Ok(list) = returned.cast::<PyList>() {
            list.iter().collect()
        } else if let Ok(tuple) = returned.cast::<PyTuple>() {
            tuple.iter().collect()
        } else {
            return Err(HostError::InvalidResult(Error::Result(
                CallbackError::Rejected {
                    reason: "a batch binding returned a value of no sequence type".into(),
                    returned: returned
                        .get_type()
                        .name()
                        .ok()
                        .and_then(|n| n.to_str().ok().map(str::to_owned)),
                },
            )));
        };
        if items.len() != rows {
            // The core rejects wrong lengths as a batch failure; surface
            // the raw vector so that check, not a host error, decides.
            return Ok(items
                .iter()
                .map(|item| crate::function_callback::host_result(item).unwrap_or(Value::Missing))
                .collect());
        }
        items
            .iter()
            .map(|item| {
                crate::function_callback::host_result(item)
                    .map_err(|error| HostError::InvalidResult(Error::Result(error)))
            })
            .collect()
    }
}
