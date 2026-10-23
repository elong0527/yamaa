//! Installed Python metadata, normal callable resolution and exact scalar IO,
//! behind the client's callbacks. The shared engine owns activation order,
//! defaults, tests and comparisons, the uv lock's included: Python reports
//! only what is installed.
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyString, PyTuple},
};
use std::collections::HashMap;
use yamaa_boundary::client::{
    Argument, BatchArgument, CallFailure, Callbacks, Failure, FailureKind, Identity, Installed,
    Scalar, ScalarRef, Signature,
};

use crate::function_callback::{host_argument, host_error, host_result, HostFailure};

/// The callbacks of one call: its interrupt state, and the installed
/// project functions. No construction imports a helper or project module;
/// empty selection has no host effects.
pub struct Port {
    state: Option<crate::interrupt::State>,
    owner: std::thread::ThreadId,
}

impl Port {
    pub fn new(state: &crate::interrupt::State) -> Self {
        Self {
            state: Some(std::sync::Arc::clone(state)),
            owner: std::thread::current().id(),
        }
    }

    pub fn streamed() -> Self {
        Self {
            state: None,
            owner: std::thread::current().id(),
        }
    }

    fn helper(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
        // A top-level private host module avoids legacy artifact/cache imports.
        py.import("yamaa._locked_functions")
    }

    /// The helper's function `name`, looked up when it runs, so a raised
    /// exception keeps its identity.
    fn helper_function<'py>(py: Python<'py>, name: &str) -> Result<Bound<'py, PyAny>, HostFailure> {
        Self::helper(py)
            .and_then(|module| module.getattr(name))
            .map_err(|error| host_error(py, error))
    }
}

fn text(value: &Bound<'_, PyAny>) -> Result<String, HostFailure> {
    let value = value.cast::<PyString>().map_err(|_| Failure::protocol())?;
    value
        .to_str()
        .map(str::to_owned)
        .map_err(|error| host_error(value.py(), error))
}

/// Call `callable` with named arguments.
fn call<'py>(
    py: Python<'py>,
    callable: &Py<PyAny>,
    arguments: impl IntoIterator<Item = (String, Bound<'py, PyAny>)>,
) -> Result<Py<PyAny>, CallFailure<Py<PyAny>>> {
    let kwargs = PyDict::new(py);
    for (name, value) in arguments {
        kwargs
            .set_item(name, value)
            .map_err(|error| CallFailure::Raised(host_error(py, error)))?;
    }
    callable
        .bind(py)
        .call((), Some(&kwargs))
        .map(Bound::unbind)
        .map_err(|error| CallFailure::Raised(host_error(py, error)))
}

/// A call's keyword arguments: each name with its Python value.
type Keywords<'py> = Vec<(String, Bound<'py, PyAny>)>;

/// Each argument converted to the Python value a function receives.
fn arguments<'py>(
    py: Python<'py>,
    arguments: &[Argument<'_>],
) -> Result<Keywords<'py>, CallFailure<Py<PyAny>>> {
    arguments
        .iter()
        .map(|argument| {
            let value = host_argument(py, argument.value)
                .map_err(|error| CallFailure::Raised(host_error(py, error)))?;
            Ok((argument.name.to_owned(), value))
        })
        .collect()
}

impl Callbacks for Port {
    type Object = Py<PyAny>;

    fn cancelled(&self) -> bool {
        if std::thread::current().id() != self.owner {
            return false;
        }
        self.state.as_ref().map_or_else(
            crate::interrupt::stream_cancelled,
            crate::interrupt::cancelled,
        )
    }

    fn bind(
        &self,
        identity: &Identity<'_>,
        signature: &Signature<'_>,
    ) -> Result<Py<PyAny>, HostFailure> {
        Python::attach(|py| {
            let names =
                PyList::new(py, &signature.parameters).map_err(|error| host_error(py, error))?;
            Self::helper_function(py, "resolve_callable")?
                .call1((identity.call, names))
                .map(Bound::unbind)
                .map_err(|error| host_error(py, error))
        })
    }

    fn invoke(
        &self,
        function: &Py<PyAny>,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallFailure<Py<PyAny>>> {
        Python::attach(|py| {
            let returned = call(py, function, self::arguments(py, arguments)?)?;
            host_result(returned.bind(py)).map_err(CallFailure::Invalid)
        })
    }

    fn providers(&self, modules: &[&str]) -> Result<Vec<(String, String)>, HostFailure> {
        Python::attach(|py| {
            let roots = PyTuple::new(py, modules).map_err(|error| host_error(py, error))?;
            let mapping = Self::helper_function(py, "_installed_providers")?
                .call1((roots,))
                .map_err(|error| host_error(py, error))?;
            let mut providers = Vec::new();
            for module in modules {
                let names = mapping
                    .call_method1("get", (module,))
                    .map_err(|error| host_error(py, error))?;
                if !names.is_truthy().map_err(|error| host_error(py, error))? {
                    continue;
                }
                for name in names.try_iter().map_err(|error| host_error(py, error))? {
                    let name = text(&name.map_err(|error| host_error(py, error))?)?;
                    providers.push(((*module).to_owned(), name));
                }
            }
            Ok(providers)
        })
    }

    fn standard_library(&self, module: &str) -> Result<bool, HostFailure> {
        Python::attach(|py| {
            Self::helper_function(py, "is_standard_library")?
                .call1((module,))
                .and_then(|reply| reply.is_truthy())
                .map_err(|error| host_error(py, error))
        })
    }

    fn distribution_version(&self, distribution: &str) -> Result<Installed, HostFailure> {
        Python::attach(|py| {
            match Self::helper_function(py, "installed_version")?.call1((distribution,)) {
                Ok(reply) if reply.is_instance_of::<PyString>() => {
                    text(&reply).map(Installed::Version)
                }
                Ok(_) => Ok(Installed::Unreadable),
                Err(error) => {
                    let missing = py
                        .import("importlib.metadata")
                        .and_then(|metadata| metadata.getattr("PackageNotFoundError"))
                        .map_err(|error| host_error(py, error))?;
                    if error.is_instance(py, &missing) {
                        Ok(Installed::Missing)
                    } else {
                        Err(host_error(py, error))
                    }
                }
            }
        })
    }

    fn environment(&self) -> Result<Vec<(String, String)>, HostFailure> {
        Python::attach(|py| {
            let reply = Self::helper_function(py, "marker_environment")?
                .call0()
                .map_err(|error| host_error(py, error))?;
            let values = reply.cast::<PyDict>().map_err(|_| Failure::protocol())?;
            values
                .iter()
                .map(|(name, value)| Ok((text(&name)?, text(&value)?)))
                .collect()
        })
    }

    fn integer_digits(&self) -> Result<usize, HostFailure> {
        Python::attach(|py| {
            Self::helper_function(py, "integer_digits")?
                .call0()
                .map_err(|error| host_error(py, error))?
                .extract::<usize>()
                .map_err(|_| Failure::protocol())
        })
    }
}

/// Caller-supplied registry port: bindings come from an explicit mapping of
/// logical call strings to Python callables, never from importing a module
/// (staged for the registry activation path; dataset execution still uses
/// the locked port, so no entry point in this crate constructs it yet).
/// Declared-lock auditing stays separate: this port offers no lock
/// verification, so the engine refuses one instead of silently skipping it.
#[allow(dead_code)]
pub struct RegistryPort {
    registry: HashMap<String, Py<PyAny>>,
}

#[allow(dead_code)]
impl RegistryPort {
    pub fn new(registry: HashMap<String, Py<PyAny>>) -> Self {
        Self { registry }
    }
}

impl Callbacks for RegistryPort {
    type Object = Py<PyAny>;

    fn bind(
        &self,
        identity: &Identity<'_>,
        signature: &Signature<'_>,
    ) -> Result<Py<PyAny>, HostFailure> {
        Python::attach(|py| {
            let handle = self
                .registry
                .get(identity.call)
                .map(|handle| handle.clone_ref(py))
                .ok_or_else(|| Failure::new(FailureKind::MissingBinding, identity.call))?;
            // The registry callable must declare exactly the logical parameter
            // names, checked with the same concrete-signature rules as an
            // installed callable, minus the module import.
            let names =
                PyList::new(py, &signature.parameters).map_err(|error| host_error(py, error))?;
            Port::helper_function(py, "check_registry_callable")?
                .call1((handle.bind(py), identity.call, names))
                .map_err(|error| host_error(py, error))?;
            Ok(handle)
        })
    }

    fn invoke(
        &self,
        function: &Py<PyAny>,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallFailure<Py<PyAny>>> {
        Python::attach(|py| {
            let returned = call(py, function, self::arguments(py, arguments)?)?;
            host_result(returned.bind(py)).map_err(CallFailure::Invalid)
        })
    }

    /// Vectorized Python contract: the callable receives one list per named
    /// parameter (declaration order is the core's, names are the keyword
    /// arguments) and must return a list or tuple of exactly the batch
    /// length. Every element is admitted through the same scalar result
    /// boundary as `invoke`; a non-sequence return, a wrong length, or one
    /// inadmissible element fails the whole call here and the core
    /// re-validates length and types before accepting output.
    fn call_batch(
        &self,
        function: &Py<PyAny>,
        rows: usize,
        arguments: &[BatchArgument<'_>],
    ) -> Result<Vec<Scalar>, CallFailure<Py<PyAny>>> {
        Python::attach(|py| {
            let columns = arguments
                .iter()
                .map(|argument| {
                    let column = PyList::empty(py);
                    for value in &argument.values {
                        let cell: ScalarRef<'_> = *value;
                        let cell = host_argument(py, cell)
                            .map_err(|error| CallFailure::Raised(host_error(py, error)))?;
                        column
                            .append(cell)
                            .map_err(|error| CallFailure::Raised(host_error(py, error)))?;
                    }
                    Ok((argument.name.to_owned(), column.into_any()))
                })
                .collect::<Result<Vec<_>, CallFailure<Py<PyAny>>>>()?;
            let returned = call(py, function, columns)?;
            let returned = returned.bind(py);
            let items: Vec<Bound<'_, PyAny>> = if let Ok(list) = returned.cast::<PyList>() {
                list.iter().collect()
            } else if let Ok(tuple) = returned.cast::<PyTuple>() {
                tuple.iter().collect()
            } else {
                let returned_type = returned
                    .get_type()
                    .name()
                    .ok()
                    .and_then(|n| n.to_str().ok().map(str::to_owned));
                return Err(CallFailure::Invalid(Failure {
                    returned: returned_type,
                    ..Failure::new(
                        FailureKind::Rejected,
                        "a batch binding returned a value of no sequence type",
                    )
                }));
            };
            if items.len() != rows {
                // The core rejects wrong lengths as a batch failure; surface
                // the raw vector so that check, not a host error, decides.
                return Ok(items
                    .iter()
                    .map(|item| host_result(item).unwrap_or(Scalar::Missing))
                    .collect());
            }
            items
                .iter()
                .map(|item| host_result(item).map_err(CallFailure::Invalid))
                .collect()
        })
    }
}
