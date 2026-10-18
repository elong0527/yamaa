//! Registered owned public result handles; the engine owns file orchestration.
use crate::handle::{address, boundary};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::Arc,
};
use yamaa::{Build, CancellationToken, Engine, EngineOptions, HostLanguage, Request, SaveOutcome};

pub(crate) struct State {
    pub(crate) build: Build,
}
thread_local! { static HANDLES:RefCell<BTreeMap<usize,Weak<RefCell<State>>>>=const { RefCell::new(BTreeMap::new()) }; }
struct Handle {
    state: Rc<RefCell<State>>,
    identity: Cell<usize>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        HANDLES.with(|h| {
            h.borrow_mut().remove(&self.identity.get());
        });
    }
}
fn store(state: State) -> std::result::Result<Robj, String> {
    let handle = ExternalPtr::new(Handle {
        state: Rc::new(RefCell::new(state)),
        identity: Cell::new(0),
    });
    let id = address(handle.as_robj())?;
    handle.identity.set(id);
    HANDLES.with(|h| {
        h.borrow_mut().insert(id, Rc::downgrade(&handle.state));
    });
    Ok(handle.into_robj())
}
fn resolve(handle: &Robj) -> std::result::Result<Rc<RefCell<State>>, String> {
    let id = address(handle)?;
    HANDLES
        .with(|h| h.borrow().get(&id).and_then(Weak::upgrade))
        .ok_or_else(|| "unknown domain result handle".into())
}
fn text(bytes: &Raw) -> std::result::Result<&str, String> {
    std::str::from_utf8(bytes.as_slice()).map_err(|_| "invalid path argument UTF-8".into())
}
fn environment(value: &Robj) -> std::result::Result<Option<Raw>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        value
            .as_raw()
            .map(Some)
            .ok_or_else(|| "environment must be raw UTF-8 or NULL".into())
    }
}
pub(crate) fn parse_capabilities(
    parsed: List,
) -> std::result::Result<(Function, Function, Function), String> {
    if parsed.len() != 3 {
        return Err("invalid project host capabilities".into());
    }
    let verify = parsed
        .elt(0)
        .map_err(|_| "missing lock capability")?
        .as_function()
        .ok_or("invalid lock capability")?;
    let resolve = parsed
        .elt(1)
        .map_err(|_| "missing binding capability")?
        .as_function()
        .ok_or("invalid binding capability")?;
    let formatter = parsed
        .elt(2)
        .map_err(|_| "missing condition capability")?
        .as_function()
        .ok_or("invalid condition capability")?;
    Ok((verify, resolve, formatter))
}

#[extendr]
fn domain_file(
    specification: Raw,
    environment_path: Robj,
    version: Raw,
    capabilities: List,
) -> List {
    domain_request(specification, environment_path, version, capabilities, None)
}
#[extendr]
fn domain_resolved(
    specification: Raw,
    environment_path: Robj,
    version: Raw,
    capabilities: List,
    resolver: Function,
) -> List {
    domain_request(
        specification,
        environment_path,
        version,
        capabilities,
        Some(crate::resource_resolver::new(resolver)),
    )
}
fn domain_request(
    specification: Raw,
    environment_path: Robj,
    version: Raw,
    capabilities: List,
    resolver: Option<Arc<dyn yamaa::ResourceResolver>>,
) -> List {
    boundary(|| {
        let specification_text = text(&specification)?.to_owned();
        let environment = environment(&environment_path)?;
        let environment_text = environment
            .as_ref()
            .map(text)
            .transpose()?
            .map(str::to_owned);
        let version = text(&version)?.to_owned();
        let (verify, resolve, formatter) = parse_capabilities(capabilities)?;
        let functions = Arc::new(crate::project_activation::Functions::new(
            verify, resolve, formatter,
        ));
        let mut options =
            EngineOptions::new(HostLanguage::R, "r".into(), version).with_functions(functions);
        if let Some(resolver) = resolver {
            options = options.with_resolver(resolver);
        }
        let mut request = Request::new(&specification_text);
        if let Some(environment) = environment_text.as_deref() {
            request = request.with_environment(environment);
        }
        let build = Engine::new(options)
            .build(&request, &CancellationToken::new())
            .map_err(|error| error.to_string())?;
        store(State { build })
    })
}
#[extendr]
fn check_file(specification: Raw, environment_path: Robj, version: Raw) -> List {
    check_request(specification, environment_path, version, None)
}
#[extendr]
fn check_resolved(
    specification: Raw,
    environment_path: Robj,
    version: Raw,
    resolver: Function,
) -> List {
    check_request(
        specification,
        environment_path,
        version,
        Some(crate::resource_resolver::new(resolver)),
    )
}
fn check_request(
    specification: Raw,
    environment_path: Robj,
    version: Raw,
    resolver: Option<Arc<dyn yamaa::ResourceResolver>>,
) -> List {
    boundary(|| {
        let specification_text = text(&specification)?.to_owned();
        let environment = environment(&environment_path)?;
        let environment_text = environment
            .as_ref()
            .map(text)
            .transpose()?
            .map(str::to_owned);
        let version = text(&version)?.to_owned();
        let mut options = EngineOptions::new(HostLanguage::R, "r".into(), version);
        if let Some(resolver) = resolver {
            options = options.with_resolver(resolver);
        }
        let mut request = Request::new(&specification_text);
        if let Some(environment) = environment_text.as_deref() {
            request = request.with_environment(environment);
        }
        let report = Engine::new(options).check(&request, &CancellationToken::new());
        // Both public views come from this one report, so they cannot disagree.
        Ok(list!(
            issues = crate::issue_frame::frame(&report.diagnostics)?,
            diagnostics = crate::diagnostic_records::records(&report.diagnostics)?
        )
        .into_robj())
    })
}
#[extendr]
fn domain_issues(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        crate::issue_frame::frame(&state.build.report().diagnostics)
    })
}
/// Named records from the same retained report as `domain_issues`.
#[extendr]
fn domain_diagnostics(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        crate::diagnostic_records::records(&state.build.report().diagnostics)
    })
}
#[extendr]
fn domain_output(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        match state.build.output() {
            Some(table) => crate::public_table::frame(table),
            None => Ok(NULL.into_robj()),
        }
    })
}
#[extendr]
fn domain_arrow(handle: Robj, destination: Robj, kind: i32) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        // Interim mapping until the binding is renamed: the verification slot
        // returns the run report and the retired warning slot returns nothing.
        let table = match kind {
            0 => state.build.output(),
            1 => state.build.run_report(),
            2 => None,
            _ => return Err("unknown Arrow table selection".into()),
        };
        table.map_or_else(
            || Ok(NULL.into_robj()),
            |table| crate::arrow_bridge::export(table, destination),
        )
    })
}
#[extendr]
fn domain_observations(handle: Robj, include_measurements: bool) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        let observed = state.build.observations(include_measurements);
        Ok(observed.map_or_else(
            || NULL.into_robj(),
            |value| Raw::from_bytes(value.as_bytes()).into_robj(),
        ))
    })
}
#[extendr]
fn domain_log(handle: Robj, warning: bool) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        // Interim mapping until the binding is renamed: the verification slot
        // returns the run report and the retired warning slot returns nothing.
        match state.build.run_report().filter(|_| !warning) {
            Some(table) => crate::public_table::frame(table),
            None => Ok(NULL.into_robj()),
        }
    })
}
#[extendr]
fn domain_save(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let mut state = state
            .try_borrow_mut()
            .map_err(|_| "domain result already borrowed")?;
        let saved = match state.build.save() {
            Ok(SaveOutcome::Saved) => (true, false),
            Ok(_) => (false, false),
            Err(_) => (false, true),
        };
        Ok(list!(saved = saved.0, failed = saved.1).into_robj())
    })
}
#[extendr]
fn domain_interrupt(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        let interrupted = state.build.interrupt().and_then(|error| match error {
            yamaa::FunctionError::Interrupted { payload } => payload
                .as_ref()?
                .with::<crate::thread_bound::ThreadBound<Robj>, _>(|condition| {
                    condition.get().cloned().ok()
                })
                .flatten(),
            _ => None,
        });
        Ok(interrupted.unwrap_or_else(|| r!(NULL)))
    })
}
/// Private installed evidence consults the exact held public-domain owner.
#[extendr]
fn domain_producer_evidence(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        let build = &state.build;
        let Some(nodes) = build.producer_nodes() else {
            return Ok(r!(NULL));
        };
        let failures = build
            .producer_failures()
            .into_iter()
            .flatten()
            .map(|failure| {
                list!(
                    stage = failure.stage,
                    facts = crate::project_activation::facts(&failure.error)
                )
            })
            .collect::<Vec<_>>();
        Ok(list!(
            accepted = build.producer_accepted().unwrap_or(false),
            entered = List::from_values(nodes.iter().map(|node| r!(*node as f64))),
            failures = List::from_values(failures),
            sources = List::from_values(
                build
                    .producer_sources()
                    .into_iter()
                    .flatten()
                    .map(|(name, bytes)| list!(
                        name = Raw::from_bytes(name.as_bytes()),
                        bytes = Raw::from_bytes(bytes)
                    ))
                    .collect::<Vec<_>>()
            )
        )
        .into_robj())
    })
}
#[extendr]
fn domain_preparation_sources(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        Ok(List::from_values(
            state
                .build
                .preparation_sources()
                .iter()
                .map(|(name, bytes)| {
                    list!(
                        name = Raw::from_bytes(name.as_bytes()),
                        bytes = Raw::from_bytes(bytes)
                    )
                }),
        )
        .into_robj())
    })
}

#[extendr]
fn domain_projection_failure(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        match state.build.projection_failure() {
            Some(value) => Ok(r!(value)),
            None => Ok(r!(NULL)),
        }
    })
}
include!("generated/domain_entry.rs");
