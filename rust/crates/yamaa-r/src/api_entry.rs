//! Native half of the R host API: specification operations, their results and
//! one error shape. The R facade keeps paths in lazy objects and calls these
//! functions, which call the `yamaa` facade; behavior, validation and
//! messages stay in Rust.
//!
//! Every function replies `list(value, error)`. `error` is `NULL` on success,
//! otherwise `list(message, issues, paths, state)`, where `issues` is the issues data
//! frame (empty when the refusal carries none) and `paths` the files a refused
//! save wrote before it stopped. Every function replies behind the interface
//! gate, so it refuses until the handshake succeeds (`crate::interface`).
//!
//! The facade engine of every call is built in one place, [`engine`], from
//! the R host, R's version and, for a call that may run project functions,
//! the installed functions' port.
use crate::handle::address;
use crate::progress::Progress;
use crate::project_activation::Functions;
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    num::NonZeroUsize,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::{Rc, Weak},
    sync::{Arc, Mutex, MutexGuard},
};
use yamaa::{
    CancellationToken, EngineOptions, Failure, FailureKind, HostLanguage, Partitions,
    ProjectRequest, SpecRequest, SubmissionOptions,
};
use yamaa_boundary::{HandleKind, Live};

/// Why a call could not return its value.
pub(crate) enum Refusal {
    /// The engine refused: D39's text, the issues that explain it and the
    /// files a refused save wrote.
    Failure(Failure),
    /// The binding refused, with no issues.
    Message(String),
    /// A handle or operation state refused the call.
    State(String),
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

/// A facade failure: a cancelled call's interrupt is signalled in preference
/// to its refusal, and any other failure keeps its issues and paths.
impl From<Failure> for Refusal {
    fn from(failure: Failure) -> Self {
        match failure.kind {
            FailureKind::Cancelled => Self::State("the call was cancelled".to_owned()),
            _ => Self::Failure(failure),
        }
    }
}

/// A streamed run's refusal of a reader or of its result (#3697): the
/// failure's text, `cannot read <subject>: ...`, with no issues, as the
/// binding's other refusals.
fn streamed(failure: Failure) -> Refusal {
    Refusal::Failure(failure)
}

/// The refusal `error` as the reply's error. A refusal of this binding itself
/// wrote nothing and carries an empty issues frame even before negotiation
/// and after a conversion failure (#4077, reversing D258).
fn refused(error: Refusal) -> List {
    let (message, issues, paths, state, parent) = match error {
        Refusal::Failure(failure) => {
            let parent = failure
                .cause
                .as_ref()
                .and_then(|payload| {
                    payload.with::<crate::thread_bound::ThreadBound<Robj>, _>(|condition| {
                        condition.get().cloned()
                    })
                })
                .flatten()
                .unwrap_or_else(|| NULL.into_robj());
            let state = matches!(
                failure.kind,
                FailureKind::Stream { .. } | FailureKind::Cancelled
            );
            (
                failure.message,
                failure.issues,
                failure.paths,
                state,
                parent,
            )
        }
        Refusal::State(message) => (
            message,
            yamaa::diagnostic::issues(&[]),
            Vec::new(),
            true,
            NULL.into_robj(),
        ),
        Refusal::Message(message) => (
            message,
            yamaa::diagnostic::issues(&[]),
            Vec::new(),
            false,
            NULL.into_robj(),
        ),
    };
    let (message, issues) = match crate::public_table::frame(&issues) {
        Ok(issues) => (message, issues),
        Err(conversion) => (
            format!("{message}; {conversion}"),
            crate::public_table::frame(&yamaa::diagnostic::issues(&[]))
                .expect("the fixed empty issues schema fits an R data frame"),
        ),
    };
    list!(
        value = NULL,
        error = list!(
            message = message,
            issues = issues,
            paths = Strings::from_values(&paths),
            state = state,
            parent = parent
        )
    )
}

/// Reply with a value or a refusal, including issues even before negotiation.
pub(crate) fn reply(run: impl FnOnce() -> std::result::Result<Robj, Refusal>) -> List {
    answer(|| {
        crate::interface::gate()?;
        run()
    })
}

/// Contain panics and retain interrupts; every ordinary refusal carries a frame.
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

/// The partitions every build runs over (#3794, D264): key-bounded over the
/// specification's two leading declared keys, of at most 624,640 driver
/// records each, with the whole-table fallback. The binding depends on the
/// facade alone, so it spells core's `Limits::PARTITION_KEYS` and
/// `Limits::PARTITION_ROWS`, which the facade's `client_partitions.rs` checks.
const PARTITIONS: [usize; 2] = [2, 624_640];

/// [`PARTITIONS`], deriving the engine's default threads at once.
fn default_partitions() -> Partitions {
    let [keys, rows] = PARTITIONS.map(|count| NonZeroUsize::new(count).expect("nonzero"));
    Partitions::by_keys(keys, rows).with_whole_table_fallback()
}

/// [`PARTITIONS`], up to `threads` of one run deriving at once unless it is
/// `NULL` (#4193); `NA`, or anything but one positive whole number, refuses
/// the call.
fn partitions(threads: &Robj) -> std::result::Result<Partitions, Refusal> {
    let partitions = default_partitions();
    if threads.is_null() {
        return Ok(partitions);
    }
    let value = match threads.is_na() {
        true => yamaa::ContextValue::Text("NA".into()),
        false => odm_limit_value(threads),
    };
    partitions
        .with_host_threads(&value)
        .ok_or_else(|| Refusal::from("`threads` must be one positive whole number or NULL"))
}

/// The facade engine of one call: the R host on R's `version`, over
/// `partitions`, with the installed project functions when `functions` are
/// given, reporting to `progress` when one is given (#3762). Every call
/// builds its engine here.
fn engine(
    version: &str,
    functions: Option<Functions>,
    progress: Option<Progress>,
    partitions: Partitions,
) -> yamaa::Engine {
    let options =
        EngineOptions::new(HostLanguage::R, "r".into(), version.into()).with_partitions(partitions);
    let options = match functions {
        Some(functions) => options.with_functions(Arc::new(functions)),
        None => options,
    };
    yamaa::Engine::new(match progress {
        Some(progress) => options.with_progress(move |event| progress.call(event)),
        None => options,
    })
}

/// The progress callback the R layer wrapped, or none for `NULL`.
fn progress(value: &Robj) -> std::result::Result<Option<Progress>, Refusal> {
    Progress::of(value).map_err(Refusal::from)
}

/// The facade engine that a call's `settings` configure (#4214): the one
/// list the generated layer passes for what configures the engine rather
/// than the request, holding R's version, the project host capabilities,
/// the progress callback (#3762) and `threads` (#4193), in that order. The
/// engine installs the project functions only when `functions`.
fn configured(settings: &List, functions: bool) -> std::result::Result<yamaa::Engine, Refusal> {
    if settings.len() != 4 {
        return Err("invalid engine settings".into());
    }
    let setting = |index: usize| {
        settings
            .elt(index)
            .map_err(|_| Refusal::from("invalid engine settings"))
    };
    let version = setting(0)?
        .as_raw()
        .ok_or("engine settings version must be UTF-8 bytes")?;
    let version = text(&version)?;
    let functions = match functions {
        true => Some(parse_capabilities(
            setting(1)?
                .as_list()
                .ok_or("engine settings capabilities must be a list")?,
        )?),
        false => None,
    };
    let partitions = partitions(&setting(3)?)?;
    let progress = progress(&setting(2)?)?;
    Ok(engine(&version, functions, progress, partitions))
}

/// Run a cancellable call of the engine its `settings` configure, with the
/// R interrupt probe behind its token and, when `functions`, the installed
/// project functions. Once the call returns the token is polled once more,
/// so an interrupt that arrived as it returned still cancels it (D186).
fn called<T>(
    settings: &List,
    functions: bool,
    run: impl FnOnce(&yamaa::Engine, &CancellationToken) -> std::result::Result<T, Failure>,
) -> std::result::Result<T, Refusal> {
    let engine = configured(settings, functions)?;
    let token = crate::interrupt::token();
    let answer = run(&engine, &token);
    if token.is_cancelled() {
        return Err("the call was cancelled".into());
    }
    answer.map_err(Refusal::from)
}

/// What an R external pointer holds: a result or a plans holder.
enum Value {
    Check(Mutex<yamaa::Check>),
    Run(Mutex<yamaa::Run>),
    Streamed(yamaa::Streamed),
    ProjectRun(Mutex<yamaa::ProjectRun>),
    Submission(Mutex<yamaa::Submission>),
    Lint(yamaa::Lint),
    SelfTest(yamaa::SelfTest),
    Plans(yamaa::Plans),
}

impl Value {
    fn kind(&self) -> HandleKind {
        match self {
            Value::Check(_) => HandleKind::Check,
            Value::Run(_) | Value::Streamed(_) => HandleKind::Run,
            Value::ProjectRun(_) => HandleKind::ProjectRun,
            Value::Submission(_) => HandleKind::Submission,
            Value::Lint(_) => HandleKind::Lint,
            Value::SelfTest(_) => HandleKind::SelfTest,
            Value::Plans(_) => HandleKind::Plans,
        }
    }
}

/// One owner per R external pointer: dropping the pointer drops the owner,
/// and lookups go through the registry, never the pointee. Each owner counts
/// in the census while it lives.
struct Holder {
    value: Rc<Value>,
    identity: Cell<usize>,
    _live: Live,
}

thread_local! {
    static HANDLES: RefCell<BTreeMap<usize, Weak<Value>>> = const { RefCell::new(BTreeMap::new()) };
}

impl Drop for Holder {
    fn drop(&mut self) {
        HANDLES.with(|handles| {
            handles.borrow_mut().remove(&self.identity.get());
        });
    }
}

/// Hold `value` in a new R external pointer.
fn store(value: Value) -> std::result::Result<Robj, Refusal> {
    let kind = value.kind();
    let holder = ExternalPtr::new(Holder {
        value: Rc::new(value),
        identity: Cell::new(0),
        _live: crate::interface::CENSUS.hold(kind),
    });
    let identity = address(holder.as_robj())?;
    holder.identity.set(identity);
    HANDLES.with(|handles| {
        handles
            .borrow_mut()
            .insert(identity, Rc::downgrade(&holder.value))
    });
    Ok(holder.into_robj())
}

/// The value of `kind` an R external pointer holds; `what` names the kind in
/// a refusal.
fn held(robj: &Robj, kind: HandleKind, what: &str) -> std::result::Result<Rc<Value>, Refusal> {
    let identity = address(robj).map_err(Refusal::State)?;
    HANDLES
        .with(|handles| handles.borrow().get(&identity).and_then(Weak::upgrade))
        .filter(|value| value.kind() == kind)
        .ok_or_else(|| Refusal::State(format!("unknown {what} handle")))
}

/// A result, unless another call holds it (D258): `noun` names its kind in
/// words.
fn locked<'a, T>(
    value: &'a Mutex<T>,
    noun: &str,
) -> std::result::Result<MutexGuard<'a, T>, Refusal> {
    value
        .try_lock()
        .map_err(|_| Refusal::State(format!("{noun} handle is in use or poisoned")))
}

/// Read the run an R external pointer holds with `read`. A streamed run
/// nobody started to read runs whole first; one whose stream has not ended,
/// or that was cancelled, refuses.
fn read_run<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::Run) -> std::result::Result<T, Refusal>,
) -> std::result::Result<T, Refusal> {
    let value = held(handle, HandleKind::Run, "run")?;
    match &*value {
        Value::Run(run) => read(&*locked(run, "run")?),
        Value::Streamed(run) => read(&*run.run().map_err(streamed)?),
        _ => Err(Refusal::State("unknown run handle".into())),
    }
}

/// Read the check an R external pointer holds with `read`.
fn read_check<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::Check) -> T,
) -> std::result::Result<T, Refusal> {
    match &*held(handle, HandleKind::Check, "check")? {
        Value::Check(check) => Ok(read(&*locked(check, "check")?)),
        _ => Err(Refusal::State("unknown check handle".into())),
    }
}

/// Read the project run an R external pointer holds with `read`.
fn read_project_run<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::ProjectRun) -> std::result::Result<T, Refusal>,
) -> std::result::Result<T, Refusal> {
    match &*held(handle, HandleKind::ProjectRun, "project run")? {
        Value::ProjectRun(runs) => read(&*locked(runs, "project run")?),
        _ => Err(Refusal::State("unknown project run handle".into())),
    }
}

/// Read the member `name` of the project run an R external pointer holds.
fn read_member<T>(
    handle: &Robj,
    name: &Raw,
    read: impl FnOnce(&yamaa::Run) -> std::result::Result<T, Refusal>,
) -> std::result::Result<T, Refusal> {
    read_project_run(handle, |runs| read(runs.member(&text(name)?)?))
}

/// Read the submission an R external pointer holds with `read`.
fn read_submission<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::Submission) -> T,
) -> std::result::Result<T, Refusal> {
    match &*held(handle, HandleKind::Submission, "submission")? {
        Value::Submission(submission) => Ok(read(&*locked(submission, "submission")?)),
        _ => Err(Refusal::State("unknown submission handle".into())),
    }
}

/// Read the selftest an R external pointer holds with `read`.
fn read_selftest<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::SelfTest) -> std::result::Result<T, Refusal>,
) -> std::result::Result<T, Refusal> {
    match &*held(handle, HandleKind::SelfTest, "selftest")? {
        Value::SelfTest(selftest) => read(selftest),
        _ => Err(Refusal::State("unknown selftest handle".into())),
    }
}

/// A table as a data frame, or `NULL` for none.
fn table(table: Option<&yamaa::Table>) -> std::result::Result<Robj, Refusal> {
    match table {
        Some(table) => Ok(crate::public_table::frame(table)?),
        None => Ok(NULL.into_robj()),
    }
}

/// A result's printed lines (D37), which `format()` returns.
fn lines(summary: &yamaa::Summary) -> Robj {
    Strings::from_values(summary.lines().collect::<Vec<_>>()).into_robj()
}

/// Save a result with `save`, with the R interrupt probe behind the call's
/// token.
fn save(
    save: impl FnOnce(&CancellationToken) -> std::result::Result<Vec<String>, Refusal>,
) -> std::result::Result<Robj, Refusal> {
    let token = crate::interrupt::token();
    let paths = save(&token);
    if token.is_cancelled() {
        return Err("the call was cancelled".into());
    }
    Ok(Strings::from_values(paths?).into_robj())
}

/// The request of `specification`, read with `project` when one is given.
fn spec_request<'a>(
    engine: &'a yamaa::Engine,
    specification: &'a str,
    project: Option<&'a str>,
) -> SpecRequest<'a> {
    let request = engine.spec(specification);
    match project {
        Some(project) => request.project(project),
        None => request,
    }
}

/// The request of `project`, its run and submission scoped to `section`
/// when one is given.
fn project_request<'a>(
    engine: &'a yamaa::Engine,
    project: &'a str,
    section: Option<&'a str>,
) -> ProjectRequest<'a> {
    let request = engine.project(project);
    match section {
        Some(section) => request.section(section),
        None => request,
    }
}

/// The check of a specification, or, with `parent`, of a fragment composed on
/// the specification `parent` names as its parent (#3694).
#[extendr]
fn spec_check(specification: Raw, project: Robj, parent: Robj, settings: List) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let parent = optional_text(&parent)?;
        let specification = text(&specification)?;
        let check = called(&settings, false, |engine, token| {
            let request =
                spec_request(engine, &specification, project.as_deref()).cancel_with(token);
            match parent.as_deref() {
                Some(parent) => request.check_on(parent),
                None => request.check(),
            }
        })?;
        store(Value::Check(Mutex::new(check)))
    })
}

#[extendr]
fn check_status(handle: Robj) -> List {
    reply(|| Ok(r!(read_check(&handle, |check| check.status().as_str())?)))
}

#[extendr]
fn check_issues(handle: Robj) -> List {
    reply(|| table(Some(&read_check(&handle, yamaa::Check::issues)?)))
}

#[extendr]
fn check_timings(handle: Robj) -> List {
    reply(|| table(Some(&read_check(&handle, |check| check.timings().table())?)))
}

#[extendr]
fn check_summary(handle: Robj) -> List {
    reply(|| read_check(&handle, |check| lines(&check.summary())))
}

/// The printed line of a lazy specification object, which has read nothing
/// (D37): `yamaa spec <path>`, with the path as given.
#[extendr]
fn spec_summary(specification: Raw) -> List {
    reply(|| Ok(lines(&yamaa::Summary::spec(&text(&specification)?))))
}

/// The printed line of a lazy project object: `yamaa project <path>`.
#[extendr]
fn project_summary(project: Raw) -> List {
    reply(|| Ok(lines(&yamaa::Summary::project(&text(&project)?))))
}

/// The holder for the compiled plans one lazy object keeps between its runs
/// (#2821): `handle` while it is live, else a new empty one. A lazy object
/// passes `NULL` before its first run; a handle restored by `readRDS`, an
/// `.RData` file or another R process has expired, so the restored object
/// starts afresh (#3069). Creating a holder reads nothing.
#[extendr]
fn plans(handle: Robj) -> List {
    reply(|| {
        if !handle.is_null() && held(&handle, HandleKind::Plans, "plans").is_ok() {
            return Ok(handle);
        }
        store(Value::Plans(yamaa::Plans::new()))
    })
}

/// The holder a lazy object passes, or none for a one-shot run. Nothing
/// borrows a holder mutably, so a run started from a project function shares
/// it too.
fn held_plans(plans: &Robj) -> std::result::Result<Option<Rc<Value>>, Refusal> {
    if plans.is_null() {
        return Ok(None);
    }
    held(plans, HandleKind::Plans, "plans").map(Some)
}

/// The plans a holder keeps.
fn plans_of(holder: &Option<Rc<Value>>) -> Option<&yamaa::Plans> {
    match holder.as_deref() {
        Some(Value::Plans(plans)) => Some(plans),
        _ => None,
    }
}

/// A run of one specification: built now, or with `stream` built as its
/// output is read (#3697), polling a token of its own from then on.
#[extendr]
fn spec_run(
    specification: Raw,
    project: Robj,
    plans: Robj,
    stream: bool,
    inputs: Robj,
    settings: List,
) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let specification = text(&specification)?;
        let inputs = crate::input_frames::import(&specification, &inputs)?;
        let holder = held_plans(&plans)?;
        let plans = plans_of(&holder);
        if stream {
            let engine = configured(&settings, true)?;
            let token = crate::interrupt::stream_token();
            let request = spec_request(&engine, &specification, project.as_deref())
                .cancel_with(&token)
                .inputs(inputs);
            let run = match plans {
                Some(plans) => request.reusing(plans),
                None => request,
            }
            .stream();
            if token.is_cancelled() {
                return Err("the call was cancelled".into());
            }
            return store(Value::Streamed(run?));
        }
        let run = called(&settings, true, |engine, token| {
            let request = spec_request(engine, &specification, project.as_deref())
                .cancel_with(token)
                .inputs(inputs);
            match plans {
                Some(plans) => request.reusing(plans),
                None => request,
            }
            .run()
        })?;
        store(Value::Run(Mutex::new(run)))
    })
}

#[extendr]
fn spec_resolve(specification: Raw, project: Robj, version: Raw) -> List {
    reply(|| {
        let project = optional_text(&project)?;
        let specification = text(&specification)?;
        let engine = engine(&text(&version)?, None, None, default_partitions());
        let value = spec_request(&engine, &specification, project.as_deref()).resolve()?;
        Ok(crate::context_values::context_value(&value)?)
    })
}

#[extendr]
fn read_odm(
    source: Raw,
    archive_member: Robj,
    max_archive_members: Robj,
    max_expanded_bytes: Robj,
    max_xml_depth: Robj,
) -> List {
    reply(|| {
        let source = text(&source)?;
        let mut options = yamaa::OdmOptions::from_host_limits(
            &source,
            odm_limit_value(&max_archive_members),
            odm_limit_value(&max_expanded_bytes),
            odm_limit_value(&max_xml_depth),
        )?;
        if let Some(member) = optional_text(&archive_member)? {
            options = options.with_archive_member(member);
        }
        // The read needs no host capability, so the engine names no R version.
        let items = engine("", None, None, default_partitions()).read_odm(&source, &options)?;
        table(Some(&items))
    })
}

/// Preserve R scalar kinds; NA and NULL are missing rather than integers.
fn odm_limit_value(value: &Robj) -> yamaa::ContextValue {
    use yamaa::ContextValue as V;
    if value.is_null() || value.is_na() {
        V::Null
    } else if value.len() != 1 || value.is_object() {
        V::Text("non-numeric value".into())
    } else if let Some(value) = value.as_bool() {
        V::Bool(value)
    } else if let Some(value) = value.as_integer() {
        V::Int(i64::from(value))
    } else if let Some(value) = value.as_real() {
        V::Float(value)
    } else if let Some(value) = value.as_str() {
        V::Text(value.into())
    } else {
        V::Text("non-numeric value".into())
    }
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

/// The lint result of every file `paths` names, fixing them first when `fix`
/// is true; a file that cannot be read or admitted refuses with its issues.
#[extendr]
fn lint(paths: Robj, fix: bool) -> List {
    reply(|| store(Value::Lint(yamaa::lint(&texts(&paths)?, fix)?)))
}

fn read_lint<T>(
    handle: &Robj,
    read: impl FnOnce(&yamaa::Lint) -> std::result::Result<T, Refusal>,
) -> std::result::Result<T, Refusal> {
    match &*held(handle, HandleKind::Lint, "lint")? {
        Value::Lint(result) => read(result),
        _ => Err(Refusal::State("unknown lint handle".into())),
    }
}

#[extendr]
fn lint_status(handle: Robj) -> List {
    reply(|| read_lint(&handle, |result| Ok(r!(result.status().as_str()))))
}

#[extendr]
fn lint_findings(handle: Robj) -> List {
    reply(|| read_lint(&handle, |result| table(Some(&result.table()))))
}

#[extendr]
fn lint_paths(handle: Robj) -> List {
    reply(|| {
        read_lint(&handle, |result| {
            Ok(Strings::from_values(result.rewritten()).into_robj())
        })
    })
}

#[extendr]
fn lint_summary(handle: Robj) -> List {
    reply(|| read_lint(&handle, |result| Ok(r!(result.summary()))))
}

/// The JSON Schema of a file type as canonical JSON text, one character
/// string (#3030 D149); a type the facade does not know refuses with its
/// issue.
#[extendr]
fn schema(file_type: Raw) -> List {
    reply(|| Ok(r!(yamaa::schema(&text(&file_type)?)?)))
}

/// The condition catalogue (#3700) as a data frame: every condition the
/// engine may report, and every condition a release retired.
#[extendr]
fn conditions() -> List {
    reply(|| table(Some(&yamaa::conditions().table())))
}

#[extendr]
fn run_status(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| Ok(r!(run.status().as_str()))))
}

/// What a streamed run's output reads as: its stream, the table it kept once
/// it ran whole, or none for a failed run.
enum StreamedOutput {
    Stream,
    Kept(yamaa::Table),
    Failed,
}

/// A run's output: a data frame, or for a streamed run its Arrow C stream
/// (#3697), which produces the output as it is read; `NULL` for a failed
/// run.
#[extendr]
fn run_output(handle: Robj) -> List {
    reply(|| {
        let value = held(&handle, HandleKind::Run, "run")?;
        let run = match &*value {
            Value::Run(run) => return table(locked(run, "run")?.output()),
            Value::Streamed(run) => run,
            _ => return Err(Refusal::State("unknown run handle".into())),
        };
        let output = match run.progress() {
            // A cancelled run's result refuses as cancelled.
            yamaa::StreamProgress::Cancelled | yamaa::StreamProgress::Ended => {
                let built = run.run().map_err(streamed)?;
                match built.output() {
                    Some(table) => StreamedOutput::Kept(table.clone()),
                    None if built.status() == yamaa::Status::Failed => StreamedOutput::Failed,
                    None => StreamedOutput::Stream,
                }
            }
            _ => StreamedOutput::Stream,
        };
        let stream = match output {
            StreamedOutput::Stream => run.reader().into_stream(),
            StreamedOutput::Kept(table) => table.stream(),
            StreamedOutput::Failed => return Ok(NULL.into_robj()),
        };
        Ok(crate::arrow_stream::stream_object(stream)?)
    })
}

#[extendr]
fn run_issues(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| table(Some(&run.issues()))))
}

#[extendr]
fn run_report(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| table(run.report())))
}

/// The time each phase of the run took (#3762).
#[extendr]
fn run_timings(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| table(Some(&run.timings().table()))))
}

#[extendr]
fn run_save(handle: Robj) -> List {
    reply(|| {
        let value = held(&handle, HandleKind::Run, "run")?;
        save(|token| match &*value {
            Value::Run(run) => Ok(locked(run, "run")?.save(token)?),
            Value::Streamed(run) => Ok(run.run().map_err(streamed)?.save(token)?),
            _ => Err(Refusal::State("unknown run handle".into())),
        })
    })
}

#[extendr]
fn run_summary(handle: Robj) -> List {
    reply(|| read_run(&handle, |run| Ok(lines(&run.summary()))))
}

/// Private installed-run evidence for qualification suites.
#[extendr]
fn run_observations(handle: Robj, include_measurements: bool) -> List {
    reply(|| {
        read_run(&handle, |run| {
            let observations = match include_measurements {
                true => Some(run.measured_observations()),
                false => run.observations(),
            };
            Ok(observations.map_or_else(|| NULL.into_robj(), |value| r!(value.as_str())))
        })
    })
}

#[extendr]
fn project_spec(project: Raw, name: Raw, version: Raw) -> List {
    reply(|| {
        let (project, name) = (text(&project)?, text(&name)?);
        let engine = engine(&text(&version)?, None, None, default_partitions());
        let path = engine.project(&project).spec(&name)?.path().to_owned();
        Ok(r!(path.as_str()))
    })
}

/// The listed specifications' names in run order, read without data (D45).
#[extendr]
fn project_names(project: Raw, version: Raw) -> List {
    reply(|| {
        let project = text(&project)?;
        let names = engine(&text(&version)?, None, None, default_partitions())
            .project(&project)
            .names()?;
        Ok(Strings::from_values(names).into_robj())
    })
}

#[extendr]
fn project_check(project: Raw, settings: List) -> List {
    reply(|| {
        let project = text(&project)?;
        let check = called(&settings, false, |engine, token| {
            engine.project(&project).cancel_with(token).check()
        })?;
        store(Value::Check(Mutex::new(check)))
    })
}

#[extendr]
fn project_run(project: Raw, section: Robj, plans: Robj, settings: List) -> List {
    reply(|| {
        let section = optional_text(&section)?;
        let project = text(&project)?;
        let holder = held_plans(&plans)?;
        let plans = plans_of(&holder);
        let runs = called(&settings, true, |engine, token| {
            let request = project_request(engine, &project, section.as_deref()).cancel_with(token);
            // Only an unknown section refuses the call with issues.
            match plans {
                Some(plans) => request.reusing(plans),
                None => request,
            }
            .run()
        })?;
        store(Value::ProjectRun(Mutex::new(runs)))
    })
}

#[extendr]
fn project_run_status(handle: Robj) -> List {
    reply(|| read_project_run(&handle, |runs| Ok(r!(runs.status().as_str()))))
}

#[extendr]
fn project_run_names(handle: Robj) -> List {
    reply(|| {
        read_project_run(&handle, |runs| {
            Ok(Strings::from_values(runs.names()).into_robj())
        })
    })
}

#[extendr]
fn project_run_member_status(handle: Robj, name: Raw) -> List {
    reply(|| read_member(&handle, &name, |run| Ok(r!(run.status().as_str()))))
}

#[extendr]
fn project_run_output(handle: Robj, name: Raw) -> List {
    reply(|| read_member(&handle, &name, |run| table(run.output())))
}

#[extendr]
fn project_run_member_issues(handle: Robj, name: Raw) -> List {
    reply(|| read_member(&handle, &name, |run| table(Some(&run.issues()))))
}

#[extendr]
fn project_run_member_report(handle: Robj, name: Raw) -> List {
    reply(|| read_member(&handle, &name, |run| table(run.report())))
}

#[extendr]
fn project_run_member_timings(handle: Robj, name: Raw) -> List {
    reply(|| read_member(&handle, &name, |run| table(Some(&run.timings().table()))))
}

/// One member's printed lines, its run's under its name (D37).
#[extendr]
fn project_run_member_summary(handle: Robj, name: Raw) -> List {
    reply(|| read_member(&handle, &name, |run| Ok(lines(&run.summary()))))
}

#[extendr]
fn project_run_issues(handle: Robj) -> List {
    reply(|| read_project_run(&handle, |runs| table(Some(&runs.issues()))))
}

#[extendr]
fn project_run_report(handle: Robj) -> List {
    reply(|| read_project_run(&handle, |runs| table(Some(&runs.report()))))
}

#[extendr]
fn project_run_timings(handle: Robj) -> List {
    reply(|| read_project_run(&handle, |runs| table(Some(&runs.timings().table()))))
}

#[extendr]
fn project_run_save(handle: Robj) -> List {
    reply(|| {
        let value = held(&handle, HandleKind::ProjectRun, "project run")?;
        save(|token| match &*value {
            Value::ProjectRun(runs) => Ok(locked(runs, "project run")?.save(token)?),
            _ => Err(Refusal::State("unknown project run handle".into())),
        })
    })
}

/// The printed lines, with one line per member (D37).
#[extendr]
fn project_run_summary(handle: Robj) -> List {
    reply(|| read_project_run(&handle, |runs| Ok(lines(&runs.summary()))))
}

#[extendr]
fn project_submission(project: Raw, section: Robj, created: Robj, settings: List) -> List {
    reply(|| {
        let section = optional_text(&section)?;
        let created = optional_text(&created)?;
        let project = text(&project)?;
        let mut options = SubmissionOptions::new(yamaa::VERSION);
        options.created = created;
        let submission = called(&settings, false, |engine, token| {
            project_request(engine, &project, section.as_deref())
                .cancel_with(token)
                .submission(&options)
        })?;
        store(Value::Submission(Mutex::new(submission)))
    })
}

#[extendr]
fn submission_status(handle: Robj) -> List {
    reply(|| {
        Ok(r!(read_submission(&handle, |submission| submission
            .status()
            .as_str())?))
    })
}

/// The creation time the documents carry (D69, D129).
#[extendr]
fn submission_created(handle: Robj) -> List {
    reply(|| {
        let created = read_submission(&handle, |submission| submission.created().to_owned())?;
        Ok(r!(created))
    })
}

#[extendr]
fn submission_issues(handle: Robj) -> List {
    reply(|| table(Some(&read_submission(&handle, yamaa::Submission::issues)?)))
}

#[extendr]
fn submission_timings(handle: Robj) -> List {
    reply(|| {
        table(Some(&read_submission(&handle, |submission| {
            submission.timings().table()
        })?))
    })
}

#[extendr]
fn submission_save(handle: Robj) -> List {
    reply(|| {
        let value = held(&handle, HandleKind::Submission, "submission")?;
        save(|token| match &*value {
            Value::Submission(submission) => Ok(locked(submission, "submission")?.save(token)?),
            _ => Err(Refusal::State("unknown submission handle".into())),
        })
    })
}

#[extendr]
fn submission_summary(handle: Robj) -> List {
    reply(|| read_submission(&handle, |submission| lines(&submission.summary())))
}

/// Run every selftest case through this engine as the R host, with R's
/// `version`, such as `4.6.1` (#3030 D28, D119).
#[extendr]
fn selftest(version: Raw) -> List {
    reply(|| {
        let version = text(&version)?;
        store(Value::SelfTest(yamaa::selftest(
            yamaa::Host::R,
            Some(&version),
        )))
    })
}

#[extendr]
fn selftest_status(handle: Robj) -> List {
    reply(|| read_selftest(&handle, |selftest| Ok(r!(selftest.status().as_str()))))
}

#[extendr]
fn selftest_cases(handle: Robj) -> List {
    reply(|| read_selftest(&handle, |selftest| table(Some(&selftest.table()))))
}

/// The engine information that ran the cases, as `yamaa_info()` gives it.
#[extendr]
fn selftest_info(handle: Robj) -> List {
    reply(|| read_selftest(&handle, |selftest| Ok(info_list(selftest.info()))))
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
        let version = text(&version)?;
        Ok(info_list(&yamaa::info(yamaa::Host::R, Some(&version))))
    })
}

/// The printed report's lines (D37, D119), which `format()` returns.
#[extendr]
fn selftest_summary(handle: Robj) -> List {
    reply(|| read_selftest(&handle, |selftest| Ok(lines(&selftest.summary()))))
}

#[extendr]
fn empty_issues() -> List {
    reply(|| Ok(crate::public_table::frame(&yamaa::diagnostic::issues(&[]))?))
}

include!("generated/api_entry.rs");
