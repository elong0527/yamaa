//! Prototype owned specification handle. Incoming R external pointers are only
//! looked up by address; they are never cast or dereferenced as a Rust object.
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Weak},
};
use yamaa_adapters::{
    specification_diagnostics::{capture_failure, failure},
    specification_run::PreparedRun,
    specification_source::{CapturedSchema, Limits, Source},
};
thread_local! {static HANDLES:RefCell<BTreeMap<usize,Weak<PreparedRun>>>=const{RefCell::new(BTreeMap::new())};}
struct Handle {
    run: Arc<PreparedRun>,
    identity: Cell<usize>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        HANDLES.with(|handles| {
            handles.borrow_mut().remove(&self.identity.get());
        });
    }
}
pub(super) fn boundary(run: impl FnOnce() -> std::result::Result<Robj, String>) -> List {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(value)) => list!(value = value, error = NULL),
        Ok(Err(error)) => list!(value = NULL, error = error),
        Err(_) => list!(
            value = NULL,
            error = "internal specification prototype failure"
        ),
    }
}
pub(super) fn address(value: &Robj) -> std::result::Result<usize, String> {
    if !value.is_external_pointer() {
        return Err("invalid specification handle".into());
    }
    // Merely reads the pointer value after checking SEXPTYPE; no pointee access.
    let address = unsafe { value.external_ptr_addr::<()>() as usize };
    if address == 0 {
        return Err("expired specification handle".into());
    }
    Ok(address)
}
pub(super) fn resolve(value: &Robj) -> std::result::Result<Arc<PreparedRun>, String> {
    let address = address(value)?;
    HANDLES
        .with(|handles| handles.borrow().get(&address).and_then(Weak::upgrade))
        .ok_or_else(|| "unknown specification handle".into())
}
/// Bound and decode raw UTF-8 identities before copying schema/entry snapshots.
#[extendr]
fn prepare_specification(
    names: List,
    modules: List,
    entry: i32,
    identity: Raw,
    source: Raw,
) -> List {
    boundary(|| {
        let (schema, source) = capture_inputs(&names, &modules, entry, &identity, &source)?;
        let document = schema
            .prepare_standalone(source)
            .map_err(|e| capture_failure(&e))?;
        store_document(document)
    })
}
pub(super) fn capture_inputs(
    names: &List,
    modules: &List,
    entry: i32,
    identity: &Raw,
    source: &Raw,
) -> std::result::Result<(Arc<CapturedSchema>, Source), String> {
    let limits = Limits::default();
    if names.len() != modules.len()
        || names.len() > limits.bundle.modules
        || entry < 0
        || identity.len() > limits.identity_bytes
        || source.len() > limits.captured_bytes
    {
        return Err("invalid or over-limit specification sources".into());
    }
    let mut bytes = 0usize;
    let mut name_bytes = 0usize;
    let mut sources = Vec::new();
    // Aggregate admission precedes owned byte copies.
    let buffers = names
        .iter()
        .zip(modules.iter())
        .map(|((_, name), (_, module))| {
            let name = name.as_raw().ok_or("schema names must be raw UTF-8")?;
            let module = module.as_raw().ok_or("schema modules must be raw YAML")?;
            bytes = bytes
                .checked_add(module.len())
                .filter(|&n| n <= limits.captured_bytes)
                .ok_or("captured byte limit")?;
            name_bytes = name_bytes
                .checked_add(name.len())
                .filter(|&n| n <= limits.identity_bytes)
                .ok_or("identity byte limit")?;
            Ok((name, module))
        })
        .collect::<std::result::Result<Vec<_>, String>>()?;
    for (name, module) in buffers {
        sources.push(Source {
            identity: std::str::from_utf8(name.as_slice())
                .map_err(|_| "invalid UTF-8 schema identity")?
                .into(),
            bytes: module.as_slice().to_vec(),
        });
    }
    let identity = std::str::from_utf8(identity.as_slice())
        .map_err(|_| "invalid UTF-8 specification identity")?;
    let schema =
        CapturedSchema::admit(sources, entry as usize, limits).map_err(|e| capture_failure(&e))?;
    Ok((
        schema,
        Source {
            identity: identity.into(),
            bytes: source.as_slice().to_vec(),
        },
    ))
}
pub(super) fn store_document(
    document: yamaa_adapters::specification_source::PreparedDocument,
) -> std::result::Result<Robj, String> {
    let run = Arc::new(PreparedRun::prepare(document).map_err(|e| failure(&e, None))?);
    let handle = ExternalPtr::new(Handle {
        run,
        identity: Cell::new(0),
    });
    let id = address(handle.as_robj())?;
    handle.identity.set(id);
    HANDLES.with(|handles| {
        handles.borrow_mut().insert(id, Arc::downgrade(&handle.run));
    });
    Ok(handle.into_robj())
}

#[extendr]
fn specification_source(handle: Robj) -> List {
    boundary(|| {
        let run = resolve(&handle)?;
        let source = run.source();
        Ok(list!(name = source.name.as_str(), path = source.path.as_str()).into_robj())
    })
}
#[extendr]
fn execute_specification_csv(handle: Robj, source: Raw) -> List {
    boundary(|| {
        let run = resolve(&handle)?;
        let result = run
            .execute_csv(source.as_slice())
            .map_err(|e| failure(&e, Some(run.source())))?;
        let table = result
            .table
            .map_or_else(|| r!(NULL), |bytes| Raw::from_bytes(&bytes).into_robj());
        Ok(list!(table = table, outcome = result.outcome).into_robj())
    })
}
/// Capture callbacks return list(raw content, logical newly_created). They own
/// filesystem authorization/cache policy; the shared compiler owns all semantics.
#[extendr]
fn specification_failure_report(
    handle: Robj,
    capture: Function,
    metadata: List,
    inspect: Robj,
) -> List {
    observed_report(handle, capture, None, metadata, inspect)
}
#[extendr]
fn specification_report(
    handle: Robj,
    capture: Function,
    publish: Function,
    metadata: List,
    inspect: Robj,
) -> List {
    observed_report(handle, capture, Some(publish), metadata, inspect)
}
fn observed_report(
    handle: Robj,
    capture: Function,
    publisher: Option<Function>,
    metadata: List,
    inspect: Robj,
) -> List {
    boundary(|| {
        use crate::specification_result::{
            capture_attempt, fields, identity, inspection, Publisher,
        };
        use yamaa_adapters::specification_report;
        let fields = fields(&metadata)?;
        let run = resolve(&handle)?;
        let attempt = capture_attempt(&run, capture, inspection(inspect)?);
        if let Some(callback) = publisher {
            specification_report::complete(
                &run,
                &attempt,
                identity(&fields),
                &mut Publisher(callback),
            )
            .map(|value| r!(value.to_string()))
            .map_err(|error| match error {
                specification_report::CompleteError::Publish(error) => error,
                specification_report::CompleteError::Report(_) => {
                    "unsupported or invalid report observation".into()
                }
            })
        } else {
            specification_report::failure(&run, &attempt, identity(&fields))
                .map(|value| r!(value.to_string()))
                .map_err(|_| "unsupported or invalid failure-report observation".into())
        }
    })
}

extendr_module! {mod specification_service;fn prepare_specification;fn specification_source;fn execute_specification_csv;fn specification_failure_report;fn specification_report;}
