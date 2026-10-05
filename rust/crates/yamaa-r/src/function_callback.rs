//! Synchronous R callback capability; all R access stays on the calling thread.
use extendr_api::prelude::*;
use yamaa_adapters::{
    function_transport::{
        CallbackError, FunctionTransportError as Error, PreparedInvocation, MAX_ERROR_BYTES,
        MAX_REQUEST_BYTES,
    },
    scalar_bytes,
};
use yamaa_core::{table::ValueRef, value::Value};
use yamaa_engine::function_invocation::{Argument, FunctionPort, HostError};

/// Invoke an explicitly supplied R dispatcher after complete request admission.
/// The package dispatcher owns the bound callback; labels are not artifact proof.
#[extendr]
fn invoke_function(request: Raw, dispatch: Function) -> List {
    let run = || {
        if request.len() > MAX_REQUEST_BYTES {
            return Err(Error::RequestLimit);
        }
        let text = std::str::from_utf8(request.as_slice()).map_err(|_| Error::InvalidRequest)?;
        let prepared = PreparedInvocation::parse(text)?;
        if !prepared.host_names().all(host_name) {
            return Err(Error::InvalidHostName);
        }
        prepared.invoke(&mut RPort { dispatch })
    };
    match run() {
        Ok(value) => list!(value = value, error = NULL),
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

/// Execute an admitted dataset with a bounded snapshot of R-owned callback dispatchers.
#[extendr]
fn execute_dataset_functions(
    request: Raw,
    source: Raw,
    secondary: List,
    dispatchers: List,
) -> List {
    use yamaa_adapters::dataset_transport::{
        DatasetTransportError as DatasetError, PreparedDataset, MAX_SOURCES,
    };
    let run = || {
        if request.len() > yamaa_adapters::scalar_transport::MAX_REQUEST_BYTES {
            return Err(DatasetError::RequestLimit);
        }
        let text =
            std::str::from_utf8(request.as_slice()).map_err(|_| DatasetError::InvalidRequest)?;
        let prepared = PreparedDataset::parse(text)?;
        if dispatchers.len() != prepared.function_signatures().len() {
            return Err(DatasetError::FunctionBinding);
        }
        if !prepared
            .function_signatures()
            .iter()
            .all(|s| s.parameters().iter().all(|p| host_name(&p.host_name)))
        {
            return Err(DatasetError::Function(Error::InvalidHostName));
        }
        let callbacks = dispatchers
            .iter()
            .map(|(_, value)| value.as_function().ok_or(DatasetError::FunctionBinding))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if secondary.len() >= MAX_SOURCES {
            return Err(DatasetError::RequestLimit);
        }
        let buffers = secondary
            .iter()
            .map(|(_, value)| value.as_raw().ok_or(DatasetError::InvalidRequest))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let slices = buffers
            .iter()
            .map(|bytes| bytes.as_slice())
            .collect::<Vec<_>>();
        let mut bindings = RBindings {
            signatures: prepared.function_signatures(),
            callbacks,
        };
        prepared.execute_sources_functions(source.as_slice(), &slices, &mut bindings)
    };
    match run() {
        Ok(result) => {
            let table = result
                .table
                .map_or_else(|| r!(NULL), |bytes| Raw::from_bytes(&bytes).into_robj());
            list!(
                value = list!(table = table, outcome = result.outcome),
                error = NULL
            )
        }
        Err(error) => list!(value = NULL, error = error.to_string()),
    }
}

struct RBindings<'a> {
    signatures: &'a [yamaa_engine::function_invocation::InvocationPlan],
    callbacks: Vec<Function>,
}
impl yamaa_engine::dataset::FunctionBindings for RBindings<'_> {
    type Error = CallbackError;
    /// Borrow immutable admitted metadata independently of callback evaluation.
    fn signature(&self, slot: usize) -> Option<&yamaa_engine::function_invocation::InvocationPlan> {
        self.signatures.get(slot)
    }
    /// Root one stable dispatcher and reuse the existing scalar/condition boundary.
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Value, HostError<CallbackError>> {
        RPort {
            dispatch: self.callbacks[slot].clone(),
        }
        .call(arguments)
    }
}

/// Match the environment's ASCII R host-name policy, including reserved forms.
fn host_name(name: &str) -> bool {
    let b = name.as_bytes();
    let start = match b.first() {
        Some(c) if c.is_ascii_alphabetic() => true,
        Some(b'.') => b.len() > 1 && !b[1].is_ascii_digit(),
        _ => false,
    };
    start
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_'))
        && name != "..."
        && !(name.len() > 2
            && name.starts_with("..")
            && name[2..].bytes().all(|c| c.is_ascii_digit()))
        && !matches!(
            name,
            "break"
                | "else"
                | "FALSE"
                | "for"
                | "function"
                | "if"
                | "Inf"
                | "in"
                | "NA"
                | "NA_character_"
                | "NA_complex_"
                | "NA_integer_"
                | "NA_real_"
                | "NaN"
                | "next"
                | "NULL"
                | "repeat"
                | "TRUE"
                | "while"
        )
}

struct RPort {
    dispatch: Function,
}
impl FunctionPort for RPort {
    type Error = CallbackError;
    /// Marshal owned byte scalars, call once through R_tryEval, and decode raw
    /// result fields by position. Never inspect arbitrary R character pointers.
    fn call(
        &mut self,
        arguments: &[Argument<'_>],
    ) -> std::result::Result<Value, HostError<CallbackError>> {
        let mut encoded = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = match argument.value {
                ValueRef::Missing => Value::Missing,
                ValueRef::Int(n) => Value::Int(n),
                ValueRef::Float(n) => Value::Float(n),
                ValueRef::Str(s) => Value::Str(s.into()),
                ValueRef::Bool(b) => Value::Bool(b),
                ValueRef::Date(d) => Value::Date(d),
                ValueRef::DateTime(d) => Value::DateTime(d),
            };
            let value = scalar_bytes::encode(value).map_err(|_| internal())?;
            encoded.push((
                argument.name,
                Robj::from(list!(
                    tag = value.tag,
                    payload = Raw::from_bytes(&value.payload)
                )),
            ));
        }
        let encoded = List::from_pairs(encoded);
        let result = self
            .dispatch
            .call(pairlist!(encoded = encoded))
            .map_err(|_| internal())?;
        let result = result
            .as_list()
            .filter(|v| v.len() == 3)
            .ok_or_else(internal)?;
        let status = result
            .elt(0)
            .map_err(|_| internal())?
            .as_integer()
            .ok_or_else(internal)?;
        let first = result.elt(1).map_err(|_| internal())?;
        let second = result.elt(2).map_err(|_| internal())?;
        match status {
            0 => {
                let tag = first.as_integer().ok_or_else(internal)?;
                let payload = second.as_raw().ok_or_else(internal)?;
                scalar_bytes::decode(tag, payload.as_slice()).map_err(|reason| {
                    HostError::InvalidResult(CallbackError::Rejected {
                        reason: reason.into(),
                        returned: None,
                    })
                })
            }
            1 => {
                let (class, class_cut) = detail(&first, "R condition")?;
                let (message, message_cut) = detail(&second, "unavailable R condition message")?;
                Err(HostError::Raised(CallbackError::Exception {
                    class,
                    message,
                    truncated: class_cut || message_cut,
                }))
            }
            2 => {
                let (reason, _) = detail(&first, "a binding returned a value of no scalar type")?;
                let (returned, _) = detail(&second, "unknown")?;
                Err(HostError::InvalidResult(CallbackError::Rejected {
                    reason,
                    returned: Some(returned),
                }))
            }
            3 => Err(HostError::Raised(CallbackError::Boundary(
                Error::Interrupted,
            ))),
            4 => Err(HostError::Raised(CallbackError::Boundary(
                Error::OutputLimit,
            ))),
            _ => Err(internal()),
        }
    }
}

/// Unexpected dispatcher failures are boundary errors, not repaired host results.
fn internal() -> HostError<CallbackError> {
    HostError::Raised(CallbackError::Boundary(Error::Internal))
}

/// Decode host detail bytes strictly and cap only at a Unicode scalar boundary.
/// Invalid encodings retain a stable fallback; raw invalid text is never repaired.
fn detail(
    value: &Robj,
    fallback: &str,
) -> std::result::Result<(String, bool), HostError<CallbackError>> {
    let bytes = value.as_raw().ok_or_else(internal)?;
    let text = match std::str::from_utf8(bytes.as_slice()) {
        Ok(text) => text,
        Err(_) => return Ok((fallback.into(), false)),
    };
    let mut end = text.len().min(MAX_ERROR_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((text[..end].into(), end < text.len()))
}

extendr_module! {
    mod function_callback;
    fn invoke_function;
    fn execute_dataset_functions;
}
