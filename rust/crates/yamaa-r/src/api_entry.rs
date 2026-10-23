//! Native half of the R host API: specification operations, their results and
//! one error shape. The R facade keeps paths in lazy objects and calls these
//! functions, which reach the engine through the boundary's client;
//! behavior, validation and messages stay in Rust.
//!
//! Every function replies `list(value, error)`. `error` is `NULL` on success,
//! otherwise `list(message, issues, paths)`, where `issues` is the issues data
//! frame (empty when the refusal carries none) and `paths` the files a refused
//! save wrote before it stopped. Every function replies behind the interface
//! gate, so it refuses until the handshake succeeds (`crate::interface`).
use crate::handle::address;
use crate::project_activation::{Functions, Host};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_boundary::abi::Handle;
use yamaa_boundary::client::{Call, Engine, Info, Language, OdmOptions, Owned, Refused};
use yamaa_boundary::HandleKind;

/// Why a call could not return its value.
pub(crate) enum Refusal {
    /// The engine refused: D39's text, the issues that explain it and the
    /// files a refused save wrote.
    Failure {
        message: String,
        issues: Option<Owned>,
        paths: Vec<String>,
    },
    /// The binding or the boundary refused, with no issues.
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

impl From<Refused> for Refusal {
    fn from(refused: Refused) -> Self {
        match refused {
            Refused::Engine {
                message,
                issues,
                paths,
            } => Self::Failure {
                message,
                issues,
                paths,
            },
            Refused::Boundary(message) => Self::Message(message),
            // The interrupt the call kept is signalled in preference to this.
            Refused::Cancelled => Self::Message("the call was cancelled".to_owned()),
        }
    }
}

/// The refusal `error` as the reply's error. A refusal of this boundary
/// itself wrote nothing and carries the engine's table of no issues, or
/// `NULL` before the handshake, when there is no `engine` to ask.
fn refused(error: Refusal, engine: Option<Engine>) -> List {
    let (message, issues, paths) = match error {
        Refusal::Failure {
            message,
            issues,
            paths,
        } => (message, issues, paths),
        Refusal::Message(message) => {
            let issues = engine.and_then(|engine| engine.no_issues().ok());
            (message, issues, Vec::new())
        }
    };
    let paths = Strings::from_values(&paths);
    match issues.as_ref().map(crate::public_table::frame).transpose() {
        Ok(issues) => list!(
            value = NULL,
            error = list!(
                message = message,
                issues = issues.unwrap_or_else(|| NULL.into_robj()),
                paths = paths
            )
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
pub(crate) fn reply(run: impl FnOnce(Engine) -> std::result::Result<Robj, Refusal>) -> List {
    let gate = crate::interface::gate();
    answer(gate.as_ref().ok().copied(), || run(gate?))
}

/// Reply with `run`'s value or refusal, containing a panic and keeping an
/// interrupt the call observed; `engine`, once negotiated, gives a refusal
/// its table of no issues. Only the handshake replies without the gate.
pub(crate) fn answer(
    engine: Option<Engine>,
    run: impl FnOnce() -> std::result::Result<Robj, Refusal>,
) -> List {
    let scope = crate::interrupt::Scope::new();
    let result = catch_unwind(AssertUnwindSafe(run));
    if let Some(condition) = scope.take() {
        return list!(value = NULL, error = list!(condition = condition));
    }
    match result {
        Ok(Ok(value)) => list!(value = value, error = NULL),
        Ok(Err(error)) => refused(error, engine),
        Err(_) => refused("internal host boundary failure".into(), engine),
    }
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
fn parse_capabilities(parsed: List) -> std::result::Result<Functions, Refusal> {
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
    Ok(Functions::new(
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

/// Run a call of the engine as the R host on R's `version`, with the
/// interrupt probe and, when `capabilities` are given, the installed project
/// functions behind its callbacks.
fn called<T>(
    version: &Raw,
    capabilities: Option<List>,
    run: impl FnOnce(&Call<'_>) -> Result<T, Refused>,
) -> std::result::Result<T, Refusal> {
    let version = text(version)?;
    let host = Host::new(capabilities.map(parse_capabilities).transpose()?);
    let call = Call::new(Language::R, "r", &version).with(&host, host.offers_functions());
    run(&call).map_err(Refusal::from)
}

/// One owner per R external pointer: dropping the pointer drops the owner,
/// which gives its engine handle back, and lookups go through the registry,
/// never the pointee.
struct Holder {
    _owned: Owned,
    identity: Cell<usize>,
}

thread_local! {
    static HANDLES: RefCell<BTreeMap<usize, (Handle, HandleKind)>> =
        const { RefCell::new(BTreeMap::new()) };
}

impl Drop for Holder {
    fn drop(&mut self) {
        HANDLES.with(|handles| {
            handles.borrow_mut().remove(&self.identity.get());
        });
    }
}

/// Hold `owned`, a handle of `kind`, in a new R external pointer.
fn store(owned: Owned, kind: HandleKind) -> std::result::Result<Robj, Refusal> {
    let handle = owned.handle();
    let holder = ExternalPtr::new(Holder {
        _owned: owned,
        identity: Cell::new(0),
    });
    let identity = address(holder.as_robj())?;
    holder.identity.set(identity);
    HANDLES.with(|handles| handles.borrow_mut().insert(identity, (handle, kind)));
    Ok(holder.into_robj())
}

/// The engine handle of `kind` an R external pointer holds; `what` names the
/// kind in a refusal.
fn handle_of(robj: &Robj, kind: HandleKind, what: &str) -> std::result::Result<Handle, Refusal> {
    let identity = address(robj)?;
    HANDLES
        .with(|handles| handles.borrow().get(&identity).copied())
        .filter(|(_, held)| *held == kind)
        .map(|(handle, _)| handle)
        .ok_or_else(|| format!("unknown {what} handle").into())
}

fn run_of(robj: &Robj) -> std::result::Result<Handle, Refusal> {
    handle_of(robj, HandleKind::Run, "run")
}

fn check_of(robj: &Robj) -> std::result::Result<Handle, Refusal> {
    handle_of(robj, HandleKind::Check, "check")
}

fn project_run_of(robj: &Robj) -> std::result::Result<Handle, Refusal> {
    handle_of(robj, HandleKind::ProjectRun, "project run")
}

fn submission_of(robj: &Robj) -> std::result::Result<Handle, Refusal> {
    handle_of(robj, HandleKind::Submission, "submission")
}

fn selftest_of(robj: &Robj) -> std::result::Result<Handle, Refusal> {
    handle_of(robj, HandleKind::SelfTest, "selftest")
}

/// A table as a data frame, or `NULL` for none; the table's handle is given
/// back once it is copied, or once its conversion fails.
fn table(table: Option<Owned>) -> std::result::Result<Robj, Refusal> {
    match table {
        Some(table) => Ok(crate::public_table::frame(&table)?),
        None => Ok(NULL.into_robj()),
    }
}

/// A result's printed lines (D37), which `format()` returns.
fn lines(lines: Vec<String>) -> Robj {
    Strings::from_values(lines).into_robj()
}

/// Save a result, with the R interrupt probe behind the call.
fn save(engine: Engine, result: Handle) -> std::result::Result<Robj, Refusal> {
    let host = Host::new(None);
    let call = Call::new(Language::R, "r", "").with(&host, false);
    Ok(Strings::from_values(engine.save(&call, result)?).into_robj())
}

#[extendr]
fn spec_check(specification: Raw, project: Robj, version: Raw) -> List {
    reply(|engine| {
        let project = optional_text(&project)?;
        let specification = text(&specification)?;
        let check = called(&version, None, |call| {
            engine.spec_check(call, &specification, project.as_deref())
        })?;
        store(check, HandleKind::Check)
    })
}

#[extendr]
fn check_status(handle: Robj) -> List {
    reply(|engine| Ok(r!(engine.status(check_of(&handle)?, None)?)))
}

#[extendr]
fn check_issues(handle: Robj) -> List {
    reply(|engine| table(Some(engine.issues(check_of(&handle)?, None)?)))
}

#[extendr]
fn check_summary(handle: Robj) -> List {
    reply(|engine| Ok(lines(engine.summary(check_of(&handle)?, None)?)))
}

/// The printed line of a lazy specification object, which has read nothing
/// (D37): `yamaa spec <path>`, with the path as given.
#[extendr]
fn spec_summary(specification: Raw) -> List {
    reply(|engine| Ok(lines(engine.spec_summary(&text(&specification)?)?)))
}

/// The printed line of a lazy project object: `yamaa project <path>`.
#[extendr]
fn project_summary(project: Raw) -> List {
    reply(|engine| Ok(lines(engine.project_summary(&text(&project)?)?)))
}

/// The holder for the compiled plans one lazy object keeps between its runs
/// (#2821): `handle` while it is live, else a new empty one. A lazy object
/// passes `NULL` before its first run; a handle restored by `readRDS`, an
/// `.RData` file or another R process has expired, so the restored object
/// starts afresh (#3069). Creating a holder reads nothing.
#[extendr]
fn plans(handle: Robj) -> List {
    reply(|engine| {
        if !handle.is_null() && handle_of(&handle, HandleKind::Plans, "plans").is_ok() {
            return Ok(handle);
        }
        store(engine.plans()?, HandleKind::Plans)
    })
}

/// The holder a lazy object passes, or 0 for a one-shot run. Nothing borrows
/// a holder mutably, so a run started from a project function shares it too.
fn held_plans(plans: &Robj) -> std::result::Result<Handle, Refusal> {
    if plans.is_null() {
        return Ok(0);
    }
    handle_of(plans, HandleKind::Plans, "plans")
}

#[extendr]
fn spec_run(
    specification: Raw,
    project: Robj,
    version: Raw,
    capabilities: List,
    plans: Robj,
) -> List {
    reply(|engine| {
        let project = optional_text(&project)?;
        let specification = text(&specification)?;
        let plans = held_plans(&plans)?;
        let run = called(&version, Some(capabilities), |call| {
            engine.spec_run(call, &specification, project.as_deref(), plans)
        })?;
        store(run, HandleKind::Run)
    })
}

#[extendr]
fn spec_resolve(specification: Raw, project: Robj, version: Raw) -> List {
    reply(|engine| {
        let project = optional_text(&project)?;
        let specification = text(&specification)?;
        let value = called(&version, None, |call| {
            engine.spec_resolve(call, &specification, project.as_deref())
        })?;
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
    reply(|engine| {
        let member = optional_text(&archive_member)?;
        let limit = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
        let options = OdmOptions {
            archive_member: member.as_deref(),
            max_archive_members: limit(max_archive_members),
            max_expanded_bytes: limit(max_expanded_bytes),
            max_xml_depth: limit(max_xml_depth),
        };
        let source = text(&source)?;
        // The read needs no host capability, so the call names no R version.
        let call = Call::new(Language::R, "r", "");
        table(Some(engine.read_odm(&call, &source, &options)?))
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
    reply(|engine| table(Some(engine.lint(&texts(&paths)?, fix)?)))
}

/// The JSON Schema of a file type as canonical JSON text, one character
/// string (#3030 D149); a type the facade does not know refuses with its
/// issue.
#[extendr]
fn schema(file_type: Raw) -> List {
    reply(|engine| Ok(r!(engine.schema(&text(&file_type)?)?)))
}

#[extendr]
fn run_status(handle: Robj) -> List {
    reply(|engine| Ok(r!(engine.status(run_of(&handle)?, None)?)))
}

#[extendr]
fn run_output(handle: Robj) -> List {
    reply(|engine| table(engine.output(run_of(&handle)?, None)?))
}

#[extendr]
fn run_issues(handle: Robj) -> List {
    reply(|engine| table(Some(engine.issues(run_of(&handle)?, None)?)))
}

#[extendr]
fn run_report(handle: Robj) -> List {
    reply(|engine| table(engine.report(run_of(&handle)?, None)?))
}

#[extendr]
fn run_save(handle: Robj) -> List {
    reply(|engine| save(engine, run_of(&handle)?))
}

#[extendr]
fn run_summary(handle: Robj) -> List {
    reply(|engine| Ok(lines(engine.summary(run_of(&handle)?, None)?)))
}

/// Private installed-run evidence for qualification suites.
#[extendr]
fn run_observations(handle: Robj, include_measurements: bool) -> List {
    reply(|engine| {
        let observations = engine.observations(run_of(&handle)?, include_measurements)?;
        Ok(observations.map_or_else(|| NULL.into_robj(), |value| r!(value.as_str())))
    })
}

#[extendr]
fn project_spec(project: Raw, name: Raw, version: Raw) -> List {
    reply(|engine| {
        let (project, name) = (text(&project)?, text(&name)?);
        let path = called(&version, None, |call| {
            engine.project_spec(call, &project, &name)
        })?;
        Ok(r!(path.as_str()))
    })
}

/// The listed specifications' names in run order, read without data (D45).
#[extendr]
fn project_names(project: Raw, version: Raw) -> List {
    reply(|engine| {
        let project = text(&project)?;
        let names = called(&version, None, |call| engine.project_names(call, &project))?;
        Ok(Strings::from_values(names).into_robj())
    })
}

#[extendr]
fn project_check(project: Raw, version: Raw) -> List {
    reply(|engine| {
        let project = text(&project)?;
        let check = called(&version, None, |call| engine.project_check(call, &project))?;
        store(check, HandleKind::Check)
    })
}

#[extendr]
fn project_run(project: Raw, section: Robj, version: Raw, capabilities: List, plans: Robj) -> List {
    reply(|engine| {
        let section = optional_text(&section)?;
        let project = text(&project)?;
        let plans = held_plans(&plans)?;
        let runs = called(&version, Some(capabilities), |call| {
            engine.project_run(call, &project, section.as_deref(), plans)
        })?;
        store(runs, HandleKind::ProjectRun)
    })
}

#[extendr]
fn project_run_status(handle: Robj) -> List {
    reply(|engine| Ok(r!(engine.status(project_run_of(&handle)?, None)?)))
}

#[extendr]
fn project_run_names(handle: Robj) -> List {
    reply(|engine| {
        let names = engine.names(project_run_of(&handle)?)?;
        Ok(Strings::from_values(names).into_robj())
    })
}

#[extendr]
fn project_run_member_status(handle: Robj, name: Raw) -> List {
    reply(|engine| {
        let status = engine.status(project_run_of(&handle)?, Some(&text(&name)?))?;
        Ok(r!(status))
    })
}

#[extendr]
fn project_run_output(handle: Robj, name: Raw) -> List {
    reply(|engine| table(engine.output(project_run_of(&handle)?, Some(&text(&name)?))?))
}

#[extendr]
fn project_run_member_issues(handle: Robj, name: Raw) -> List {
    reply(|engine| {
        let issues = engine.issues(project_run_of(&handle)?, Some(&text(&name)?))?;
        table(Some(issues))
    })
}

#[extendr]
fn project_run_member_report(handle: Robj, name: Raw) -> List {
    reply(|engine| table(engine.report(project_run_of(&handle)?, Some(&text(&name)?))?))
}

/// One member's printed lines, its run's under its name (D37).
#[extendr]
fn project_run_member_summary(handle: Robj, name: Raw) -> List {
    reply(|engine| {
        let summary = engine.summary(project_run_of(&handle)?, Some(&text(&name)?))?;
        Ok(lines(summary))
    })
}

#[extendr]
fn project_run_issues(handle: Robj) -> List {
    reply(|engine| table(Some(engine.issues(project_run_of(&handle)?, None)?)))
}

#[extendr]
fn project_run_report(handle: Robj) -> List {
    reply(|engine| table(engine.report(project_run_of(&handle)?, None)?))
}

#[extendr]
fn project_run_save(handle: Robj) -> List {
    reply(|engine| save(engine, project_run_of(&handle)?))
}

/// The printed lines, with one line per member (D37).
#[extendr]
fn project_run_summary(handle: Robj) -> List {
    reply(|engine| Ok(lines(engine.summary(project_run_of(&handle)?, None)?)))
}

#[extendr]
fn project_submission(project: Raw, section: Robj, created: Robj, version: Raw) -> List {
    reply(|engine| {
        let section = optional_text(&section)?;
        let created = optional_text(&created)?;
        let project = text(&project)?;
        let submission = called(&version, None, |call| {
            engine.project_submission(call, &project, section.as_deref(), created.as_deref())
        })?;
        store(submission, HandleKind::Submission)
    })
}

#[extendr]
fn submission_status(handle: Robj) -> List {
    reply(|engine| Ok(r!(engine.status(submission_of(&handle)?, None)?)))
}

/// The creation time the documents carry (D69, D129).
#[extendr]
fn submission_created(handle: Robj) -> List {
    reply(|engine| Ok(r!(engine.created(submission_of(&handle)?)?)))
}

#[extendr]
fn submission_issues(handle: Robj) -> List {
    reply(|engine| table(Some(engine.issues(submission_of(&handle)?, None)?)))
}

#[extendr]
fn submission_save(handle: Robj) -> List {
    reply(|engine| save(engine, submission_of(&handle)?))
}

#[extendr]
fn submission_summary(handle: Robj) -> List {
    reply(|engine| Ok(lines(engine.summary(submission_of(&handle)?, None)?)))
}

/// Run every selftest case through this engine as the R host, with R's
/// `version`, such as `4.6.1` (#3030 D28, D119).
#[extendr]
fn selftest(version: Raw) -> List {
    reply(|engine| {
        let version = text(&version)?;
        let selftest = engine.selftest(&Call::new(Language::R, "r", &version))?;
        store(selftest, HandleKind::SelfTest)
    })
}

#[extendr]
fn selftest_status(handle: Robj) -> List {
    reply(|engine| Ok(r!(engine.status(selftest_of(&handle)?, None)?)))
}

#[extendr]
fn selftest_cases(handle: Robj) -> List {
    reply(|engine| table(Some(engine.cases(selftest_of(&handle)?)?)))
}

/// The engine information that ran the cases, as `yamaa_info()` gives it.
#[extendr]
fn selftest_info(handle: Robj) -> List {
    reply(|engine| Ok(info_list(&engine.selftest_info(selftest_of(&handle)?)?)))
}

/// The engine information as a named list in the facade's field order, with
/// `host_version` `NULL` when the engine has none: the one construction that
/// `yamaa_info()` and a selftest's `info` share (D41).
fn info_list(info: &Info) -> Robj {
    let host_version = info
        .host_version
        .as_deref()
        .map_or_else(|| NULL.into_robj(), |version| r!(version));
    list!(
        version = info.version.as_str(),
        language_versions = Strings::from_values(&info.language_versions),
        features = Strings::from_values(&info.features),
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
    reply(|engine| {
        let version = text(&version)?;
        Ok(info_list(&engine.info(&Call::new(
            Language::R,
            "r",
            &version,
        ))?))
    })
}

/// The printed report's lines (D37, D119), which `format()` returns.
#[extendr]
fn selftest_summary(handle: Robj) -> List {
    reply(|engine| Ok(lines(engine.summary(selftest_of(&handle)?, None)?)))
}

include!("generated/api_entry.rs");
