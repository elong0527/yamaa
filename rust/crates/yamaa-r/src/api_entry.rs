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

/// The three installed R capabilities project functions need: lock
/// verification, binding resolution and condition rendering.
fn parse_capabilities(
    parsed: List,
) -> std::result::Result<(Function, Function, Function), Refusal> {
    if parsed.len() != 3 {
        return Err("invalid project host capabilities".into());
    }
    let capability = |index: usize, missing: &str, invalid: &str| {
        parsed
            .elt(index)
            .map_err(|_| Refusal::from(missing))?
            .as_function()
            .ok_or_else(|| Refusal::from(invalid))
    };
    Ok((
        capability(0, "missing lock capability", "invalid lock capability")?,
        capability(
            1,
            "missing binding capability",
            "invalid binding capability",
        )?,
        capability(
            2,
            "missing condition capability",
            "invalid condition capability",
        )?,
    ))
}

fn engine(version: &Raw, capabilities: Option<List>) -> std::result::Result<Engine, Refusal> {
    let mut options = EngineOptions::new(HostLanguage::R, "r".into(), text(version)?);
    if let Some(capabilities) = capabilities {
        let (verify, resolve, formatter) = parse_capabilities(capabilities)?;
        options = options.with_functions(Arc::new(crate::project_activation::Functions::new(
            verify, resolve, formatter,
        )));
    }
    Ok(Engine::new(options))
}

/// One registered owner per R external pointer: dropping the pointer drops
/// the owner, and lookups go through the registry, never the pointee.
macro_rules! registry {
    ($registry:ident, $handle:ident, $store:ident, $lookup:ident, $kind:ty, $what:literal) => {
        thread_local! {
            static $registry: RefCell<BTreeMap<usize, Weak<RefCell<$kind>>>> =
                const { RefCell::new(BTreeMap::new()) };
        }

        struct $handle {
            value: Rc<RefCell<$kind>>,
            identity: Cell<usize>,
        }

        impl Drop for $handle {
            fn drop(&mut self) {
                $registry.with(|registry| {
                    registry.borrow_mut().remove(&self.identity.get());
                });
            }
        }

        fn $store(value: $kind) -> std::result::Result<Robj, Refusal> {
            let handle = ExternalPtr::new($handle {
                value: Rc::new(RefCell::new(value)),
                identity: Cell::new(0),
            });
            let identity = address(handle.as_robj())?;
            handle.identity.set(identity);
            $registry.with(|registry| {
                registry
                    .borrow_mut()
                    .insert(identity, Rc::downgrade(&handle.value));
            });
            Ok(handle.into_robj())
        }

        fn $lookup(handle: &Robj) -> std::result::Result<Rc<RefCell<$kind>>, Refusal> {
            let identity = address(handle)?;
            $registry
                .with(|registry| registry.borrow().get(&identity).and_then(Weak::upgrade))
                .ok_or_else(|| concat!("unknown ", $what, " handle").into())
        }
    };
}

registry!(RUNS, RunHandle, store, run, yamaa::Run, "run");
registry!(
    PROJECT_RUNS,
    ProjectRunHandle,
    store_project_run,
    project_run_of,
    yamaa::ProjectRun,
    "project run"
);
registry!(
    SUBMISSIONS,
    SubmissionHandle,
    store_submission,
    submission_of,
    yamaa::Submission,
    "submission"
);
registry!(
    PLANS,
    PlansHandle,
    store_plans,
    plans_of,
    yamaa::Plans,
    "plans"
);

/// The original interrupt condition a project function raised, or `None`.
fn interrupt(build: &yamaa::Build) -> Option<Robj> {
    build.interrupt().and_then(|error| match error {
        yamaa::FunctionError::Interrupted { payload } => payload
            .as_ref()?
            .with::<crate::thread_bound::ThreadBound<Robj>, _>(|condition| {
                condition.get().cloned().ok()
            })
            .flatten(),
        _ => None,
    })
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
        let issues = engine(&version, None)?
            .spec_check(
                &text(&specification)?,
                project.as_deref(),
                &CancellationToken::new(),
            )
            .map_err(|error| from_error("cannot check the specification", &error))?;
        Ok(crate::public_table::frame(&issues)?)
    })
}

/// The holder for the compiled plans one lazy object keeps between its runs
/// (#2821): `handle` while it is live, else a new empty one. A lazy object
/// passes `NULL` before its first run; a handle restored by `readRDS`, an
/// `.RData` file or another R process has expired, so the restored object
/// starts afresh (#3069). Creating a holder reads nothing.
#[extendr]
fn plans(handle: Robj) -> List {
    reply(|| {
        if !handle.is_null() && plans_of(&handle).is_ok() {
            return Ok(handle);
        }
        store_plans(yamaa::Plans::new())
    })
}

/// The holder a lazy object passes, or `None` for a one-shot run. Nothing
/// borrows a holder mutably, so a run started from a project function shares
/// it too.
fn held_plans(plans: &Robj) -> std::result::Result<Option<Rc<RefCell<yamaa::Plans>>>, Refusal> {
    if plans.is_null() {
        return Ok(None);
    }
    plans_of(plans).map(Some)
}

#[extendr]
fn spec_run(
    specification: Raw,
    project: Robj,
    version: Raw,
    capabilities: List,
    plans: Robj,
) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let engine = engine(&version, Some(capabilities))?;
        let specification = text(&specification)?;
        let token = CancellationToken::new();
        let run = match held_plans(&plans)? {
            Some(plans) => {
                engine.spec_run_reusing(&specification, project.as_deref(), &plans.borrow(), &token)
            }
            None => engine.spec_run(&specification, project.as_deref(), &token),
        }
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
        Ok(crate::context_values::context_value(&value)?)
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
        Ok(interrupt(run.build()).unwrap_or_else(|| r!(NULL)))
    })
}

#[extendr]
fn project_spec(project: Raw, name: Raw, version: Raw) -> List {
    reply(|| {
        let path = engine(&version, None)?
            .project_spec(&text(&project)?, &text(&name)?)
            .map_err(Refusal::Failure)?;
        Ok(r!(path.as_str()))
    })
}

#[extendr]
fn project_check(project: Raw, version: Raw) -> List {
    reply(|| {
        let issues = engine(&version, None)?
            .project_check(&text(&project)?, &CancellationToken::new())
            .map_err(|error| from_error("cannot check the project", &error))?;
        Ok(crate::public_table::frame(&issues)?)
    })
}

#[extendr]
fn project_run(project: Raw, section: Robj, version: Raw, capabilities: List, plans: Robj) -> List {
    reply(|| {
        let section = optional_text(&section)?;
        let engine = engine(&version, Some(capabilities))?;
        let project = text(&project)?;
        let token = CancellationToken::new();
        let runs = match held_plans(&plans)? {
            Some(plans) => {
                engine.project_run_reusing(&project, section.as_deref(), &plans.borrow(), &token)
            }
            None => engine.project_run(&project, section.as_deref(), &token),
        }
        .map_err(|error| from_error("cannot run the project", &error))?;
        store_project_run(runs)
    })
}

#[extendr]
fn project_run_names(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        Ok(Strings::from_values(runs.names()).into_robj())
    })
}

#[extendr]
fn project_run_output(handle: Robj, name: Raw) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        let run = runs.member(&text(&name)?).map_err(Refusal::Failure)?;
        table(run.output())
    })
}

#[extendr]
fn project_run_member_issues(handle: Robj, name: Raw) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        let run = runs.member(&text(&name)?).map_err(Refusal::Failure)?;
        Ok(crate::public_table::frame(&run.issues())?)
    })
}

#[extendr]
fn project_run_member_report(handle: Robj, name: Raw) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        let run = runs.member(&text(&name)?).map_err(Refusal::Failure)?;
        table(run.report())
    })
}

#[extendr]
fn project_run_issues(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        Ok(crate::public_table::frame(&runs.issues())?)
    })
}

#[extendr]
fn project_run_report(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        Ok(crate::public_table::frame(&runs.report())?)
    })
}

#[extendr]
fn project_run_save(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let mut runs = runs
            .try_borrow_mut()
            .map_err(|_| "project run handle in use")?;
        runs.save().map_err(Refusal::Failure)?;
        Ok(NULL.into_robj())
    })
}

/// The first original interrupt condition any specification of the project
/// run raised, or `NULL`.
#[extendr]
fn project_run_interrupt(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        let interrupted = runs
            .names()
            .iter()
            .filter_map(|name| runs.get(name))
            .find_map(|run| interrupt(run.build()));
        Ok(interrupted.unwrap_or_else(|| r!(NULL)))
    })
}

#[extendr]
fn project_submission(project: Raw, section: Robj, created: Robj, version: Raw) -> List {
    reply(|| {
        let section = optional_text(&section)?;
        let created = optional_text(&created)?;
        let submission = engine(&version, None)?
            .project_submission(
                &text(&project)?,
                section.as_deref(),
                created.as_deref(),
                yamaa::VERSION,
                &CancellationToken::new(),
            )
            .map_err(|error| from_error("cannot compose the submission", &error))?;
        store_submission(submission)
    })
}

#[extendr]
fn submission_issues(handle: Robj) -> List {
    reply(|| {
        let submission = submission_of(&handle)?;
        let submission = submission
            .try_borrow()
            .map_err(|_| "submission handle in use")?;
        Ok(crate::public_table::frame(&submission.issues())?)
    })
}

#[extendr]
fn submission_save(handle: Robj) -> List {
    reply(|| {
        let submission = submission_of(&handle)?;
        let mut submission = submission
            .try_borrow_mut()
            .map_err(|_| "submission handle in use")?;
        submission.save().map_err(Refusal::Failure)?;
        Ok(NULL.into_robj())
    })
}

include!("generated/api_entry.rs");
