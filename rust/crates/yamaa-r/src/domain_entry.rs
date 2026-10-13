//! Registered owned public result handles; the engine owns file orchestration.
use crate::specification_service::{address, boundary};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};
use yamaa_adapters::issue_rows::Issue;

pub(crate) struct State {
    #[cfg(any(unix, windows))]
    pub(crate) result: Option<yamaa_adapters::file_application::Domain>,
    #[cfg(any(unix, windows))]
    pub(crate) project: Option<Rc<crate::file_project::platform::HeldAttempt>>,
    #[cfg(any(unix, windows))]
    pub(crate) _preparation: Option<yamaa_adapters::file_workflow::SelectionError>,
    #[cfg(any(unix, windows))]
    pub(crate) producer: Option<crate::file_producer::platform::PublicResult>,
    pub(crate) issues: Vec<Issue>,
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
    // The R facade retains at most 65537 Unicode characters. Let the shared
    // application classify an oversized path as an issue before file access.
    if bytes.len() > 65537 * 4 {
        return Err("path argument byte limit".into());
    }
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
#[cfg(not(any(unix, windows)))]
fn unavailable() -> Vec<Issue> {
    vec![
        Issue::from_core(yamaa_engine::host_values::unavailable_file_transport())
            .expect("application issue"),
    ]
}
#[extendr]
fn domain_file(
    specification: Raw,
    environment_path: Robj,
    version: Raw,
    capabilities: List,
) -> List {
    boundary(|| {
        let specification = text(&specification)?;
        let environment = environment(&environment_path)?;
        let environment = environment.as_ref().map(text).transpose()?;
        let version = text(&version)?;
        #[cfg(any(unix, windows))]
        {
            let runtime = crate::file_domain::Runtime::new(specification, version, capabilities)?;
            let mut ports = yamaa_adapters::file_application::Application {
                host: yamaa_engine::host_values::Language::R,
                runtime,
            };
            let state = match yamaa_engine::domain_entry::domain(
                yamaa_engine::domain_entry::Request {
                    specification,
                    environment,
                },
                &mut ports,
            ) {
                Ok(state) => state,
                Err(yamaa_adapters::file_application::ApplicationError::Runtime(error)) => {
                    return Err(error)
                }
                Err(yamaa_adapters::file_application::ApplicationError::Preparation(error)) => {
                    State {
                        producer: None,
                        result: None,
                        project: None,
                        issues: yamaa_adapters::project_application::selection_failure(
                            &error, ports.host,
                        ),
                        _preparation: Some(error),
                    }
                }
            };
            store(state)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (specification, environment, version, capabilities);
            store(State {
                issues: unavailable(),
            })
        }
    })
}
#[extendr]
fn check_file(specification: Raw, environment_path: Robj, version: Raw) -> List {
    boundary(|| {
        let specification = text(&specification)?;
        let environment = environment(&environment_path)?;
        let environment = environment.as_ref().map(text).transpose()?;
        let version = text(&version)?;
        #[cfg(any(unix, windows))]
        {
            let issues = yamaa_adapters::file_application::check(
                yamaa_engine::domain_entry::Request {
                    specification,
                    environment,
                },
                yamaa_adapters::specification_report::Identity {
                    runtime: "r",
                    runtime_version: version,
                    engine_version: yamaa_engine::host_values::VERSION,
                    example: "check",
                    specification,
                    base_directory: ".",
                },
            );
            crate::issue_frame::frame(&issues)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (specification, environment, version);
            crate::issue_frame::frame(&unavailable())
        }
    })
}
#[extendr]
fn domain_issues(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(result) = &state.result {
            return crate::issue_frame::frame(&result.issues());
        }
        #[cfg(any(unix, windows))]
        if let Some(producer) = &state.producer {
            return crate::issue_frame::frame(&producer.issues());
        }
        crate::issue_frame::frame(&state.issues)
    })
}
#[extendr]
fn domain_output(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(table) = state
            .result
            .as_ref()
            .and_then(|result| result.output_table())
        {
            return crate::public_table::frame(table);
        }
        #[cfg(any(unix, windows))]
        if let Some(table) = state
            .producer
            .as_ref()
            .and_then(|result| result.output_table())
        {
            return crate::public_table::frame(table);
        }
        let _ = state;
        Ok(NULL.into_robj())
    })
}
#[extendr]
fn domain_observations(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(result) = &state.result {
            return Ok(Raw::from_bytes(result.observations_json().as_bytes()).into_robj());
        }
        #[cfg(any(unix, windows))]
        if let Some(value) = state
            .producer
            .as_ref()
            .and_then(|result| result.observations())
        {
            return Ok(Raw::from_bytes(value.as_bytes()).into_robj());
        }
        let _ = state;
        Ok(NULL.into_robj())
    })
}
#[extendr]
fn domain_log(handle: Robj, warning: bool) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(table) = state
            .result
            .as_ref()
            .and_then(|result| result.log_table(warning))
        {
            return crate::public_table::frame(table);
        }
        #[cfg(any(unix, windows))]
        if let Some(table) = state
            .producer
            .as_ref()
            .and_then(|result| result.log_table(warning))
        {
            return crate::public_table::frame(table);
        }
        let _ = (state, warning);
        Ok(NULL.into_robj())
    })
}
#[extendr]
fn domain_save(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let mut state = state
            .try_borrow_mut()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(result) = &mut state.result {
            return Ok(match result.save() {
                Ok(saved) => list!(saved = saved, failed = false),
                Err(_) => list!(saved = false, failed = true),
            }
            .into_robj());
        }
        #[cfg(any(unix, windows))]
        if let Some(result) = &mut state.producer {
            return Ok(match result.save() {
                Ok(saved) => list!(saved = saved, failed = false),
                Err(_) => list!(saved = false, failed = true),
            }
            .into_robj());
        }
        let _ = &mut state;
        Ok(list!(saved = false, failed = true).into_robj())
    })
}
#[extendr]
fn domain_interrupt(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(attempt) = &state.project {
            let mut interrupted = None;
            yamaa_adapters::project_attempt::visit_host_failures(&attempt.inner, |_, error| {
                if let crate::project_activation::Error::Interrupt(condition) = error {
                    if interrupted.is_none() {
                        interrupted = Some(condition.clone());
                    }
                }
            });
            return Ok(interrupted.unwrap_or_else(|| r!(NULL)));
        }
        #[cfg(any(unix, windows))]
        if let Some(producer) = &state.producer {
            return producer.interrupt();
        }
        let _ = state;
        Ok(r!(NULL))
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
        #[cfg(any(unix, windows))]
        if let Some(producer) = &state.producer {
            return producer.evidence();
        }
        let _ = state;
        Ok(r!(NULL))
    })
}
#[extendr]
fn domain_preparation_sources(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(error) = &state._preparation {
            let sources = error
                .retained_sources(
                    yamaa_adapters::host_limits::RECORDS,
                    yamaa_adapters::host_limits::REPLY_BYTES,
                )
                .map_err(|_| "source evidence limit; original preparation remains retained")?;
            let mut remaining = yamaa_adapters::host_limits::SOURCE_BYTES;
            for (_, bytes) in &sources {
                remaining = remaining
                    .checked_sub(bytes.len())
                    .ok_or("source evidence limit; original preparation remains retained")?;
            }
            return Ok(List::from_values(sources.iter().map(|(name, bytes)| {
                list!(
                    name = Raw::from_bytes(name.as_bytes()),
                    bytes = Raw::from_bytes(bytes)
                )
            }))
            .into_robj());
        }
        let _ = state;
        Ok(List::new(0).into_robj())
    })
}

#[extendr]
fn domain_projection_failure(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "domain result already borrowed")?;
        #[cfg(any(unix, windows))]
        if let Some(value) = state
            .producer
            .as_ref()
            .and_then(|producer| producer.projection_failure())
        {
            return Ok(r!(value));
        }
        let _ = state;
        Ok(r!(NULL))
    })
}
extendr_module! {
    mod domain_entry;
    fn domain_file;
    fn check_file;
    fn domain_issues;
    fn domain_output;
    fn domain_log;
    fn domain_observations;
    fn domain_save;
    fn domain_interrupt;
    fn domain_producer_evidence;
    fn domain_preparation_sources;
    fn domain_projection_failure;
}
