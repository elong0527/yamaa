//! The function table's entries, the memory they hand out, the Arrow stream
//! export and the negotiation entry.
//!
//! # Safety
//!
//! Every unsafe operation here reads what a host passes to an entry or
//! writes what the entry answers, under the contract of
//! [`yamaa_boundary::abi`]: each text, list and record argument is valid for
//! reads for the call, `out` and `refusal` are valid for writes, and a call
//! or a request holds at least `size` readable bytes. A buffer's, stream's,
//! schema's or array's `release` receives the struct the engine wrote,
//! wherever its holder moved it, and runs once: it rebuilds and drops exactly
//! the box the engine leaked into the struct's `private_data`.
use std::any::Any;
use std::ffi::{c_char, c_int, c_void};
use std::mem::{offset_of, MaybeUninit};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use yamaa::{Action, CancellationToken, ContextValue, EngineOptions, HostLanguage};
use yamaa_boundary::abi::{
    self, ArrowArray, ArrowArrayStream, ArrowSchema, Buffer, Call, Handle, Refusal, Status, Str,
};
use yamaa_boundary::{EngineFacts, HandleKind, Reason, Value as Requested, BOUNDARY_VERSION};

use crate::handles::{self, Held, Value};
use crate::host;

/// The buffers, exported streams and emitted schemas and arrays that their
/// holders have not released yet.
static BUFFERS: AtomicU64 = AtomicU64::new(0);
static STREAMS: AtomicU64 = AtomicU64::new(0);
static ARRAYS: AtomicU64 = AtomicU64::new(0);

/// Why an entry answers no value.
enum Fault {
    Refused(yamaa::Failure),
    Cancelled,
    Boundary(String),
}

impl From<yamaa::Failure> for Fault {
    fn from(failure: yamaa::Failure) -> Self {
        Fault::Refused(failure)
    }
}

type Answer<T> = Result<T, Fault>;

fn boundary(reason: impl Into<String>) -> Fault {
    Fault::Boundary(reason.into())
}

/// The refusal of `action` for `error` (D39), or the cancellation.
fn failed(action: Action<'_>, error: yamaa::Error) -> Fault {
    match error {
        yamaa::Error::Cancelled => Fault::Cancelled,
        error => Fault::Refused(yamaa::Failure::from_error(action, &error)),
    }
}

// -- arguments ----------------------------------------------------------------

/// The items of a list argument; a list of no items may be null.
///
/// # Safety
///
/// A non-null `items` points at `len` readable items.
unsafe fn list<'a, T>(items: *const T, len: usize, name: &str) -> Answer<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if items.is_null() || len > isize::MAX as usize / size_of::<T>().max(1) {
        return Err(boundary(format!("{name} is not a list")));
    }
    // SAFETY: the caller's contract.
    Ok(unsafe { std::slice::from_raw_parts(items, len) })
}

/// The text of an optional argument.
///
/// # Safety
///
/// A non-null `text.ptr` points at `text.len` readable bytes.
unsafe fn optional<'a>(text: Str, name: &str) -> Answer<Option<&'a str>> {
    if text.ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller's contract.
    let bytes = unsafe { list(text.ptr, text.len, name) }?;
    std::str::from_utf8(bytes)
        .map(Some)
        .map_err(|_| boundary(format!("{name} is not UTF-8")))
}

/// The text of a required argument.
///
/// # Safety
///
/// As [`optional`].
unsafe fn required<'a>(text: Str, name: &str) -> Answer<&'a str> {
    // SAFETY: the caller's contract.
    unsafe { optional(text, name) }?.ok_or_else(|| boundary(format!("{name} is missing")))
}

/// One call's options: its host, runtime and callbacks, whose context it
/// holds from the moment it is read.
struct CallArgs {
    language: Option<HostLanguage>,
    runtime_name: String,
    runtime_version: String,
    callbacks: Arc<host::Host>,
}

impl CallArgs {
    /// Read `call` up to the caller's size, a field past it reading as null,
    /// and take its context whatever happens next.
    ///
    /// # Safety
    ///
    /// `call` is null or points at a record of at least `size` readable
    /// bytes, whose texts are valid.
    unsafe fn read(call: *const Call) -> Answer<CallArgs> {
        if call.is_null() {
            return Err(boundary("the call is missing"));
        }
        let mut record = MaybeUninit::<Call>::zeroed();
        // SAFETY: the caller's contract. The copy stops at a multiple of the
        // pointer's alignment, so it copies whole fields only: `size` and
        // `host` fill the first such span together, and every later field
        // begins and ends on one. Each field is valid all zeros or as the
        // caller wrote it.
        let mut record = unsafe {
            let size = ((*call).size as usize).min(size_of::<Call>());
            let length = size / align_of::<usize>() * align_of::<usize>();
            std::ptr::copy_nonoverlapping(call.cast::<u8>(), record.as_mut_ptr().cast(), length);
            record.assume_init()
        };
        let callbacks = host::Host::adopt(&mut record);
        let language = match record.host {
            abi::HOST_NONE => None,
            abi::HOST_PYTHON => Some(HostLanguage::Python),
            abi::HOST_R => Some(HostLanguage::R),
            other => {
                return Err(boundary(format!(
                    "the call names host {other}, which is none"
                )))
            }
        };
        // SAFETY: the caller's contract.
        let (name, version) = unsafe {
            (
                required(record.runtime_name, "the runtime name")?,
                required(record.runtime_version, "the runtime version")?,
            )
        };
        Ok(CallArgs {
            language,
            runtime_name: name.to_owned(),
            runtime_version: version.to_owned(),
            callbacks,
        })
    }

    /// The engine of the call, as both bindings build one for each call.
    fn engine(&self) -> yamaa::Engine {
        let (name, version) = (self.runtime_name.clone(), self.runtime_version.clone());
        let options = match self.language {
            Some(language) => EngineOptions::new(language, name, version),
            None => EngineOptions::neutral(name, version),
        };
        yamaa::Engine::new(match self.callbacks.port() {
            Some(port) => options.with_functions(port),
            None => options,
        })
    }

    /// The host `info()` and `selftest()` report: the command line's for a
    /// call that claims no host language (D123).
    fn host(&self) -> (yamaa::Host, Option<&str>) {
        let version = Some(self.runtime_version.as_str());
        match self.language {
            Some(HostLanguage::Python) => (yamaa::Host::Python, version),
            Some(HostLanguage::R) => (yamaa::Host::R, version),
            _ => (yamaa::Host::Cli, None),
        }
    }

    /// Run a cancellable operation under the call's token. Once it returns,
    /// the token is polled once more, so an interrupt that arrived as the
    /// call returned still cancels it (D186), and a cancelled call answers
    /// nothing else.
    fn cancellable<T>(&self, run: impl FnOnce(&CancellationToken) -> Answer<T>) -> Answer<T> {
        let token = self.callbacks.token();
        let answer = run(&token);
        if token.is_cancelled() {
            return Err(Fault::Cancelled);
        }
        answer
    }
}

// -- what the engine hands out -------------------------------------------------

/// What a buffer's `private_data` owns: the memory its items live in.
/// Moving an owner moves none of the heap memory its items borrow.
trait Owner<T>: 'static {
    fn items(&self) -> &[T];
}

impl Owner<u8> for String {
    fn items(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Texts and the items that borrow them.
struct TextList {
    _texts: Vec<String>,
    items: Vec<Str>,
}

impl Owner<Str> for TextList {
    fn items(&self) -> &[Str] {
        &self.items
    }
}

/// A nested value, the spellings made for it, and its nodes.
struct Tree {
    _value: ContextValue,
    _spellings: Vec<String>,
    nodes: Vec<abi::Node>,
}

impl Owner<abi::Node> for Tree {
    fn items(&self) -> &[abi::Node] {
        &self.nodes
    }
}

/// The engine information, its lists and its record.
struct InfoRecord {
    _info: yamaa::Info,
    _lists: [Vec<Str>; 2],
    record: [abi::InfoRecord; 1],
}

impl Owner<abi::InfoRecord> for InfoRecord {
    fn items(&self) -> &[abi::InfoRecord] {
        &self.record
    }
}

/// Hand `owner`'s items out as a buffer, which owns it until released.
fn handed<T, O: Owner<T>>(owner: O) -> Buffer<T> {
    let owner = Box::new(owner);
    let (ptr, len) = (owner.items().as_ptr(), owner.items().len());
    BUFFERS.fetch_add(1, Ordering::Relaxed);
    Buffer {
        ptr,
        len,
        release: Some(release_buffer::<T, O>),
        private_data: Box::into_raw(owner).cast(),
    }
}

unsafe extern "C" fn release_buffer<T, O: Owner<T>>(buffer: *mut Buffer<T>) {
    // SAFETY: see the module documentation.
    let Some(buffer) = (unsafe { buffer.as_mut() }) else {
        return;
    };
    if buffer.release.take().is_none() || buffer.private_data.is_null() {
        return;
    }
    // SAFETY: `private_data` is the box `handed` leaked, released once.
    drop(unsafe { Box::from_raw(buffer.private_data.cast::<O>()) });
    (buffer.ptr, buffer.len, buffer.private_data) = (std::ptr::null(), 0, std::ptr::null_mut());
    BUFFERS.fetch_sub(1, Ordering::Relaxed);
}

fn text(text: impl Into<String>) -> abi::Text {
    handed(text.into())
}

fn texts<S: Into<String>>(texts: impl IntoIterator<Item = S>) -> abi::Texts {
    let texts: Vec<String> = texts.into_iter().map(Into::into).collect();
    let items = texts.iter().map(|text| Str::of(text)).collect();
    handed(TextList {
        _texts: texts,
        items,
    })
}

/// `value`'s nodes in pre-order, borrowing its texts.
fn tree(value: ContextValue) -> Answer<abi::Nodes> {
    fn flatten(
        value: &ContextValue,
        key: Str,
        nodes: &mut Vec<abi::Node>,
        spellings: &mut Vec<String>,
    ) -> Answer<()> {
        let (mut kind, mut int, mut float, mut text) = (abi::NODE_NULL, 0, 0.0, Str::NONE);
        let mut children: Vec<(Str, &ContextValue)> = Vec::new();
        match value {
            ContextValue::Null => {}
            ContextValue::Bool(value) => (kind, int) = (abi::NODE_BOOL, i64::from(*value)),
            ContextValue::Int(value) => (kind, int) = (abi::NODE_INT, *value),
            ContextValue::Uint(value) => match i64::try_from(*value) {
                Ok(value) => (kind, int) = (abi::NODE_INT, value),
                Err(_) => {
                    spellings.push(value.to_string());
                    (kind, text) = (abi::NODE_NUMBER, Str::of(&spellings[spellings.len() - 1]));
                }
            },
            ContextValue::Wide(spelling) => (kind, text) = (abi::NODE_NUMBER, Str::of(spelling)),
            ContextValue::Float(value) => (kind, float) = (abi::NODE_FLOAT, *value),
            ContextValue::Text(value) => (kind, text) = (abi::NODE_TEXT, Str::of(value)),
            ContextValue::List(items) => {
                kind = abi::NODE_LIST;
                children = items.iter().map(|item| (Str::NONE, item)).collect();
            }
            ContextValue::Map(entries) => {
                kind = abi::NODE_MAP;
                children = entries
                    .iter()
                    .map(|(key, item)| (Str::of(key), item))
                    .collect();
            }
            _ => return Err(boundary("the resolved value holds a kind no node has")),
        }
        let children_len = children.len();
        nodes.push(abi::Node {
            kind,
            key,
            children: children_len,
            int,
            float,
            text,
        });
        children
            .into_iter()
            .try_for_each(|(key, item)| flatten(item, key, nodes, spellings))
    }
    let (mut nodes, mut spellings) = (Vec::new(), Vec::new());
    flatten(&value, Str::NONE, &mut nodes, &mut spellings)?;
    Ok(handed(Tree {
        _value: value,
        _spellings: spellings,
        nodes,
    }))
}

/// The engine information as one record, borrowing it.
fn information(info: yamaa::Info) -> abi::Info {
    let language_versions: Vec<Str> = info.language_versions.iter().map(|v| Str::of(v)).collect();
    let features: Vec<Str> = info
        .features
        .iter()
        .map(|feature| Str::of(feature))
        .collect();
    let record = abi::InfoRecord {
        version: Str::of(info.version),
        language_versions: language_versions.as_ptr(),
        language_versions_len: language_versions.len(),
        features: features.as_ptr(),
        features_len: features.len(),
        host: Str::of(info.host.as_str()),
        host_version: Str::optional(info.host_version.as_deref()),
        platform: Str::of(&info.platform),
    };
    handed(InfoRecord {
        _info: info,
        _lists: [language_versions, features],
        record: [record],
    })
}

// -- answering ------------------------------------------------------------------

/// Run an entry's `body`, containing a panic, and write what it answers:
/// its value into `out`, or its refusal. An entry with nowhere to write its
/// value answers a boundary refusal, written when `refusal` is not null, and
/// one with nowhere to write a refusal answers [`abi::BOUNDARY`] alone;
/// either way its body does not run, and what the body holds drops, a call's
/// context included.
///
/// # Safety
///
/// `out` and `refusal` are null or valid for writes.
unsafe fn guard<T>(out: *mut T, refusal: *mut Refusal, body: impl FnOnce() -> Answer<T>) -> Status {
    if refusal.is_null() {
        return abi::BOUNDARY;
    }
    let answer = if out.is_null() {
        Err(boundary("the entry's destination is null"))
    } else {
        catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|panic| Err(contained(&*panic)))
    };
    let (status, record) = match answer {
        Ok(value) => {
            // SAFETY: the caller's contract; `out` is not null.
            unsafe { out.write(value) };
            return abi::DONE;
        }
        Err(Fault::Cancelled) => return abi::CANCELLED,
        Err(Fault::Refused(failure)) => (
            abi::REFUSED,
            Refusal {
                message: text(failure.message),
                issues: handles::insert(Value::Table(failure.issues)).unwrap_or(0),
                paths: texts(failure.paths),
            },
        ),
        Err(Fault::Boundary(reason)) => (
            abi::BOUNDARY,
            Refusal {
                message: text(reason),
                issues: 0,
                paths: Buffer::empty(),
            },
        ),
    };
    // SAFETY: the caller's contract; `refusal` is not null.
    unsafe { refusal.write(record) };
    status
}

/// The boundary refusal of a contained panic.
fn contained(panic: &(dyn Any + Send)) -> Fault {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("a panic");
    boundary(format!("internal engine failure: {message}"))
}

/// Define entries that answer their value into a last `out` argument and a
/// refusal into `refusal`, whose body runs inside [`guard`]. An entry whose
/// first argument is a call reads it before anything else, so that it takes
/// the call's context whatever it answers, and its body receives what it
/// read. A body reads its other arguments in unsafe blocks of its own.
macro_rules! entries {
    () => {};
    (fn $name:ident($call:ident: *const Call $(, $argument:ident: $type:ty)*) -> $out:ty
        $body:block $($rest:tt)*) => {
        unsafe extern "C" fn $name(
            $call: *const Call,
            $($argument: $type,)*
            out: *mut $out,
            refusal: *mut Refusal,
        ) -> Status {
            // SAFETY: the entry's contract: the call is null or holds at
            // least its size.
            let read = AssertUnwindSafe(|| unsafe { CallArgs::read($call) });
            let $call = catch_unwind(read).unwrap_or_else(|panic| Err(contained(&*panic)));
            let body = move || {
                let $call = $call?;
                $body
            };
            // SAFETY: the entry's contract: `out` and `refusal` are null or
            // valid for writes.
            unsafe { guard(out, refusal, body) }
        }
        entries! { $($rest)* }
    };
    (fn $name:ident($($argument:ident: $type:ty),*) -> $out:ty $body:block $($rest:tt)*) => {
        unsafe extern "C" fn $name(
            $($argument: $type,)*
            out: *mut $out,
            refusal: *mut Refusal,
        ) -> Status {
            let body = move || $body;
            // SAFETY: the entry's contract: `out` and `refusal` are null or
            // valid for writes.
            unsafe { guard(out, refusal, body) }
        }
        entries! { $($rest)* }
    };
}

// -- handles --------------------------------------------------------------------

fn insert(value: Value) -> Answer<Handle> {
    handles::insert(value).map_err(boundary)
}

/// The value of a live handle.
fn held(handle: Handle) -> Answer<Arc<Held>> {
    handles::get(handle).ok_or_else(|| boundary("expired native handle"))
}

fn another(entry: &str, kind: HandleKind) -> Fault {
    boundary(format!("{entry} does not take a {} handle", kind.as_str()))
}

/// A result, unless another call holds it.
fn locked<T>(value: &Mutex<T>, kind: HandleKind) -> Answer<MutexGuard<'_, T>> {
    value
        .try_lock()
        .map_err(|_| boundary(format!("{} handle is in use or poisoned", kind.as_str())))
}

/// A result, or a project run's member, as an accessor reads it.
enum View<'a> {
    Check(&'a yamaa::Check),
    Run(&'a yamaa::Run),
    ProjectRun(&'a yamaa::ProjectRun),
    Submission(&'a yamaa::Submission),
    SelfTest(&'a yamaa::SelfTest),
}

impl View<'_> {
    fn kind(&self) -> HandleKind {
        match self {
            View::Check(_) => HandleKind::Check,
            View::Run(_) => HandleKind::Run,
            View::ProjectRun(_) => HandleKind::ProjectRun,
            View::Submission(_) => HandleKind::Submission,
            View::SelfTest(_) => HandleKind::SelfTest,
        }
    }
}

/// Read the result `handle` names, or its member `member`, under its lock.
fn view<T>(
    handle: Handle,
    member: Option<&str>,
    entry: &str,
    read: impl FnOnce(View<'_>) -> Answer<T>,
) -> Answer<T> {
    let held = held(handle)?;
    let kind = held.value.kind();
    if member.is_some() && kind != HandleKind::ProjectRun {
        return Err(boundary(format!(
            "a {} handle has no members",
            kind.as_str()
        )));
    }
    match &held.value {
        Value::Check(check) => read(View::Check(&*locked(check, kind)?)),
        Value::Run(run) => read(View::Run(&*locked(run, kind)?)),
        Value::ProjectRun(runs) => {
            let runs = locked(runs, kind)?;
            match member {
                Some(name) => read(View::Run(runs.member(name)?)),
                None => read(View::ProjectRun(&runs)),
            }
        }
        Value::Submission(submission) => read(View::Submission(&*locked(submission, kind)?)),
        Value::SelfTest(selftest) => read(View::SelfTest(selftest)),
        _ => Err(another(entry, kind)),
    }
}

/// Read the table `handle` names.
fn table<T>(handle: Handle, entry: &str, read: impl FnOnce(&yamaa::Table) -> T) -> Answer<T> {
    let held = held(handle)?;
    match &held.value {
        Value::Table(table) => Ok(read(table)),
        other => Err(another(entry, other.kind())),
    }
}

/// Hand a table out as a new handle, or 0 for none.
fn handed_table(table: Option<&yamaa::Table>) -> Answer<Handle> {
    table.map_or(Ok(0), |table| insert(Value::Table(table.clone())))
}

/// Run a specification or a project, reusing the plans holder `plans`, or
/// none for 0; a holder is shared without a lock.
fn run<T>(
    plans: Handle,
    entry: &str,
    run: impl FnOnce(Option<&yamaa::Plans>) -> Answer<T>,
) -> Answer<T> {
    let held = if plans == 0 { None } else { Some(held(plans)?) };
    match held.as_deref().map(|held| &held.value) {
        None => run(None),
        Some(Value::Plans(plans)) => run(Some(plans)),
        Some(other) => Err(another(entry, other.kind())),
    }
}

// -- the entries ------------------------------------------------------------------

entries! {
    fn spec_check(call: *const Call, specification: Str, project: Str) -> Handle {
        // SAFETY: the entry's contract: its texts are valid for the call.
        let specification = unsafe { required(specification, "the specification") }?;
        // SAFETY: as above.
        let project = unsafe { optional(project, "the project") }?;
        let check = call.cancellable(|token| {
            call.engine()
                .spec_check(specification, project, token)
                .map_err(|error| failed(Action::Read(specification), error))
        })?;
        insert(Value::Check(Mutex::new(check)))
    }

    fn project_check(call: *const Call, project: Str) -> Handle {
        // SAFETY: the entry's contract: its text is valid for the call.
        let project = unsafe { required(project, "the project") }?;
        let check = call.cancellable(|token| {
            call.engine()
                .project_check(project, token)
                .map_err(|error| failed(Action::Read(project), error))
        })?;
        insert(Value::Check(Mutex::new(check)))
    }

    fn spec_run(call: *const Call, specification: Str, project: Str, plans: Handle) -> Handle {
        // SAFETY: the entry's contract: its texts are valid for the call.
        let specification = unsafe { required(specification, "the specification") }?;
        // SAFETY: as above.
        let project = unsafe { optional(project, "the project") }?;
        let built = run(plans, "spec_run", |plans| {
            call.cancellable(|token| {
                let engine = call.engine();
                match plans {
                    Some(plans) => engine.spec_run_reusing(specification, project, plans, token),
                    None => engine.spec_run(specification, project, token),
                }
                .map_err(|error| failed(Action::Read(specification), error))
            })
        })?;
        insert(Value::Run(Mutex::new(built)))
    }

    fn project_run(call: *const Call, project: Str, section: Str, plans: Handle) -> Handle {
        // SAFETY: the entry's contract: its texts are valid for the call.
        let project = unsafe { required(project, "the project") }?;
        // SAFETY: as above.
        let section = unsafe { optional(section, "the section") }?;
        let built = run(plans, "project_run", |plans| {
            call.cancellable(|token| {
                let engine = call.engine();
                match plans {
                    Some(plans) => engine.project_run_reusing(project, section, plans, token),
                    None => engine.project_run(project, section, token),
                }
                // Only an unknown section refuses the call with issues.
                .map_err(|error| failed(Action::Select(section.unwrap_or_default()), error))
            })
        })?;
        insert(Value::ProjectRun(Mutex::new(built)))
    }

    fn project_submission(call: *const Call, project: Str, section: Str, created: Str) -> Handle {
        // SAFETY: the entry's contract: its texts are valid for the call.
        let project = unsafe { required(project, "the project") }?;
        // SAFETY: as above.
        let section = unsafe { optional(section, "the section") }?;
        // SAFETY: as above.
        let created = unsafe { optional(created, "the creation time") }?;
        let submission = call.cancellable(|token| {
            call.engine()
                .project_submission(project, section, created, yamaa::VERSION, token)
                .map_err(|error| failed(Action::Select(section.unwrap_or_default()), error))
        })?;
        insert(Value::Submission(Mutex::new(submission)))
    }

    fn read_odm(call: *const Call, source: Str, options: *const abi::OdmOptions) -> Handle {
        // SAFETY: the entry's contract: its text is valid for the call.
        let source = unsafe { required(source, "the source") }?;
        let mut read = yamaa::OdmOptions::new();
        // SAFETY: the entry's contract: the options are null or valid for the
        // call.
        if let Some(options) = unsafe { options.as_ref() } {
            let limit = |value: u64| usize::try_from(value).unwrap_or(usize::MAX);
            read = read
                .with_max_archive_members(limit(options.max_archive_members))
                .with_max_expanded_bytes(options.max_expanded_bytes)
                .with_max_xml_depth(limit(options.max_xml_depth));
            // SAFETY: as above, for the options' text.
            if let Some(member) = unsafe { optional(options.archive_member, "the member") }? {
                read = read.with_archive_member(member);
            }
        }
        let items = call
            .engine()
            .read_odm(source, &read)
            .map_err(|error| failed(Action::Read(source), error))?;
        insert(Value::Table(items))
    }

    fn lint(paths: *const Str, paths_len: usize, fix: u32) -> Handle {
        // SAFETY: the entry's contract: the list and its texts are valid for
        // the call.
        let paths = unsafe { list(paths, paths_len, "the paths") }?
            .iter()
            // SAFETY: as above, for each text.
            .map(|path| unsafe { required(*path, "a path") }.map(str::to_owned))
            .collect::<Answer<Vec<_>>>()?;
        insert(Value::Table(yamaa::lint(&paths, fix != 0)?.table()))
    }

    fn selftest(call: *const Call) -> Handle {
        let (host, version) = call.host();
        insert(Value::SelfTest(yamaa::selftest(host, version)))
    }

    fn plans_new() -> Handle {
        insert(Value::Plans(yamaa::Plans::new()))
    }

    fn no_issues() -> Handle {
        insert(Value::Table(yamaa::Report::default().table()))
    }

    fn spec_resolve(call: *const Call, specification: Str, project: Str) -> abi::Nodes {
        // SAFETY: the entry's contract: its texts are valid for the call.
        let specification = unsafe { required(specification, "the specification") }?;
        // SAFETY: as above.
        let project = unsafe { optional(project, "the project") }?;
        tree(call.engine().spec_resolve(specification, project)?)
    }

    fn project_spec(call: *const Call, project: Str, name: Str) -> abi::Text {
        // SAFETY: the entry's contract: its texts are valid for the call.
        let project = unsafe { required(project, "the project") }?;
        // SAFETY: as above.
        let name = unsafe { required(name, "the name") }?;
        Ok(text(call.engine().project_spec(project, name)?))
    }

    fn project_names(call: *const Call, project: Str) -> abi::Texts {
        // SAFETY: the entry's contract: its text is valid for the call.
        let project = unsafe { required(project, "the project") }?;
        Ok(texts(call.engine().project_names(project)?))
    }

    fn spec_summary(specification: Str) -> abi::Texts {
        // SAFETY: the entry's contract: its text is valid for the call.
        let specification = unsafe { required(specification, "the specification") }?;
        Ok(texts(yamaa::Summary::spec(specification).lines()))
    }

    fn project_summary(project: Str) -> abi::Texts {
        // SAFETY: the entry's contract: its text is valid for the call.
        let project = unsafe { required(project, "the project") }?;
        Ok(texts(yamaa::Summary::project(project).lines()))
    }

    fn schema(file_type: Str) -> abi::Text {
        // SAFETY: the entry's contract: its text is valid for the call.
        let file_type = unsafe { required(file_type, "the file type") }?;
        Ok(text(yamaa::schema(file_type)?))
    }

    fn info(call: *const Call) -> abi::Info {
        let (host, version) = call.host();
        Ok(information(yamaa::info(host, version)))
    }

    fn status(result: Handle, member: Str) -> abi::Text {
        // SAFETY: the entry's contract: its text is valid for the call.
        let member = unsafe { optional(member, "the member") }?;
        view(result, member, "status", |view| {
            let status = match view {
                View::Check(check) => check.status(),
                View::Run(run) => run.status(),
                View::ProjectRun(runs) => runs.status(),
                View::Submission(submission) => submission.status(),
                View::SelfTest(selftest) => selftest.status(),
            };
            Ok(text(status.as_str()))
        })
    }

    fn issues(result: Handle, member: Str) -> Handle {
        // SAFETY: the entry's contract: its text is valid for the call.
        let member = unsafe { optional(member, "the member") }?;
        let issues = view(result, member, "issues", |view| match view {
            View::Check(check) => Ok(check.issues()),
            View::Run(run) => Ok(run.issues()),
            View::ProjectRun(runs) => Ok(runs.issues()),
            View::Submission(submission) => Ok(submission.issues()),
            other => Err(another("issues", other.kind())),
        })?;
        insert(Value::Table(issues))
    }

    fn summary(result: Handle, member: Str) -> abi::Texts {
        // SAFETY: the entry's contract: its text is valid for the call.
        let member = unsafe { optional(member, "the member") }?;
        view(result, member, "summary", |view| {
            let summary = match view {
                View::Check(check) => check.summary(),
                View::Run(run) => run.summary(),
                View::ProjectRun(runs) => runs.summary(),
                View::Submission(submission) => submission.summary(),
                View::SelfTest(selftest) => selftest.summary(),
            };
            Ok(texts(summary.lines()))
        })
    }

    fn output(result: Handle, member: Str) -> Handle {
        // SAFETY: the entry's contract: its text is valid for the call.
        let member = unsafe { optional(member, "the member") }?;
        view(result, member, "output", |view| match view {
            View::Run(run) => handed_table(run.output()),
            other => Err(another("output", other.kind())),
        })
    }

    fn report(result: Handle, member: Str) -> Handle {
        // SAFETY: the entry's contract: its text is valid for the call.
        let member = unsafe { optional(member, "the member") }?;
        view(result, member, "report", |view| match view {
            View::Run(run) => handed_table(run.report()),
            View::ProjectRun(runs) => handed_table(Some(&runs.report())),
            other => Err(another("report", other.kind())),
        })
    }

    fn save(call: *const Call, result: Handle) -> abi::Texts {
        let held = held(result)?;
        let kind = held.value.kind();
        let paths = call.cancellable(|token| {
            Ok(match &held.value {
                Value::Run(run) => locked(run, kind)?.save(token),
                Value::ProjectRun(runs) => locked(runs, kind)?.save(token),
                Value::Submission(submission) => locked(submission, kind)?.save(token),
                _ => return Err(another("save", kind)),
            }?)
        })?;
        Ok(texts(paths))
    }

    fn names(result: Handle) -> abi::Texts {
        view(result, None, "names", |view| match view {
            View::ProjectRun(runs) => Ok(texts(runs.names())),
            other => Err(another("names", other.kind())),
        })
    }

    fn created(result: Handle) -> abi::Text {
        view(result, None, "created", |view| match view {
            View::Submission(submission) => Ok(text(submission.created())),
            other => Err(another("created", other.kind())),
        })
    }

    fn observations(result: Handle, include_measurements: u32) -> abi::Text {
        view(result, None, "observations", |view| match view {
            View::Run(run) => Ok(run
                .observations(include_measurements != 0)
                .map_or_else(Buffer::empty, text)),
            other => Err(another("observations", other.kind())),
        })
    }

    fn cases(result: Handle) -> Handle {
        let cases = view(result, None, "cases", |view| match view {
            View::SelfTest(selftest) => Ok(selftest.table()),
            other => Err(another("cases", other.kind())),
        })?;
        insert(Value::Table(cases))
    }

    fn selftest_info(result: Handle) -> abi::Info {
        view(result, None, "selftest_info", |view| match view {
            View::SelfTest(selftest) => Ok(information(selftest.info().clone())),
            other => Err(another("selftest_info", other.kind())),
        })
    }

    fn rows(handle: Handle) -> u64 {
        table(handle, "rows", |table| table.rows() as u64)
    }

    fn columns(handle: Handle) -> abi::Texts {
        table(handle, "columns", |table| {
            texts(table.columns().iter().map(|field| field.name().as_str()))
        })
    }

    fn preview(handle: Handle) -> abi::Text {
        table(handle, "preview", |table| text(table.preview()))
    }
}

unsafe extern "C" fn export(
    handle: Handle,
    out: *mut ArrowArrayStream,
    refusal: *mut Refusal,
) -> Status {
    // SAFETY: the entry's contract: `out` is null or holds a stream, which
    // the export reads so as never to overwrite a live one.
    let live = unsafe { out.as_ref() }.is_some_and(|stream| stream.release.is_some());
    let body = move || {
        if live {
            return Err(boundary("the stream's destination holds a live stream"));
        }
        table(handle, "export", |table| exported(table.stream()))
    };
    // SAFETY: the entry's contract: `out` and `refusal` are null or valid
    // for writes.
    unsafe { guard(out, refusal, body) }
}

unsafe extern "C" fn release(handle: Handle) -> Status {
    // The value drops here, outside the table, which a release callback it
    // runs may enter again.
    match catch_unwind(|| drop(handles::release(handle))) {
        Ok(()) => abi::DONE,
        Err(_) => abi::BOUNDARY,
    }
}

unsafe extern "C" fn census(out: *mut abi::Census) -> Status {
    if out.is_null() {
        return abi::BOUNDARY;
    }
    let record = abi::Census {
        handles: handles::CENSUS.counts().map(|(_, count)| count as u64),
        buffers: BUFFERS.load(Ordering::Relaxed),
        streams: STREAMS.load(Ordering::Relaxed),
        arrays: ARRAYS.load(Ordering::Relaxed),
    };
    // SAFETY: the entry's contract.
    unsafe { out.write(record) };
    abi::DONE
}

// -- the Arrow stream export ----------------------------------------------------

/// What an exported stream owns: the facade's stream over the table's
/// shared immutable buffers, with its own cursor.
struct Exported(yamaa::FFI_ArrowArrayStream);

/// The facade's stream, exported with the engine's callbacks, which count
/// the stream and each schema and array it emits until each is released.
fn exported(inner: yamaa::FFI_ArrowArrayStream) -> ArrowArrayStream {
    STREAMS.fetch_add(1, Ordering::Relaxed);
    ArrowArrayStream {
        get_schema: Some(stream_schema),
        get_next: Some(stream_next),
        get_last_error: Some(stream_error),
        release: Some(stream_release),
        private_data: Box::into_raw(Box::new(Exported(inner))).cast(),
    }
}

/// The facade's stream inside a live exported stream.
///
/// # Safety
///
/// `stream` is a live stream that [`exported`] made.
unsafe fn inner<'a>(stream: *mut ArrowArrayStream) -> &'a mut yamaa::FFI_ArrowArrayStream {
    // SAFETY: the caller's contract.
    unsafe { &mut (*(*stream).private_data.cast::<Exported>()).0 }
}

/// The Arrow C error number for a stream with no such callback.
const EIO: c_int = 5;

unsafe extern "C" fn stream_schema(stream: *mut ArrowArrayStream, out: *mut ArrowSchema) -> c_int {
    // SAFETY: see the module documentation; the facade's stream fills the
    // consumer's `out`, which a successful call leaves live.
    unsafe {
        let inner = inner(stream);
        let code = inner.get_schema.map_or(EIO, |get| get(inner, out.cast()));
        if code == 0 {
            adopt(&mut *out);
        }
        code
    }
}

unsafe extern "C" fn stream_next(stream: *mut ArrowArrayStream, out: *mut ArrowArray) -> c_int {
    // SAFETY: as `stream_schema`; the stream's end leaves `out` released.
    unsafe {
        let inner = inner(stream);
        let code = inner.get_next.map_or(EIO, |get| get(inner, out.cast()));
        if code == 0 {
            adopt(&mut *out);
        }
        code
    }
}

unsafe extern "C" fn stream_error(stream: *mut ArrowArrayStream) -> *const c_char {
    // SAFETY: see the module documentation.
    unsafe {
        let inner = inner(stream);
        inner
            .get_last_error
            .map_or(std::ptr::null(), |get| get(inner))
    }
}

unsafe extern "C" fn stream_release(stream: *mut ArrowArrayStream) {
    // SAFETY: see the module documentation.
    let Some(stream) = (unsafe { stream.as_mut() }) else {
        return;
    };
    if stream.release.take().is_none() || stream.private_data.is_null() {
        return;
    }
    // SAFETY: `private_data` is the box `exported` leaked; dropping the
    // facade's stream releases it.
    drop(unsafe { Box::from_raw(stream.private_data.cast::<Exported>()) });
    stream.private_data = std::ptr::null_mut();
    STREAMS.fetch_sub(1, Ordering::Relaxed);
}

/// A schema or array a stream emits, whose release the export wraps.
trait Emitted: Sized {
    fn parts(
        &mut self,
    ) -> (
        &mut Option<unsafe extern "C" fn(*mut Self)>,
        &mut *mut c_void,
    );
}

impl Emitted for ArrowSchema {
    fn parts(
        &mut self,
    ) -> (
        &mut Option<unsafe extern "C" fn(*mut Self)>,
        &mut *mut c_void,
    ) {
        (&mut self.release, &mut self.private_data)
    }
}

impl Emitted for ArrowArray {
    fn parts(
        &mut self,
    ) -> (
        &mut Option<unsafe extern "C" fn(*mut Self)>,
        &mut *mut c_void,
    ) {
        (&mut self.release, &mut self.private_data)
    }
}

/// The facade's release and private data of an emitted struct.
struct Shell<T>(unsafe extern "C" fn(*mut T), *mut c_void);

/// Count a live emitted struct until it is released.
fn adopt<T: Emitted>(item: &mut T) {
    let (release, private_data) = item.parts();
    if let Some(inner) = release.take() {
        *private_data = Box::into_raw(Box::new(Shell(inner, *private_data))).cast();
        *release = Some(release_emitted::<T>);
        ARRAYS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe extern "C" fn release_emitted<T: Emitted>(item: *mut T) {
    // SAFETY: see the module documentation.
    let Some(emitted) = (unsafe { item.as_mut() }) else {
        return;
    };
    let (release, private_data) = emitted.parts();
    if release.take().is_none() || private_data.is_null() {
        return;
    }
    // SAFETY: `private_data` is the shell `adopt` leaked, released once; the
    // facade's own release then runs with its own private data back.
    unsafe {
        let Shell(inner, data) = *Box::from_raw((*private_data).cast::<Shell<T>>());
        (*release, *private_data) = (Some(inner), data);
        inner(item);
    }
    ARRAYS.fetch_sub(1, Ordering::Relaxed);
}

// -- the table and the negotiation ----------------------------------------------

static ENGINE: abi::Engine = abi::Engine {
    size: size_of::<abi::Engine>() as u32,
    version: BOUNDARY_VERSION,
    spec_check,
    project_check,
    spec_run,
    project_run,
    project_submission,
    read_odm,
    lint,
    selftest,
    plans_new,
    spec_resolve,
    project_spec,
    project_names,
    spec_summary,
    project_summary,
    schema,
    info,
    status,
    issues,
    summary,
    output,
    report,
    save,
    names,
    created,
    observations,
    cases,
    selftest_info,
    rows,
    columns,
    preview,
    export,
    release,
    census,
    no_issues,
};

/// What the engine reports when it accepts a request, with the texts the
/// outcome points at.
struct Facts {
    facts: EngineFacts,
    supported: Vec<Str>,
    language_versions: Vec<Str>,
}

// SAFETY: the texts point into `facts`, which never changes once made and
// lives in a static for the life of the process.
unsafe impl Send for Facts {}
// SAFETY: as above.
unsafe impl Sync for Facts {}

fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        let info = yamaa::info(yamaa::Host::Cli, None);
        let facts = EngineFacts::new(&info.features, yamaa::VERSION, &info.language_versions);
        let views = |texts: &[String]| texts.iter().map(|text| Str::of(text)).collect();
        Facts {
            supported: views(facts.supported()),
            language_versions: views(facts.language_versions()),
            facts,
        }
    })
}

/// A request as the boundary's check order reads it. A request too short to
/// hold its version, or of another boundary version, is read no further, so
/// an older engine never reads a newer request's lists.
///
/// # Safety
///
/// As [`negotiate`] asks of `request`.
unsafe fn requested(request: *const abi::Request) -> Requested {
    // SAFETY: the caller's contract; each field is read through the raw
    // pointer within the request's size, and the whole request is borrowed
    // only when its size covers this version's lists.
    unsafe {
        if request.is_null() || ((*request).size as usize) < offset_of!(abi::Request, required) {
            return Requested::Other;
        }
        let (size, version) = ((*request).size as usize, (*request).version);
        let mut fields = vec![("version".to_owned(), Requested::Integer(version.into()))];
        if version == BOUNDARY_VERSION && size >= size_of::<abi::Request>() {
            let request = &*request;
            for (name, items, len) in [
                ("required", request.required, request.required_len),
                ("optional", request.optional, request.optional_len),
            ] {
                let identifiers = match list(items, len, name) {
                    Ok(items) => Requested::List(
                        items
                            .iter()
                            .map(|item| match optional(*item, name) {
                                Ok(Some(text)) => Requested::Text(text.to_owned()),
                                _ => Requested::Other,
                            })
                            .collect(),
                    ),
                    Err(_) => Requested::Other,
                };
                fields.push((name.to_owned(), identifiers));
            }
        }
        Requested::Map(fields)
    }
}

/// Write `answer` into `outcome`, never past its size, and say whether it
/// records an acceptance. A request of this boundary version needs this
/// version's whole outcome, to read the supported identifiers in, so one
/// answered into less is malformed.
///
/// # Safety
///
/// As [`negotiate`] asks of `outcome`.
unsafe fn write_outcome(
    outcome: *mut abi::Outcome,
    answer: &yamaa_boundary::Outcome,
    facts: &Facts,
) -> bool {
    let (reason, requested, supported, this_version) = match answer {
        yamaa_boundary::Outcome::Accepted(accepted) => {
            (abi::REASON_NONE, accepted.version, accepted.version, true)
        }
        yamaa_boundary::Outcome::Refused(refusal) => (
            match refusal.reason {
                Reason::VersionMismatch => abi::REASON_VERSION_MISMATCH,
                Reason::MalformedRequest => abi::REASON_MALFORMED_REQUEST,
                Reason::MissingFeature => abi::REASON_MISSING_FEATURE,
                Reason::MissingInterface => abi::REASON_MISSING_INTERFACE,
            },
            refusal.requested_version.unwrap_or(0),
            refusal.supported_version.unwrap_or(0),
            refusal.reason == Reason::MissingFeature,
        ),
    };
    // SAFETY: the caller's contract; the fixed part, `size` to `reason`, is
    // written only within the caller's size, and the rest only when the
    // whole outcome is.
    unsafe {
        if outcome.is_null()
            || ((*outcome).size as usize) < offset_of!(abi::Outcome, reason) + size_of::<u32>()
        {
            return false;
        }
        let whole = (*outcome).size as usize >= size_of::<abi::Outcome>();
        let reason = if this_version && !whole {
            abi::REASON_MALFORMED_REQUEST
        } else {
            reason
        };
        (*outcome).accepted = u32::from(reason == abi::REASON_NONE);
        (*outcome).requested_version = requested;
        (*outcome).supported_version = supported;
        (*outcome).reason = reason;
        if this_version && whole {
            (*outcome).supported = facts.supported.as_ptr();
            (*outcome).supported_len = facts.supported.len();
            (*outcome).core_version = Str::of(facts.facts.core_version());
            (*outcome).language_versions = facts.language_versions.as_ptr();
            (*outcome).language_versions_len = facts.language_versions.len();
        }
        reason == abi::REASON_NONE
    }
}

/// Answer a negotiation request in the boundary's check order, and return
/// the function table on acceptance, or null on refusal. The outcome points
/// only at static data, and an engine answering another boundary version's
/// request reads only its header and writes only the outcome's fixed part.
/// The loadable build of stage 4 exports this entry as
/// `yamaa_engine_negotiate`, the one symbol that stays fixed across boundary
/// versions; in the linked build a binding calls it as a Rust function.
///
/// # Safety
///
/// `request` is null or points at a request of at least `size` readable
/// bytes, whose lists, which the engine reads only when the request's
/// version is this boundary version and its size covers them, point at
/// `required_len` and `optional_len` readable texts. `outcome` is null or
/// points at an outcome whose `size` is set and whose first `size` bytes are
/// writable.
pub unsafe extern "C" fn negotiate(
    request: *const abi::Request,
    outcome: *mut abi::Outcome,
) -> *const abi::Engine {
    let accepted = catch_unwind(AssertUnwindSafe(|| {
        let facts = facts();
        // SAFETY: the caller's contract.
        unsafe {
            let answer = yamaa_boundary::negotiate(&requested(request), &facts.facts);
            write_outcome(outcome, &answer, facts)
        }
    }));
    match accepted {
        Ok(true) => &ENGINE,
        _ => std::ptr::null(),
    }
}
