//! Registered owned result handles; callers never supply a Rust pointer to dereference.
use crate::specification_service::{address, boundary, resolve as resolve_specification};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use yamaa_adapters::{
    specification_report::{self, BuildResult, Identity},
    specification_run::{CapturedAttempt, PreparedRun, SourcePort},
};
use yamaa_core::resource::ResourceFailure;

pub(super) struct CaptureError {
    failure: Option<ResourceFailure>,
    // Retain the host condition without deriving a cause from its class/message.
    _payload: Option<Robj>,
    _message: String,
}
impl From<&str> for CaptureError {
    fn from(message: &str) -> Self {
        Self {
            failure: None,
            _payload: None,
            _message: message.into(),
        }
    }
}

fn failure_reply(value: &Robj) -> std::result::Result<CaptureError, &'static str> {
    let result = value
        .as_list()
        .filter(|value| value.len() == 2)
        .ok_or("invalid resource failure reply")?;
    let kind = result.elt(0).map_err(|_| "missing failure kind")?;
    let failure = match kind.as_str().ok_or("invalid failure kind")? {
        "missing" => ResourceFailure::Missing,
        "not_regular_file" => ResourceFailure::NotRegularFile,
        _ => return Err("invalid capture failure kind"),
    };
    let payload = result.elt(1).map_err(|_| "missing capture error")?;
    if !payload.inherits("condition") {
        return Err("capture error must be a condition");
    }
    Ok(CaptureError {
        failure: (!payload.inherits("interrupt")).then_some(failure),
        _payload: Some(payload),
        _message: String::new(),
    })
}

pub(super) fn inspection(value: Robj) -> std::result::Result<Option<Function>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        value
            .as_function()
            .map(Some)
            .ok_or_else(|| "inspect must be a function".into())
    }
}

thread_local! {static RESULTS:RefCell<BTreeMap<usize,Weak<BuildResult>>>=const{RefCell::new(BTreeMap::new())};}
struct Handle {
    result: Arc<BuildResult>,
    identity: Cell<usize>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        RESULTS.with(|values| {
            values.borrow_mut().remove(&self.identity.get());
        });
    }
}
pub(super) fn resolve(value: &Robj) -> std::result::Result<Arc<BuildResult>, String> {
    let id = address(value)?;
    RESULTS
        .with(|values| values.borrow().get(&id).and_then(Weak::upgrade))
        .ok_or_else(|| "unknown build result handle".into())
}
pub(super) fn store(result: BuildResult) -> std::result::Result<Robj, String> {
    let handle = ExternalPtr::new(Handle {
        result: Arc::new(result),
        identity: Cell::new(0),
    });
    let id = address(handle.as_robj())?;
    handle.identity.set(id);
    RESULTS.with(|values| {
        values
            .borrow_mut()
            .insert(id, Arc::downgrade(&handle.result));
    });
    Ok(handle.into_robj())
}
pub(super) fn fields(metadata: &List) -> std::result::Result<Vec<String>, String> {
    if metadata.len() != 5 {
        return Err("invalid report metadata".into());
    }
    metadata
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
        .collect()
}
pub(super) fn identity(fields: &[String]) -> Identity<'_> {
    Identity {
        runtime: "r",
        runtime_version: &fields[0],
        engine_version: &fields[1],
        example: &fields[2],
        specification: &fields[3],
        base_directory: &fields[4],
    }
}
struct Port {
    capture: Function,
    inspect: Option<Function>,
    reads: usize,
}
impl SourcePort for Port {
    type Error = CaptureError;
    fn resource_failure(&self, error: &CaptureError) -> Option<ResourceFailure> {
        error.failure
    }
    fn inspect(
        &mut self,
        source: &yamaa_engine::specification::SourceDeclaration,
    ) -> std::result::Result<(), CaptureError> {
        if let Some(inspect) = &self.inspect {
            let result = inspect
                .call(pairlist!(
                    name = source.name.as_str(),
                    path = source.path.as_str()
                ))
                .map_err(|_| "source inspection callback failed")?;
            if !result.is_null() {
                return Err(failure_reply(&result)?);
            }
        }
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(
        &mut self,
        source: &yamaa_engine::specification::SourceDeclaration,
        maximum: usize,
    ) -> std::result::Result<Arc<[u8]>, CaptureError> {
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
            .filter(|v| v.len() == 2)
            .ok_or("invalid capture response")?;
        let content = result.elt(0).map_err(|_| "missing capture content")?;
        if content.as_str().is_some() {
            return Err(failure_reply(result.as_robj())?);
        }
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
pub(super) fn capture_attempt(
    run: &PreparedRun,
    capture: Function,
    inspect: Option<Function>,
) -> CapturedAttempt<CaptureError> {
    run.execute_with_port(&mut Port {
        capture,
        inspect,
        reads: 0,
    })
}
pub(super) struct Publisher(pub(super) Function);
impl specification_report::ArtifactPort for Publisher {
    type Error = String;
    fn publish(&mut self, path: &str, content: &[u8]) -> std::result::Result<(), String> {
        let result = self
            .0
            .call(pairlist!(path = path, content = Raw::from_bytes(content)))
            .map_err(|_| "publication callback failed")?;
        if result.as_bool() != Some(true) {
            return Err("publication callback rejected output".into());
        }
        Ok(())
    }
}
#[extendr]
fn specification_build(handle: Robj, capture: Function, metadata: List, inspect: Robj) -> List {
    boundary(|| {
        let fields = fields(&metadata)?;
        let run = resolve_specification(&handle)?;
        let attempt = capture_attempt(&run, capture, inspection(inspect)?);
        store(
            specification_report::build_result(&run, &attempt, identity(&fields))
                .map_err(|_| "unsupported or invalid build observation")?,
        )
    })
}
#[extendr]
fn build_output(handle: Robj) -> List {
    boundary(|| {
        Ok(resolve(&handle)?
            .output()
            .map_or_else(|| r!(NULL), |v| Raw::from_bytes(v).into_robj()))
    })
}
#[extendr]
fn build_observations(handle: Robj) -> List {
    boundary(|| Ok(r!(resolve(&handle)?.observations().to_string())))
}
#[extendr]
fn build_issues(handle: Robj) -> List {
    boundary(|| crate::issue_frame::frame(resolve(&handle)?.issues()))
}
#[extendr]
fn build_save(handle: Robj, publish: Function) -> List {
    boundary(|| {
        resolve(&handle)?
            .save(&mut Publisher(publish))
            .map(|v| r!(v.to_string()))
            .map_err(|error| match error {
                yamaa_engine::specification_output::SaveError::FailedBuild => {
                    "cannot save a failed build".into()
                }
                yamaa_engine::specification_output::SaveError::Publish(error) => error,
            })
    })
}
extendr_module! {mod specification_result; fn specification_build; fn build_output; fn build_observations; fn build_issues; fn build_save;}
