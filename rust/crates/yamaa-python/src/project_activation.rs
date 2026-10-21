//! Installed Python metadata, normal callable resolution and exact scalar IO.
//! The shared engine owns activation order, defaults, tests and comparisons,
//! the uv lock's included: Python reports only what is installed.
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyString, PyTuple},
};
use std::collections::{BTreeMap, HashMap};
use yamaa::{
    Argument, BatchArgument, CallError, FunctionError, FunctionHandle, FunctionIdentity,
    HostLanguage, InstalledDistributions, InstalledVersion, LockContext, LockKind,
    ProjectFunctions, Scalar, Signature,
};

/// Borrow the exact captured lock snapshot for this activation. No construction
/// imports a helper or project module; empty selection has no host effects.
pub struct Port;

impl Port {
    fn helper(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
        // A top-level private host module avoids legacy artifact/cache imports.
        py.import("yamaa._locked_functions")
    }
}

fn text(value: &Bound<'_, PyAny>) -> Result<String, FunctionError> {
    let value = value
        .cast::<PyString>()
        .map_err(|_| FunctionError::Protocol)?;
    value
        .to_str()
        .map(str::to_owned)
        .map_err(|error| crate::function_callback::host_error(value.py(), error))
}

/// The interpreter's installation, which the helper module observes through
/// `importlib.metadata`, `sys` and `platform`. Each observation looks its
/// helper up when it runs, and a raised exception keeps its identity.
struct Installed<'py> {
    py: Python<'py>,
}

impl<'py> Installed<'py> {
    fn helper(&self, name: &str) -> Result<Bound<'py, PyAny>, FunctionError> {
        Port::helper(self.py)
            .and_then(|module| module.getattr(name))
            .map_err(|error| crate::function_callback::host_error(self.py, error))
    }

    fn host(&self, error: PyErr) -> FunctionError {
        crate::function_callback::host_error(self.py, error)
    }
}

impl InstalledDistributions for Installed<'_> {
    fn providers(
        &self,
        modules: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
        let roots = PyTuple::new(self.py, modules).map_err(|error| self.host(error))?;
        let mapping = self
            .helper("_installed_providers")?
            .call1((roots,))
            .map_err(|error| self.host(error))?;
        let mut providers = BTreeMap::new();
        for module in modules {
            let names = mapping
                .call_method1("get", (module,))
                .map_err(|error| self.host(error))?;
            if !names.is_truthy().map_err(|error| self.host(error))? {
                continue;
            }
            let names = names
                .try_iter()
                .map_err(|error| self.host(error))?
                .map(|name| text(&name.map_err(|error| self.host(error))?))
                .collect::<Result<Vec<_>, _>>()?;
            providers.insert(module.clone(), names);
        }
        Ok(providers)
    }

    fn standard_library(&self, module: &str) -> Result<bool, FunctionError> {
        self.helper("is_standard_library")?
            .call1((module,))
            .and_then(|reply| reply.is_truthy())
            .map_err(|error| self.host(error))
    }

    fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
        match self.helper("installed_version")?.call1((distribution,)) {
            Ok(reply) if reply.is_instance_of::<PyString>() => {
                text(&reply).map(InstalledVersion::Version)
            }
            Ok(_) => Ok(InstalledVersion::Unreadable),
            Err(error) => {
                let missing = self
                    .py
                    .import("importlib.metadata")
                    .and_then(|metadata| metadata.getattr("PackageNotFoundError"))
                    .map_err(|error| self.host(error))?;
                if error.is_instance(self.py, &missing) {
                    Ok(InstalledVersion::Missing)
                } else {
                    Err(self.host(error))
                }
            }
        }
    }

    fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
        let reply = self
            .helper("marker_environment")?
            .call0()
            .map_err(|error| self.host(error))?;
        let values = reply
            .cast::<PyDict>()
            .map_err(|_| FunctionError::Protocol)?;
        values
            .iter()
            .map(|(name, value)| Ok((text(&name)?, text(&value)?)))
            .collect()
    }

    fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
        let digits = self
            .helper("integer_digits")?
            .call0()
            .map_err(|error| self.host(error))?
            .extract::<usize>()
            .map_err(|_| FunctionError::Protocol)?;
        // Zero turns the limit off.
        Ok((digits > 0).then_some(digits))
    }
}

impl ProjectFunctions for Port {
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Python::attach(|py| {
            if lock.language != HostLanguage::Python || lock.kind != LockKind::Uv {
                return Err(FunctionError::Protocol);
            }
            yamaa::verify_uv_lock(lock.bytes, functions, &Installed { py })
        })
    }

    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        Python::attach(|py| {
            let names = PyList::new(py, signature.parameters.iter().map(|p| &p.name))
                .map_err(|error| crate::function_callback::host_error(py, error))?;
            Self::helper(py)
                .map_err(|error| crate::function_callback::host_error(py, error))?
                .getattr("resolve_callable")
                .map_err(|error| crate::function_callback::host_error(py, error))?
                .call1((&identity.call, names))
                .map(Bound::unbind)
                .map(FunctionHandle::new)
                .map_err(|error| crate::function_callback::host_error(py, error))
        })
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError> {
        Python::attach(|py| {
            let Some(handle) = handle.with::<Py<PyAny>, _>(|held| held.clone_ref(py)) else {
                return Err(CallError::Raised(FunctionError::Protocol));
            };
            let kwargs = PyDict::new(py);
            for argument in arguments {
                let value = crate::function_callback::host_argument(py, argument.value).map_err(
                    |error| CallError::Raised(crate::function_callback::host_error(py, error)),
                )?;
                kwargs.set_item(argument.name, value).map_err(|error| {
                    CallError::Raised(crate::function_callback::host_error(py, error))
                })?;
            }
            let returned = handle.bind(py).call((), Some(&kwargs)).map_err(|error| {
                CallError::Raised(crate::function_callback::host_error(py, error))
            })?;
            crate::function_callback::host_result(&returned).map_err(CallError::InvalidResult)
        })
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
pub struct RegistryPort {
    registry: HashMap<String, Py<PyAny>>,
}
#[allow(dead_code)]
impl RegistryPort {
    pub fn new(registry: HashMap<String, Py<PyAny>>) -> Self {
        Self { registry }
    }
    fn lookup(&self, call: &str) -> Result<Py<PyAny>, FunctionError> {
        Python::attach(|py| {
            self.registry
                .get(call)
                .map(|handle| handle.clone_ref(py))
                .ok_or_else(|| FunctionError::MissingBinding(call.to_owned()))
        })
    }
}
#[allow(dead_code)]
impl ProjectFunctions for RegistryPort {
    fn verify_lock(
        &self,
        _lock: &LockContext<'_>,
        _functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        Err(FunctionError::Protocol)
    }
    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        Python::attach(|py| {
            let handle = self.lookup(&identity.call)?;
            // The registry callable must declare exactly the logical parameter
            // names, checked with the same concrete-signature rules as an
            // installed callable, minus the module import.
            let names = PyList::new(py, signature.parameters.iter().map(|p| &p.name))
                .map_err(|error| crate::function_callback::host_error(py, error))?;
            py.import("yamaa._locked_functions")
                .map_err(|error| crate::function_callback::host_error(py, error))?
                .getattr("check_registry_callable")
                .map_err(|error| crate::function_callback::host_error(py, error))?
                .call1((handle.bind(py), &identity.call, names))
                .map_err(|error| crate::function_callback::host_error(py, error))?;
            Ok(FunctionHandle::new(handle))
        })
    }
    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError> {
        Port.invoke(handle, arguments)
    }
    /// Vectorized Python contract: the callable receives one list per
    /// named parameter (declaration order is the core's, names are the
    /// keyword arguments) and must return a list or tuple of exactly the
    /// batch length. Every element is admitted through the same scalar
    /// result boundary as `invoke`; a non-sequence return, a wrong
    /// length, or one inadmissible element fails the whole call here and
    /// the core re-validates length and types before accepting output.
    fn call_batch(
        &self,
        handle: &FunctionHandle,
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Scalar>, CallError> {
        Python::attach(|py| {
            let Some(handle) = handle.with::<Py<PyAny>, _>(|held| held.clone_ref(py)) else {
                return Err(CallError::Raised(FunctionError::Protocol));
            };
            let kwargs = PyDict::new(py);
            for argument in arguments {
                let column = PyList::empty(py);
                for value in &argument.values {
                    let cell = crate::function_callback::host_argument(
                        py,
                        yamaa::ScalarRef::from_owned(value),
                    )
                    .map_err(|error| {
                        CallError::Raised(crate::function_callback::host_error(py, error))
                    })?;
                    column.append(cell).map_err(|error| {
                        CallError::Raised(crate::function_callback::host_error(py, error))
                    })?;
                }
                kwargs
                    .set_item(argument.name.as_str(), column)
                    .map_err(|error| {
                        CallError::Raised(crate::function_callback::host_error(py, error))
                    })?;
            }
            let returned = handle.bind(py).call((), Some(&kwargs)).map_err(|error| {
                CallError::Raised(crate::function_callback::host_error(py, error))
            })?;
            let items: Vec<Bound<'_, PyAny>> = if let Ok(list) = returned.cast::<PyList>() {
                list.iter().collect()
            } else if let Ok(tuple) = returned.cast::<PyTuple>() {
                tuple.iter().collect()
            } else {
                return Err(CallError::InvalidResult(FunctionError::Rejected {
                    reason: "a batch binding returned a value of no sequence type".into(),
                    returned: returned
                        .get_type()
                        .name()
                        .ok()
                        .and_then(|n| n.to_str().ok().map(str::to_owned)),
                }));
            };
            if items.len() != rows {
                // The core rejects wrong lengths as a batch failure; surface
                // the raw vector so that check, not a host error, decides.
                return Ok(items
                    .iter()
                    .map(|item| {
                        crate::function_callback::host_result(item).unwrap_or(Scalar::Missing)
                    })
                    .collect());
            }
            items
                .iter()
                .map(|item| {
                    crate::function_callback::host_result(item).map_err(CallError::InvalidResult)
                })
                .collect()
        })
    }
}
