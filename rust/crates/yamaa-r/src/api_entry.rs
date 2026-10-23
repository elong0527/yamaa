//! Native half of the R host API: specification operations, their results and
//! one error shape. The R facade keeps paths in lazy objects and calls these
//! functions; behavior, validation and messages stay in Rust.
//!
//! Every function replies `list(value, error)`. `error` is `NULL` on success,
//! otherwise `list(message, issues, paths)`, where `issues` is the issues data
//! frame (empty when the refusal carries none) and `paths` the files a refused
//! save wrote before it stopped. Every function replies behind the interface
//! gate, so it refuses until the handshake succeeds (`crate::interface`).
use crate::handle::address;
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::{Rc, Weak},
    sync::Arc,
};
use yamaa::{Action, Engine, EngineOptions, Error, Failure, HostLanguage, Report};
use yamaa_boundary::{HandleKind, Live};

/// Why a call could not return its value.
pub(crate) enum Refusal {
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
    // A refusal of this boundary itself carries an empty issues table and
    // wrote nothing.
    let (message, issues, paths) = match error {
        Refusal::Failure(failure) => (failure.message, failure.issues, failure.paths),
        Refusal::Message(message) => (message, Report::default().table(), Vec::new()),
    };
    let paths = Strings::from_values(&paths);
    match crate::public_table::frame(&issues) {
        Ok(issues) => list!(
            value = NULL,
            error = list!(message = message, issues = issues, paths = paths)
        ),
        Err(conversion) => list!(
            value = NULL,
            error = list!(
                message = format!("{message}; {conversion}"),
                issues = NULL,
                paths = paths
            )
        ),
    }
}

/// Reply with `run`'s value or refusal, behind the interface gate: until the
/// handshake succeeds, the reply refuses with no issues.
pub(crate) fn reply(run: impl FnOnce() -> std::result::Result<Robj, Refusal>) -> List {
    answer(|| {
        crate::interface::gate()?;
        run()
    })
}

/// Reply with `run`'s value or refusal, containing a panic and keeping an
/// interrupt the call observed. Only the handshake replies without the gate.
pub(crate) fn answer(run: impl FnOnce() -> std::result::Result<Robj, Refusal>) -> List {
    let scope = crate::interrupt::Scope::new();
    let result = catch_unwind(AssertUnwindSafe(run));
    if let Some(condition) = scope.take() {
        return list!(value = NULL, error = list!(condition = condition));
    }
    match result {
        Ok(Ok(value)) => list!(value = value, error = NULL),
        Ok(Err(error)) => refused(error),
        Err(_) => refused("internal host boundary failure".into()),
    }
}

/// The refusal of `action` for `error` (D39).
fn from_error(action: Action<'_>, error: &Error) -> Refusal {
    Refusal::Failure(Failure::from_error(action, error))
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
/// the owner, and lookups go through the registry, never the pointee. Each
/// owner counts in the census under `$census` while it lives.
macro_rules! registry {
    ($registry:ident, $handle:ident, $store:ident, $lookup:ident, $kind:ty, $what:literal, $census:expr) => {
        thread_local! {
            static $registry: RefCell<BTreeMap<usize, Weak<RefCell<$kind>>>> =
                const { RefCell::new(BTreeMap::new()) };
        }

        struct $handle {
            value: Rc<RefCell<$kind>>,
            identity: Cell<usize>,
            _live: Live,
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
                _live: crate::interface::CENSUS.hold($census),
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

/// A run built whole, or a streamed run (#3697), whose result is known once
/// its output's stream ends.
enum RunHeld {
    Built(Box<yamaa::Run>),
    Streamed(yamaa::Streamed),
}

registry!(RUNS, RunHandle, store, run, RunHeld, "run", HandleKind::Run);

/// A streamed run's refusal of a reader or of its result: D39's text of
/// reading the run, such as `cannot read DM: operation cancelled`, with no
/// issues, as the binding's other boundary refusals.
fn stream_refusal(streamed: &yamaa::Streamed, error: &Error) -> Refusal {
    Refusal::Message(Failure::from_error(Action::Read(streamed.subject()), error).message)
}

/// Read the run's result with `read`. A streamed run nobody started to read
/// runs whole first, polling the native call's interrupts; one whose stream
/// has not ended, or that was cancelled, refuses.
fn read_run<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::Run) -> std::result::Result<T, Refusal>,
) -> std::result::Result<T, Refusal> {
    let held = run(handle)?;
    let held = held.try_borrow().map_err(|_| "run handle in use")?;
    match &*held {
        RunHeld::Built(run) => read(run),
        RunHeld::Streamed(streamed) => read(
            &*streamed
                .run()
                .map_err(|error| stream_refusal(streamed, &error))?,
        ),
    }
}
registry!(
    CHECKS,
    CheckHandle,
    store_check,
    check_of,
    yamaa::Check,
    "check",
    HandleKind::Check
);
registry!(
    PROJECT_RUNS,
    ProjectRunHandle,
    store_project_run,
    project_run_of,
    yamaa::ProjectRun,
    "project run",
    HandleKind::ProjectRun
);
registry!(
    SUBMISSIONS,
    SubmissionHandle,
    store_submission,
    submission_of,
    yamaa::Submission,
    "submission",
    HandleKind::Submission
);
registry!(
    PLANS,
    PlansHandle,
    store_plans,
    plans_of,
    yamaa::Plans,
    "plans",
    HandleKind::Plans
);
registry!(
    SELFTESTS,
    SelfTestHandle,
    store_selftest,
    selftest_of,
    yamaa::SelfTest,
    "selftest",
    HandleKind::SelfTest
);

fn table(table: Option<&yamaa::Table>) -> std::result::Result<Robj, Refusal> {
    match table {
        Some(table) => Ok(crate::public_table::frame(table)?),
        None => Ok(NULL.into_robj()),
    }
}

/// A result's printed lines (D37), which `format()` returns.
fn summary_lines(summary: &yamaa::Summary) -> Robj {
    Strings::from_values(summary.lines().collect::<Vec<_>>()).into_robj()
}

#[extendr]
fn spec_check(specification: Raw, project: Robj, version: Raw) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let specification = text(&specification)?;
        let check = engine(&version, None)?
            .spec_check(
                &specification,
                project.as_deref(),
                &crate::interrupt::token(),
            )
            .map_err(|error| from_error(Action::Read(&specification), &error))?;
        store_check(check)
    })
}

#[extendr]
fn check_status(handle: Robj) -> List {
    reply(|| {
        let check = check_of(&handle)?;
        let check = check.try_borrow().map_err(|_| "check handle in use")?;
        Ok(r!(check.status().as_str()))
    })
}

#[extendr]
fn check_issues(handle: Robj) -> List {
    reply(|| {
        let check = check_of(&handle)?;
        let check = check.try_borrow().map_err(|_| "check handle in use")?;
        Ok(crate::public_table::frame(&check.issues())?)
    })
}

#[extendr]
fn check_summary(handle: Robj) -> List {
    reply(|| {
        let check = check_of(&handle)?;
        let check = check.try_borrow().map_err(|_| "check handle in use")?;
        Ok(summary_lines(&check.summary()))
    })
}

/// The printed line of a lazy specification object, which has read nothing
/// (D37): `yamaa spec <path>`, with the path as given.
#[extendr]
fn spec_summary(specification: Raw) -> List {
    reply(|| Ok(summary_lines(&yamaa::Summary::spec(&text(&specification)?))))
}

/// The printed line of a lazy project object: `yamaa project <path>`.
#[extendr]
fn project_summary(project: Raw) -> List {
    reply(|| Ok(summary_lines(&yamaa::Summary::project(&text(&project)?))))
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

/// A run of one specification: built now, or with `stream` built as its
/// output is read (#3697), polling a token of its own from then on.
#[extendr]
fn spec_run(
    specification: Raw,
    project: Robj,
    version: Raw,
    capabilities: List,
    plans: Robj,
    stream: bool,
) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let engine = engine(&version, Some(capabilities))?;
        let specification = text(&specification)?;
        let held = held_plans(&plans)?;
        if stream {
            let once = yamaa::Plans::new();
            let borrowed = held.as_ref().map(|plans| plans.borrow());
            let streamed = engine
                .spec_stream(
                    &specification,
                    project.as_deref(),
                    borrowed.as_deref().unwrap_or(&once),
                    &crate::interrupt::stream_token(),
                )
                .map_err(|error| from_error(Action::Read(&specification), &error))?;
            drop(borrowed);
            return store(RunHeld::Streamed(streamed));
        }
        let token = crate::interrupt::token();
        let run = match held {
            Some(plans) => {
                engine.spec_run_reusing(&specification, project.as_deref(), &plans.borrow(), &token)
            }
            None => engine.spec_run(&specification, project.as_deref(), &token),
        }
        .map_err(|error| from_error(Action::Read(&specification), &error))?;
        store(RunHeld::Built(Box::new(run)))
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
        let source = text(&source)?;
        // The read needs no host capability, so the engine names no R version.
        let engine = Engine::new(EngineOptions::new(
            HostLanguage::R,
            "r".into(),
            String::new(),
        ));
        let items = engine
            .read_odm(&source, &options)
            .map_err(|error| from_error(Action::Read(&source), &error))?;
        Ok(crate::public_table::frame(&items)?)
    })
}

/// Each element of a list of raw UTF-8 vectors as text.
fn texts(value: &Robj) -> std::result::Result<Vec<String>, Refusal> {
    let list = value
        .as_list()
        .ok_or_else(|| Refusal::from("expected a list of raw UTF-8 paths"))?;
    list.values()
        .map(|item| {
            let raw = item
                .as_raw()
                .ok_or_else(|| Refusal::from("expected a list of raw UTF-8 paths"))?;
            text(&raw)
        })
        .collect()
}

/// The lint table of every file `paths` names, fixing them first when `fix`
/// is true; a file that cannot be read or admitted refuses with its issues.
#[extendr]
fn lint(paths: Robj, fix: bool) -> List {
    reply(|| {
        let lint = yamaa::lint(&texts(&paths)?, fix).map_err(Refusal::Failure)?;
        Ok(crate::public_table::frame(&lint.table())?)
    })
}

/// The JSON Schema of a file type as canonical JSON text, one character
/// string (#3030 D149); a type the facade does not know refuses with its
/// issue.
#[extendr]
fn schema(file_type: Raw) -> List {
    reply(|| {
        let schema = yamaa::schema(&text(&file_type)?).map_err(Refusal::Failure)?;
        Ok(r!(schema))
    })
}

/// The condition catalogue (#3700) as a data frame: every condition the
/// engine may report, and every condition a release retired.
#[extendr]
fn conditions() -> List {
    reply(|| Ok(crate::public_table::frame(&yamaa::conditions().table())?))
}

#[extendr]
fn run_status(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| Ok(r!(run.status().as_str()))))
}

/// A run's output: a data frame, or for a streamed run its Arrow C stream
/// (#3697), which produces the output as it is read; `NULL` for a failed
/// run.
#[extendr]
fn run_output(handle: Robj) -> List {
    reply(|| {
        let held = run(&handle)?;
        let held = held.try_borrow().map_err(|_| "run handle in use")?;
        let streamed = match &*held {
            RunHeld::Built(run) => return table(run.output()),
            RunHeld::Streamed(streamed) => streamed,
        };
        let refused = |error| stream_refusal(streamed, &error);
        let failed = match streamed.progress() {
            yamaa::StreamProgress::Cancelled => return Err(refused(Error::Cancelled)),
            yamaa::StreamProgress::Ended => {
                streamed.run().map_err(refused)?.status() == yamaa::Status::Failed
            }
            _ => false,
        };
        if failed {
            return Ok(NULL.into_robj());
        }
        Ok(crate::arrow_stream::stream_object(
            streamed.reader().into_stream(),
        )?)
    })
}

#[extendr]
fn run_issues(handle: Robj) -> List {
    reply(|| {
        read_run(&handle, |run| {
            Ok(crate::public_table::frame(&run.issues())?)
        })
    })
}

#[extendr]
fn run_report(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| table(run.report())))
}

#[extendr]
fn run_save(handle: Robj) -> List {
    reply(|| {
        let held = run(&handle)?;
        let mut held = held.try_borrow_mut().map_err(|_| "run handle in use")?;
        let token = crate::interrupt::token();
        let paths = match &mut *held {
            RunHeld::Built(run) => run.save(&token),
            RunHeld::Streamed(streamed) => streamed
                .run()
                .map_err(|error| stream_refusal(streamed, &error))?
                .save(&token),
        }
        .map_err(Refusal::Failure)?;
        Ok(Strings::from_values(paths).into_robj())
    })
}

#[extendr]
fn run_summary(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| Ok(summary_lines(&run.summary()))))
}

/// Private installed-run evidence for qualification suites.
#[extendr]
fn run_observations(handle: Robj, include_measurements: bool) -> List {
    reply(|| {
        read_run(&handle, |run| {
            Ok(run
                .observations(include_measurements)
                .map_or_else(|| NULL.into_robj(), |value| r!(value.as_str())))
        })
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

/// The listed specifications' names in run order, read without data (D45).
#[extendr]
fn project_names(project: Raw, version: Raw) -> List {
    reply(|| {
        let names = engine(&version, None)?
            .project_names(&text(&project)?)
            .map_err(Refusal::Failure)?;
        Ok(Strings::from_values(names).into_robj())
    })
}

#[extendr]
fn project_check(project: Raw, version: Raw) -> List {
    reply(|| {
        let project = text(&project)?;
        let check = engine(&version, None)?
            .project_check(&project, &crate::interrupt::token())
            .map_err(|error| from_error(Action::Read(&project), &error))?;
        store_check(check)
    })
}

#[extendr]
fn project_run(project: Raw, section: Robj, version: Raw, capabilities: List, plans: Robj) -> List {
    reply(|| {
        let section = optional_text(&section)?;
        let engine = engine(&version, Some(capabilities))?;
        let project = text(&project)?;
        let token = crate::interrupt::token();
        let runs = match held_plans(&plans)? {
            Some(plans) => {
                engine.project_run_reusing(&project, section.as_deref(), &plans.borrow(), &token)
            }
            None => engine.project_run(&project, section.as_deref(), &token),
        }
        // Only an unknown section refuses the call with issues.
        .map_err(|error| {
            from_error(
                Action::Select(section.as_deref().unwrap_or_default()),
                &error,
            )
        })?;
        store_project_run(runs)
    })
}

#[extendr]
fn project_run_status(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        Ok(r!(runs.status().as_str()))
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
fn project_run_member_status(handle: Robj, name: Raw) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        let run = runs.member(&text(&name)?).map_err(Refusal::Failure)?;
        Ok(r!(run.status().as_str()))
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

/// One member's printed lines, its run's under its name (D37).
#[extendr]
fn project_run_member_summary(handle: Robj, name: Raw) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        let run = runs.member(&text(&name)?).map_err(Refusal::Failure)?;
        Ok(summary_lines(&run.summary()))
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
        let paths = runs
            .save(&crate::interrupt::token())
            .map_err(Refusal::Failure)?;
        Ok(Strings::from_values(paths).into_robj())
    })
}

/// The printed lines, with one line per member (D37).
#[extendr]
fn project_run_summary(handle: Robj) -> List {
    reply(|| {
        let runs = project_run_of(&handle)?;
        let runs = runs.try_borrow().map_err(|_| "project run handle in use")?;
        Ok(summary_lines(&runs.summary()))
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
                &crate::interrupt::token(),
            )
            // Only an unknown section refuses the call with issues.
            .map_err(|error| {
                from_error(
                    Action::Select(section.as_deref().unwrap_or_default()),
                    &error,
                )
            })?;
        store_submission(submission)
    })
}

#[extendr]
fn submission_status(handle: Robj) -> List {
    reply(|| {
        let submission = submission_of(&handle)?;
        let submission = submission
            .try_borrow()
            .map_err(|_| "submission handle in use")?;
        Ok(r!(submission.status().as_str()))
    })
}

/// The creation time the documents carry (D69, D129).
#[extendr]
fn submission_created(handle: Robj) -> List {
    reply(|| {
        let submission = submission_of(&handle)?;
        let submission = submission
            .try_borrow()
            .map_err(|_| "submission handle in use")?;
        Ok(r!(submission.created()))
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
        let paths = submission
            .save(&crate::interrupt::token())
            .map_err(Refusal::Failure)?;
        Ok(Strings::from_values(paths).into_robj())
    })
}

#[extendr]
fn submission_summary(handle: Robj) -> List {
    reply(|| {
        let submission = submission_of(&handle)?;
        let submission = submission
            .try_borrow()
            .map_err(|_| "submission handle in use")?;
        Ok(summary_lines(&submission.summary()))
    })
}

/// Run every selftest case through this engine as the R host, with R's
/// `version`, such as `4.6.1` (#3030 D28, D119).
#[extendr]
fn selftest(version: Raw) -> List {
    reply(|| store_selftest(yamaa::selftest(yamaa::Host::R, Some(&text(&version)?))))
}

#[extendr]
fn selftest_status(handle: Robj) -> List {
    reply(|| {
        let selftest = selftest_of(&handle)?;
        let selftest = selftest
            .try_borrow()
            .map_err(|_| "selftest handle in use")?;
        Ok(r!(selftest.status().as_str()))
    })
}

#[extendr]
fn selftest_cases(handle: Robj) -> List {
    reply(|| {
        let selftest = selftest_of(&handle)?;
        let selftest = selftest
            .try_borrow()
            .map_err(|_| "selftest handle in use")?;
        Ok(crate::public_table::frame(&selftest.table())?)
    })
}

/// The engine information that ran the cases, as `yamaa_info()` gives it.
#[extendr]
fn selftest_info(handle: Robj) -> List {
    reply(|| {
        let selftest = selftest_of(&handle)?;
        let selftest = selftest
            .try_borrow()
            .map_err(|_| "selftest handle in use")?;
        Ok(info_list(selftest.info()))
    })
}

/// The engine information as a named list in the facade's field order, with
/// `host_version` `NULL` when the facade has none: the one construction that
/// `yamaa_info()` and a selftest's `info` share (D41).
fn info_list(info: &yamaa::Info) -> Robj {
    let host_version = info
        .host_version
        .as_deref()
        .map_or_else(|| NULL.into_robj(), |version| r!(version));
    list!(
        version = info.version,
        language_versions = Strings::from_values(&info.language_versions),
        features = Strings::from_values(info.features.clone()),
        host = info.host.as_str(),
        host_version = host_version,
        platform = info.platform.as_str()
    )
    .into_robj()
}

/// What this engine is and where it runs, as the R host with R's `version`,
/// such as `4.6.1` (D41, P-INFO-1).
#[extendr]
fn info(version: Raw) -> List {
    reply(|| {
        Ok(info_list(&yamaa::info(
            yamaa::Host::R,
            Some(&text(&version)?),
        )))
    })
}

/// The printed report's lines (D37, D119), which `format()` returns.
#[extendr]
fn selftest_summary(handle: Robj) -> List {
    reply(|| {
        let selftest = selftest_of(&handle)?;
        let selftest = selftest
            .try_borrow()
            .map_err(|_| "selftest handle in use")?;
        Ok(summary_lines(&selftest.summary()))
    })
}

include!("generated/api_entry.rs");
