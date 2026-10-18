//! Native half of the R host API: specification operations, their results and
//! one error shape. The R facade keeps paths in lazy objects and calls these
//! functions; behavior, validation and messages stay in Rust.
//!
//! Every function replies `list(value, error)`. `error` is `NULL` on success,
//! otherwise `list(message, issues)`, where `issues` is the issues data frame
//! (empty when the refusal carries none).
use crate::handle::address;
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::{Rc, Weak},
    sync::Arc,
};
use yamaa::{CancellationToken, Engine, EngineOptions, Error, Failure, HostLanguage, Report};

/// Why a call could not return its value.
enum Refusal {
    Failure(Failure),
    Message(String),
}

impl From<String> for Refusal {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for Refusal {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

fn refused(error: Refusal) -> List {
    let failure = match error {
        Refusal::Failure(failure) => failure,
        Refusal::Message(message) => Failure::new(message, &Report::default()),
    };
    match crate::public_table::frame(&failure.issues) {
        Ok(issues) => list!(
            value = NULL,
            error = list!(message = failure.message, issues = issues)
        ),
        Err(conversion) => list!(
            value = NULL,
            error = list!(
                message = format!("{}; {conversion}", failure.message),
                issues = NULL
            )
        ),
    }
}

fn reply(run: impl FnOnce() -> std::result::Result<Robj, Refusal>) -> List {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(value)) => list!(value = value, error = NULL),
        Ok(Err(error)) => refused(error),
        Err(_) => refused("internal host boundary failure".into()),
    }
}

fn from_error(context: &str, error: &Error) -> Refusal {
    Refusal::Failure(Failure::from_error(context, error))
}

fn text(bytes: &Raw) -> std::result::Result<String, Refusal> {
    std::str::from_utf8(bytes.as_slice())
        .map(str::to_owned)
        .map_err(|_| "invalid path argument UTF-8".into())
}

fn optional_text(value: &Robj) -> std::result::Result<Option<String>, Refusal> {
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_raw()
        .ok_or_else(|| Refusal::from("expected raw UTF-8 or NULL"))?;
    text(&raw).map(Some)
}

fn engine(version: &Raw, capabilities: Option<List>) -> std::result::Result<Engine, Refusal> {
    let mut options = EngineOptions::new(HostLanguage::R, "r".into(), text(version)?);
    if let Some(capabilities) = capabilities {
        let (verify, resolve, formatter) = crate::domain_entry::parse_capabilities(capabilities)?;
        options = options.with_functions(Arc::new(crate::project_activation::Functions::new(
            verify, resolve, formatter,
        )));
    }
    Ok(Engine::new(options))
}

thread_local! {
    static RUNS: RefCell<BTreeMap<usize, Weak<RefCell<yamaa::Run>>>> =
        const { RefCell::new(BTreeMap::new()) };
}

struct Handle {
    run: Rc<RefCell<yamaa::Run>>,
    identity: Cell<usize>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        RUNS.with(|runs| {
            runs.borrow_mut().remove(&self.identity.get());
        });
    }
}

fn store(run: yamaa::Run) -> std::result::Result<Robj, Refusal> {
    let handle = ExternalPtr::new(Handle {
        run: Rc::new(RefCell::new(run)),
        identity: Cell::new(0),
    });
    let identity = address(handle.as_robj())?;
    handle.identity.set(identity);
    RUNS.with(|runs| {
        runs.borrow_mut()
            .insert(identity, Rc::downgrade(&handle.run));
    });
    Ok(handle.into_robj())
}

fn run(handle: &Robj) -> std::result::Result<Rc<RefCell<yamaa::Run>>, Refusal> {
    let identity = address(handle)?;
    RUNS.with(|runs| runs.borrow().get(&identity).and_then(Weak::upgrade))
        .ok_or_else(|| "unknown run handle".into())
}

fn table(table: Option<&yamaa::Table>) -> std::result::Result<Robj, Refusal> {
    match table {
        Some(table) => Ok(crate::public_table::frame(table)?),
        None => Ok(NULL.into_robj()),
    }
}

#[extendr]
fn spec_check(specification: Raw, project: Robj, version: Raw) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let issues = engine(&version, None)?.spec_check(
            &text(&specification)?,
            project.as_deref(),
            &CancellationToken::new(),
        );
        Ok(crate::public_table::frame(&issues)?)
    })
}

#[extendr]
fn spec_run(specification: Raw, project: Robj, version: Raw, capabilities: List) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let run = engine(&version, Some(capabilities))?
            .spec_run(
                &text(&specification)?,
                project.as_deref(),
                &CancellationToken::new(),
            )
            .map_err(|error| from_error("cannot run the specification", &error))?;
        store(run)
    })
}

#[extendr]
fn spec_resolve(specification: Raw, project: Robj, version: Raw) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let value = engine(&version, None)?
            .spec_resolve(&text(&specification)?, project.as_deref())
            .map_err(Refusal::Failure)?;
        Ok(crate::diagnostic_records::context_value(&value)?)
    })
}

#[extendr]
fn read_odm(
    source: Raw,
    archive_member: Robj,
    max_archive_members: usize,
    max_expanded_bytes: usize,
    max_xml_depth: usize,
) -> List {
    reply(|| {
        let mut options = yamaa::OdmOptions::new()
            .with_max_archive_members(max_archive_members)
            .with_max_expanded_bytes(u64::try_from(max_expanded_bytes).unwrap_or(u64::MAX))
            .with_max_xml_depth(max_xml_depth);
        if let Some(member) = optional_text(&archive_member)? {
            options = options.with_archive_member(member);
        }
        let items = yamaa::read_odm(std::path::Path::new(&text(&source)?), &options)
            .map_err(|error| from_error("cannot read ODM", &error))?;
        Ok(crate::public_table::frame(&items)?)
    })
}

#[extendr]
fn run_output(handle: Robj) -> List {
    reply(|| {
        let run = run(&handle)?;
        let run = run.try_borrow().map_err(|_| "run handle in use")?;
        table(run.output())
    })
}

#[extendr]
fn run_issues(handle: Robj) -> List {
    reply(|| {
        let run = run(&handle)?;
        let run = run.try_borrow().map_err(|_| "run handle in use")?;
        Ok(crate::public_table::frame(&run.issues())?)
    })
}

#[extendr]
fn run_report(handle: Robj) -> List {
    reply(|| {
        let run = run(&handle)?;
        let run = run.try_borrow().map_err(|_| "run handle in use")?;
        table(run.report())
    })
}

#[extendr]
fn run_save(handle: Robj) -> List {
    reply(|| {
        let run = run(&handle)?;
        let mut run = run.try_borrow_mut().map_err(|_| "run handle in use")?;
        run.save().map_err(Refusal::Failure)?;
        Ok(NULL.into_robj())
    })
}

/// Private installed-run evidence for qualification suites.
#[extendr]
fn run_observations(handle: Robj, include_measurements: bool) -> List {
    reply(|| {
        let run = run(&handle)?;
        let run = run.try_borrow().map_err(|_| "run handle in use")?;
        Ok(run
            .build()
            .observations(include_measurements)
            .map_or_else(|| NULL.into_robj(), |value| r!(value.as_str())))
    })
}

/// The original interrupt condition a project function raised, or `NULL`.
#[extendr]
fn run_interrupt(handle: Robj) -> List {
    reply(|| {
        let run = run(&handle)?;
        let run = run.try_borrow().map_err(|_| "run handle in use")?;
        let interrupted = run.build().interrupt().and_then(|error| match error {
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

include!("generated/api_entry.rs");
