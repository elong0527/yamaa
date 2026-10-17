//! Installed Python metadata, normal callable resolution and exact scalar IO.
//! The shared engine owns activation order, defaults, tests and comparisons.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyDict, PyList, PyString, PyTuple},
};
use std::collections::HashMap;
use yamaa::{
    Argument, BatchArgument, CallError, FunctionError, FunctionHandle, FunctionIdentity,
    HostLanguage, LockContext, LockFinding, LockKind, LockReason, ProjectFunctions, Scalar,
    Signature,
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
            let calls = PyList::new(py, functions.iter().map(|f| &f.call))
                .map_err(|error| crate::function_callback::host_error(py, error))?;
            let reply = Self::helper(py)
                .map_err(|error| crate::function_callback::host_error(py, error))?
                .getattr("verify_versions")
                .map_err(|error| crate::function_callback::host_error(py, error))?
                .call1((PyBytes::new(py, lock.bytes), calls))
                .map_err(|error| crate::function_callback::host_error(py, error))?;
            let rows = reply
                .cast::<PyTuple>()
                .map_err(|_| FunctionError::Protocol)?;
            let mut findings = Vec::with_capacity(rows.len());
            for row in rows {
                let expected = row
                    .getattr("expected")
                    .map_err(|error| crate::function_callback::host_error(py, error))?;
                let expected = expected
                    .cast::<PyTuple>()
                    .map_err(|_| FunctionError::Protocol)?;
                let expected = expected
                    .iter()
                    .map(|value| text(&value))
                    .collect::<Result<Vec<_>, _>>()?;
                let actual = row
                    .getattr("actual")
                    .map_err(|error| crate::function_callback::host_error(py, error))?;
                let finding = LockFinding::new(
                    text(
                        &row.getattr("package")
                            .map_err(|error| crate::function_callback::host_error(py, error))?,
                    )?,
                    LockReason::parse(&text(
                        &row.getattr("reason")
                            .map_err(|error| crate::function_callback::host_error(py, error))?,
                    )?)
                    .ok_or(FunctionError::Protocol)?,
                    expected,
                    if actual.is_none() {
                        None
                    } else {
                        Some(text(&actual)?)
                    },
                );
                findings.push(finding);
            }
            if findings.is_empty() {
                Ok(())
            } else {
                Err(FunctionError::Lock(findings))
            }
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
