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
fn boundary(run: impl FnOnce() -> std::result::Result<Robj, String>) -> List {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(value)) => list!(value = value, error = NULL),
        Ok(Err(error)) => list!(value = NULL, error = error),
        Err(_) => list!(
            value = NULL,
            error = "internal specification prototype failure"
        ),
    }
}
fn address(value: &Robj) -> std::result::Result<usize, String> {
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
fn resolve(value: &Robj) -> std::result::Result<Arc<PreparedRun>, String> {
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
        let schema = CapturedSchema::admit(sources, entry as usize, limits)
            .map_err(|e| capture_failure(&e))?;
        let document = schema
            .prepare_standalone(Source {
                identity: identity.into(),
                bytes: source.as_slice().to_vec(),
            })
            .map_err(|e| capture_failure(&e))?;
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
    })
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
fn specification_failure_report(handle: Robj, capture: Function, metadata: List) -> List {
    boundary(|| {
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::SourcePort,
        };
        struct Port {
            capture: Function,
            reads: usize,
        }
        impl SourcePort for Port {
            type Error = String;
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                source: &yamaa_engine::specification::SourceDeclaration,
                maximum: usize,
            ) -> std::result::Result<Arc<[u8]>, String> {
                let result = self
                    .capture
                    .call(pairlist!(
                        name = source.name.as_str(),
                        path = source.path.as_str(),
                        maximum = maximum as i32
                    ))
                    .map_err(|_| "source capture callback failed")?;
                let result = result
                    .as_list()
                    .filter(|list| list.len() == 2)
                    .ok_or("invalid capture response")?;
                let content = result.elt(0).map_err(|_| "missing capture content")?;
                let content = content
                    .as_raw()
                    .ok_or("capture content must be raw bytes")?;
                let created = result
                    .elt(1)
                    .map_err(|_| "missing capture count")?
                    .as_bool()
                    .ok_or("invalid capture count")?;
                if content.len() > maximum {
                    return Err("capture byte limit".into());
                }
                self.reads += usize::from(created);
                Ok(Arc::from(content.as_slice()))
            }
        }
        if metadata.len() != 5 {
            return Err("invalid report metadata".into());
        }
        let fields = metadata
            .iter()
            .map(|(_, value)| {
                let bytes = value.as_raw().ok_or("report metadata must be raw UTF-8")?;
                if bytes.len() > 4096 {
                    return Err("report metadata limit".into());
                }
                std::str::from_utf8(bytes.as_slice())
                    .map(str::to_owned)
                    .map_err(|_| "invalid report metadata UTF-8".into())
            })
            .collect::<std::result::Result<Vec<String>, String>>()?;
        let run = resolve(&handle)?;
        let attempt = run.execute_with_port(&mut Port { capture, reads: 0 });
        specification_report::failure(
            &run,
            &attempt,
            Identity {
                runtime: "r",
                runtime_version: &fields[0],
                engine_version: &fields[1],
                example: &fields[2],
                specification: &fields[3],
                base_directory: &fields[4],
            },
        )
        .map(|value| r!(value.to_string()))
        .map_err(|_| "unsupported or invalid failure-report observation".into())
    })
}
extendr_module! {mod specification_service;fn prepare_specification;fn specification_source;fn execute_specification_csv;fn specification_failure_report;}
