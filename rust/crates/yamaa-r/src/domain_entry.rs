//! Registered owned public result handles; the engine owns file orchestration.
use crate::specification_service::{address, boundary};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};
use yamaa_adapters::issue_rows::Issue;

struct State {
    #[cfg(unix)]
    result: Option<yamaa_adapters::file_application::Domain>,
    #[cfg(unix)]
    project: Option<Rc<crate::file_project::platform::HeldAttempt>>,
    #[cfg(unix)]
    _preparation: Option<yamaa_adapters::project_application::Error>,
    issues: Vec<Issue>,
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
#[cfg(not(unix))]
fn unavailable() -> Vec<Issue> {
    vec![Issue::from_core(yamaa_core::application_issue::unsupported(
        "native_file_transport",
        None,
    ))
    .expect("application issue")]
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
        #[cfg(unix)]
        {
            if let Some(environment) = environment {
                let mut project = match yamaa_adapters::project_application::prepare(
                    specification,
                    environment,
                    yamaa_core::project_function::Language::R,
                ) {
                    Ok(project) => project,
                    Err(error) => {
                        return store(State {
                            result: None,
                            project: None,
                            issues: yamaa_adapters::project_application::failure(
                                &error,
                                yamaa_core::project_function::Language::R,
                            ),
                            _preparation: Some(error),
                        })
                    }
                };
                if capabilities.len() != 3 {
                    return Err("invalid project host capabilities".into());
                }
                let verify = capabilities
                    .elt(0)
                    .map_err(|_| "missing lock capability")?
                    .as_function()
                    .ok_or("invalid lock capability")?;
                let resolve = capabilities
                    .elt(1)
                    .map_err(|_| "missing binding capability")?
                    .as_function()
                    .ok_or("invalid binding capability")?;
                let formatter = capabilities
                    .elt(2)
                    .map_err(|_| "missing condition capability")?
                    .as_function()
                    .ok_or("invalid condition capability")?;
                let inner = project.build_with(|run| {
                    crate::project_activation::Port::new(
                        run.captured_environment()
                            .lock()
                            .map_or(&[], |lock| &lock.source.bytes),
                        verify,
                        resolve,
                    )
                });
                let attempt = Rc::new(crate::file_project::platform::HeldAttempt {
                    inner,
                    run: project.retained_run(),
                });
                let fields =
                    [version, yamaa_core::VERSION, "domain", specification, "."].map(str::to_owned);
                let formatted = crate::project_result::platform::format(
                    Rc::clone(&attempt),
                    &fields,
                    &formatter,
                );
                let result = match formatted.formatted {
                    Some(result) => {
                        yamaa_adapters::file_application::Domain::from_project(result, &project)
                    }
                    None => Err(yamaa_adapters::file_application::rejected(
                        "build",
                        formatted.refusal,
                    )),
                };
                return store(match result {
                    Ok(result) => State {
                        result: Some(result),
                        project: Some(attempt),
                        _preparation: None,
                        issues: Vec::new(),
                    },
                    Err(issues) => State {
                        result: None,
                        project: Some(attempt),
                        _preparation: None,
                        issues,
                    },
                });
            }
            let result = yamaa_adapters::file_application::domain(
                yamaa_engine::domain_entry::Request {
                    specification,
                    environment,
                },
                yamaa_adapters::specification_report::Identity {
                    runtime: "r",
                    runtime_version: version,
                    engine_version: yamaa_core::VERSION,
                    example: "domain",
                    specification,
                    base_directory: ".",
                },
            );
            store(match result {
                Ok(result) => State {
                    result: Some(result),
                    project: None,
                    _preparation: None,
                    issues: Vec::new(),
                },
                Err(issues) => State {
                    result: None,
                    project: None,
                    _preparation: None,
                    issues,
                },
            })
        }
        #[cfg(not(unix))]
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
        #[cfg(unix)]
        {
            if let Some(environment) = environment {
                return crate::issue_frame::frame(&yamaa_adapters::project_application::check(
                    specification,
                    environment,
                    yamaa_core::project_function::Language::R,
                ));
            }
            let issues = yamaa_adapters::file_application::check(
                yamaa_engine::domain_entry::Request {
                    specification,
                    environment,
                },
                yamaa_adapters::specification_report::Identity {
                    runtime: "r",
                    runtime_version: version,
                    engine_version: yamaa_core::VERSION,
                    example: "check",
                    specification,
                    base_directory: ".",
                },
            );
            crate::issue_frame::frame(&issues)
        }
        #[cfg(not(unix))]
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
        #[cfg(unix)]
        if let Some(result) = &state.result {
            return crate::issue_frame::frame(&result.issues());
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
        #[cfg(unix)]
        if let Some(table) = state
            .result
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
        #[cfg(unix)]
        if let Some(result) = &state.result {
            return Ok(Raw::from_bytes(result.observations().to_string().as_bytes()).into_robj());
        }
        let _ = state;
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
        #[cfg(unix)]
        if let Some(result) = &mut state.result {
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
        #[cfg(unix)]
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
    fn domain_observations;
    fn domain_save;
    fn domain_interrupt;
}
