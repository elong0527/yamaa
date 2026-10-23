//! The test host of the engine's C ABI: the table negotiated once, safe calls
//! of its entries, a host whose callbacks each test chooses and which counts
//! the roots and replies it hands over and gets back, the census, the shared
//! host truth and scratch directories.
//!
//! The census counts for the whole process, and the tests of one binary run
//! in parallel, so every test that calls an entry holds [`serial`].
use std::cell::Cell;
use std::ffi::{c_int, c_void};
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};

use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::{Array, Int64Array, StringArray};
use serde_json::{json, Value as Json};
use yamaa_boundary::abi::{self, Handle, Str};
use yamaa_boundary::{HandleKind, Outcome, Value, BOUNDARY_VERSION, CAPABILITIES, FORMATS};

static TRUTH: LazyLock<Json> = LazyLock::new(|| {
    let truth: Json =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    assert_eq!(truth["version"], "1", "the installed-host fixture version");
    truth
});

/// One family of the shared host truth.
pub fn family(name: &str) -> &'static Json {
    let family = &TRUTH["families"][name];
    assert!(family.is_object(), "cases.json has no family {name}");
    family
}

/// The truth with its placeholders filled: `{version}` is the boundary
/// version, `{next_version}` the one after it and `{core_version}` the
/// engine's release.
pub fn resolved(value: &Json) -> Json {
    match value {
        Json::String(text) if text == "{version}" => json!(BOUNDARY_VERSION),
        Json::String(text) if text == "{next_version}" => json!(BOUNDARY_VERSION + 1),
        Json::String(text) if text == "{core_version}" => json!(env!("CARGO_PKG_VERSION")),
        Json::Array(items) => Json::Array(items.iter().map(resolved).collect()),
        Json::Object(fields) => Json::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), resolved(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A request as a host builds it from JSON: an object is a mapping, an array
/// a list, a whole number within 64 bits an integer, and a boolean, a
/// fraction or null no field of a request.
pub fn host_request(value: &Json) -> Value {
    match value {
        Json::Number(number) => number.as_i64().map_or(Value::Other, Value::Integer),
        Json::String(text) => Value::Text(text.clone()),
        Json::Array(items) => Value::List(items.iter().map(host_request).collect()),
        Json::Object(fields) => Value::Map(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), host_request(item)))
                .collect(),
        ),
        Json::Bool(_) | Json::Null => Value::Other,
    }
}

/// An outcome as the hosts return it: the accepted record or the envelope.
pub fn outcome_record(outcome: &Outcome) -> Json {
    match outcome {
        Outcome::Accepted(accepted) => json!({
            "accepted": true,
            "version": accepted.version,
            "enabled": accepted.enabled,
            "supported": accepted.supported,
            "core_version": accepted.core_version,
            "language_versions": accepted.language_versions,
        }),
        Outcome::Refused(refusal) => json!({
            "accepted": false,
            "requested_version": refusal.requested_version,
            "supported_version": refusal.supported_version,
            "reason": refusal.reason.as_str(),
            "missing_features": refusal.missing_features,
        }),
    }
}

/// A request of this boundary version for `required` and `optional`, which
/// the texts it borrows outlive.
pub fn request(required: &[Str], optional: &[Str]) -> abi::Request {
    abi::Request {
        size: size_of::<abi::Request>() as u32,
        version: BOUNDARY_VERSION,
        required: required.as_ptr(),
        required_len: required.len(),
        optional: optional.as_ptr(),
        optional_len: optional.len(),
    }
}

/// An outcome of `size` bytes, its fields zero.
pub fn outcome(size: usize) -> abi::Outcome {
    // SAFETY: an outcome is valid all zeros.
    let mut outcome: abi::Outcome = unsafe { MaybeUninit::zeroed().assume_init() };
    outcome.size = size as u32;
    outcome
}

/// The function table, negotiated once with the request the generated
/// layers send: the boundary version, the capabilities and the formats.
pub fn engine() -> &'static abi::Engine {
    static ENGINE: OnceLock<&'static abi::Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let required: Vec<Str> = CAPABILITIES.iter().map(|name| Str::of(name)).collect();
        let optional: Vec<Str> = FORMATS.iter().map(|(name, _)| Str::of(name)).collect();
        let mut answer = outcome(size_of::<abi::Outcome>());
        // SAFETY: the request and the outcome are valid for the call; an
        // accepted negotiation returns the static table.
        unsafe { yamaa_engine::negotiate(&request(&required, &optional), &mut answer).as_ref() }
            .expect("the generated request is accepted")
    })
}

/// Hold the census still while a test runs.
pub fn serial() -> MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What an entry answered.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer<T> {
    Done(T),
    Refused(String),
    Cancelled,
    Boundary(String),
}

impl<T: std::fmt::Debug> Answer<T> {
    /// The value of a call that must be done.
    pub fn done(self) -> T {
        match self {
            Answer::Done(value) => value,
            other => panic!("expected a done call, got {other:?}"),
        }
    }

    pub fn map<U>(self, convert: impl FnOnce(T) -> U) -> Answer<U> {
        match self {
            Answer::Done(value) => Answer::Done(convert(value)),
            Answer::Refused(message) => Answer::Refused(message),
            Answer::Cancelled => Answer::Cancelled,
            Answer::Boundary(message) => Answer::Boundary(message),
        }
    }
}

/// Call an entry with fresh destinations; a refusal's issues and paths are
/// released here.
pub fn answer<T>(entry: impl FnOnce(*mut T, *mut abi::Refusal) -> abi::Status) -> Answer<T> {
    let (mut out, mut refusal) = (MaybeUninit::uninit(), MaybeUninit::uninit());
    let status = entry(out.as_mut_ptr(), refusal.as_mut_ptr());
    let refused = || {
        // SAFETY: the entry wrote the refusal.
        let refusal: abi::Refusal = unsafe { refusal.assume_init_read() };
        release(refusal.issues);
        texts(refusal.paths);
        text(refusal.message)
    };
    match status {
        // SAFETY: the entry wrote its value.
        abi::DONE => Answer::Done(unsafe { out.assume_init() }),
        abi::REFUSED => Answer::Refused(refused()),
        abi::CANCELLED => Answer::Cancelled,
        abi::BOUNDARY => Answer::Boundary(refused()),
        other => panic!("an entry answered status {other}"),
    }
}

/// The items of a buffer the engine handed out, copied, then the buffer
/// released.
fn copied<T, U>(mut buffer: abi::Buffer<T>, copy: impl Fn(&T) -> U) -> Vec<U> {
    let items = match buffer.len {
        0 => Vec::new(),
        // SAFETY: a buffer's items stay valid until it is released.
        len => unsafe { std::slice::from_raw_parts(buffer.ptr, len) }
            .iter()
            .map(copy)
            .collect(),
    };
    if let Some(release) = buffer.release {
        // SAFETY: the engine handed the buffer out with this release.
        unsafe { release(&mut buffer) };
    }
    items
}

/// Borrowed text, copied.
fn owned(text: &Str) -> String {
    // SAFETY: the text is valid while its buffer or reply lives.
    let bytes = (text.len > 0).then(|| unsafe { std::slice::from_raw_parts(text.ptr, text.len) });
    String::from_utf8(bytes.unwrap_or_default().to_vec()).unwrap()
}

pub fn text(buffer: abi::Text) -> String {
    String::from_utf8(copied(buffer, |byte| *byte)).unwrap()
}

pub fn texts(buffer: abi::Texts) -> Vec<String> {
    copied(buffer, owned)
}

/// What the engine holds for its holders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Census {
    pub handles: Vec<(&'static str, u64)>,
    pub buffers: u64,
    pub streams: u64,
    pub arrays: u64,
}

impl Census {
    /// The same census with one more handle of `kind`.
    pub fn plus(&self, kind: &str) -> Census {
        let mut census = self.clone();
        for (name, held) in &mut census.handles {
            *held += u64::from(*name == kind);
        }
        census
    }

    /// The same census with `count` more exported streams.
    pub fn streams(&self, count: u64) -> Census {
        Census {
            streams: self.streams + count,
            ..self.clone()
        }
    }
}

pub fn census() -> Census {
    let mut record = MaybeUninit::<abi::Census>::uninit();
    // SAFETY: the census writes its record.
    assert_eq!(unsafe { (engine().census)(record.as_mut_ptr()) }, abi::DONE);
    // SAFETY: as above.
    let record = unsafe { record.assume_init() };
    Census {
        handles: HandleKind::ALL
            .iter()
            .zip(record.handles)
            .map(|(kind, count)| (kind.as_str(), count))
            .collect(),
        buffers: record.buffers,
        streams: record.streams,
        arrays: record.arrays,
    }
}

/// Give a handle back.
pub fn release(handle: Handle) -> abi::Status {
    // SAFETY: any handle may be given back.
    unsafe { (engine().release)(handle) }
}

/// What a project function answers for its text argument.
pub enum Invoked {
    /// The argument itself.
    Echo,
    /// Text of these bytes.
    Bytes(&'static [u8]),
    /// The host's interrupt, keeping its original object.
    Interrupt,
}

/// The roots a host makes: the callbacks' context, bound functions and kept
/// payloads.
const CONTEXT: usize = 0;
const FUNCTION: usize = 1;
const PAYLOAD: usize = 2;

/// A test host: its callbacks answer as the test chooses and count what they
/// hand over and get back. A call takes the host as its context.
pub struct Host<'a> {
    probe: Box<dyn Fn() -> c_int + 'a>,
    invoke: Box<dyn Fn(&str) -> Invoked + 'a>,
    /// The roots made and released, by what they hold.
    roots: [Cell<(usize, usize)>; 3],
    /// The replies made that point into the host's memory, and released.
    replies: Cell<(usize, usize)>,
}

/// Count `made` and `released` more.
fn count(cell: &Cell<(usize, usize)>, made: usize, released: usize) {
    let (was_made, was_released) = cell.get();
    cell.set((was_made + made, was_released + released));
}

/// What a root or a reply points at: what it holds, and the host that
/// counts it.
struct Mark {
    host: *const Host<'static>,
    kind: usize,
    _bytes: Vec<u8>,
}

impl<'a> Host<'a> {
    /// A host whose probe lets every call go on and whose project function
    /// echoes its argument.
    pub fn new() -> Self {
        Host {
            probe: Box::new(|| abi::GO_ON),
            invoke: Box::new(|_| Invoked::Echo),
            roots: Default::default(),
            replies: Cell::new((0, 0)),
        }
    }

    pub fn probing(self, probe: impl Fn() -> c_int + 'a) -> Self {
        let probe = Box::new(probe);
        Host { probe, ..self }
    }

    pub fn invoking(self, invoke: impl Fn(&str) -> Invoked + 'a) -> Self {
        let invoke = Box::new(invoke);
        Host { invoke, ..self }
    }

    /// The roots made and released, by what they hold.
    pub fn roots(&self) -> [(&'static str, (usize, usize)); 3] {
        let [context, function, payload] = &self.roots;
        [
            ("context", context.get()),
            ("function", function.get()),
            ("payload", payload.get()),
        ]
    }

    pub fn replies(&self) -> (usize, usize) {
        self.replies.get()
    }

    fn mark(&self, kind: usize, bytes: Vec<u8>) -> *mut c_void {
        let host = (self as *const Host<'a>).cast();
        Box::into_raw(Box::new(Mark {
            host,
            kind,
            _bytes: bytes,
        }))
        .cast()
    }

    fn root(&self, kind: usize) -> abi::Root {
        count(&self.roots[kind], 1, 0);
        let pointer = self.mark(kind, Vec::new());
        abi::Root {
            pointer,
            release: Some(release_root),
        }
    }

    /// A call from the Python host with this host's callbacks.
    pub fn call(&self) -> abi::Call {
        count(&self.roots[CONTEXT], 1, 0);
        abi::Call {
            context: abi::Root {
                pointer: (self as *const Host<'a>).cast_mut().cast(),
                release: Some(release_context),
            },
            probe: Some(probe),
            bind: Some(bind),
            invoke: Some(invoke),
            ..call()
        }
    }
}

/// A call from the Python host with no callbacks.
pub fn call() -> abi::Call {
    abi::Call {
        size: size_of::<abi::Call>() as u32,
        host: abi::HOST_PYTHON,
        runtime_name: Str::of("test"),
        runtime_version: Str::of("0"),
        context: abi::Root::NONE,
        probe: None,
        bind: None,
        invoke: None,
        call_batch: None,
        is_interrupt: None,
        providers: None,
        standard_library: None,
        distribution_version: None,
        environment: None,
        integer_digits: None,
        compare_locked: None,
    }
}

/// The host a callback's context is.
///
/// # Safety
///
/// `context` is a host's pointer, and the host outlives the call.
unsafe fn host<'a>(context: *mut c_void) -> &'a Host<'a> {
    // SAFETY: the caller's contract.
    unsafe { &*context.cast::<Host<'a>>() }
}

unsafe extern "C" fn release_context(context: *mut c_void) {
    // SAFETY: the context is the host, which outlives the call.
    count(&unsafe { host(context) }.roots[CONTEXT], 0, 1);
}

unsafe extern "C" fn release_root(pointer: *mut c_void) {
    // SAFETY: the pointer is a mark the host boxed, released once, and the
    // host outlives every root it made.
    let mark = unsafe { Box::from_raw(pointer.cast::<Mark>()) };
    count(&unsafe { &*mark.host }.roots[mark.kind], 0, 1);
}

unsafe extern "C" fn release_reply(reply: *mut abi::Reply) {
    // SAFETY: the reply's private data is a mark the host boxed.
    let mark = unsafe { Box::from_raw((*reply).private_data.cast::<Mark>()) };
    count(&unsafe { &*mark.host }.replies, 0, 1);
}

unsafe extern "C" fn probe(context: *mut c_void) -> c_int {
    // SAFETY: the engine passes the call's context.
    (unsafe { host(context) }.probe)()
}

unsafe extern "C" fn bind(
    context: *mut c_void,
    _: *const abi::Identity,
    _: *const abi::Signature,
    reply: *mut abi::Reply,
) -> c_int {
    // SAFETY: the engine passes the call's context and a fresh reply.
    unsafe { (*reply).function = host(context).root(FUNCTION) };
    abi::ANSWERED
}

unsafe extern "C" fn invoke(
    context: *mut c_void,
    _: *mut c_void,
    arguments: *const abi::Argument,
    arguments_len: usize,
    reply: *mut abi::Reply,
) -> c_int {
    // SAFETY: the engine passes the call's context, its arguments for the
    // call and a fresh reply, which the host fills.
    let (host, value, reply) = unsafe {
        let arguments = std::slice::from_raw_parts(arguments, arguments_len);
        (host(context), arguments[0].value, &mut *reply)
    };
    let argument = (value.kind == abi::SCALAR_TEXT).then(|| owned(&value.text));
    // A panic of the host's own code is contained here, as a binding's
    // callbacks contain theirs, and answered as a failure of its own.
    let invoked = catch_unwind(AssertUnwindSafe(|| {
        (host.invoke)(argument.as_deref().unwrap_or_default())
    }));
    let bytes = match (invoked, argument) {
        (Ok(Invoked::Echo), Some(text)) => text.into_bytes(),
        (Ok(Invoked::Echo), None) => return abi::ANSWERED,
        (Ok(Invoked::Bytes(bytes)), _) => bytes.to_vec(),
        (Ok(Invoked::Interrupt), _) => {
            let payload = host.root(PAYLOAD);
            let kind = abi::ERROR_INTERRUPTED;
            reply.error = abi::HostError {
                kind,
                payload,
                ..abi::HostError::NONE
            };
            return abi::RAISED;
        }
        (Err(_), _) => {
            let (kind, message) = (abi::ERROR_INTERNAL, Str::of("function_boundary"));
            reply.error = abi::HostError {
                kind,
                message,
                ..abi::HostError::NONE
            };
            return abi::RAISED;
        }
    };
    count(&host.replies, 1, 0);
    let text = Str {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    };
    reply.scalar = abi::Scalar {
        kind: abi::SCALAR_TEXT,
        text,
        ..abi::Scalar::MISSING
    };
    reply.release = Some(release_reply);
    reply.private_data = host.mark(0, bytes);
    abi::ANSWERED
}

/// A specification run with `call`.
pub fn spec_run(
    call: &abi::Call,
    spec: &str,
    project: Option<&str>,
    plans: Handle,
) -> Answer<Handle> {
    let (spec, project) = (Str::of(spec), Str::optional(project));
    // SAFETY: the arguments are valid for the call.
    answer(|out, refusal| unsafe { (engine().spec_run)(call, spec, project, plans, out, refusal) })
}

/// A result's status, or its member's.
pub fn status(result: Handle, member: Option<&str>) -> Answer<String> {
    let member = Str::optional(member);
    // SAFETY: the arguments are valid for the call.
    answer(|out, refusal| unsafe { (engine().status)(result, member, out, refusal) }).map(text)
}

/// The table one of `output`, `issues` and `report` answers for a result.
pub fn table(entry: abi::OnMember<Handle>, result: Handle) -> Answer<Handle> {
    // SAFETY: the arguments are valid for the call.
    answer(|out, refusal| unsafe { entry(result, Str::NONE, out, refusal) })
}

/// A fresh stream over a table.
pub fn export(table: Handle) -> Answer<abi::ArrowArrayStream> {
    answer(|out: *mut abi::ArrowArrayStream, refusal| {
        // SAFETY: the destination holds a released stream.
        unsafe {
            out.write(abi::ArrowArrayStream::released());
            (engine().export)(table, out, refusal)
        }
    })
}

/// A save with `call`.
pub fn save(call: &abi::Call, result: Handle) -> Answer<Vec<String>> {
    // SAFETY: the arguments are valid for the call.
    answer(|out, refusal| unsafe { (engine().save)(call, result, out, refusal) }).map(texts)
}

/// A host's reader over an exported stream, which takes it.
pub fn reader(stream: abi::ArrowArrayStream) -> ArrowArrayStreamReader {
    let mut stream = stream;
    // SAFETY: the engine's stream has the C stream interface's layout; the
    // reader moves it out and leaves it released.
    let stream = unsafe { FFI_ArrowArrayStream::from_raw((&raw mut stream).cast()) };
    ArrowArrayStreamReader::try_new(stream).unwrap()
}

/// The `columns` of each row a stream holds, as a JSON object per row of
/// integers, texts and nulls; reading takes the stream and releases it.
pub fn records(stream: abi::ArrowArrayStream, columns: &[&str]) -> Json {
    let mut records = Vec::new();
    for batch in reader(stream) {
        let batch = batch.unwrap();
        for row in 0..batch.num_rows() {
            let record = columns.iter().map(|name| {
                let column = batch.column_by_name(name).unwrap().as_any();
                let integers = column.downcast_ref::<Int64Array>();
                let texts = column.downcast_ref::<StringArray>();
                let value = match (integers, texts) {
                    (Some(array), _) if array.is_valid(row) => Json::from(array.value(row)),
                    (_, Some(array)) if array.is_valid(row) => Json::from(array.value(row)),
                    _ => Json::Null,
                };
                ((*name).to_owned(), value)
            });
            records.push(Json::Object(record.collect()));
        }
    }
    Json::Array(records)
}

/// The severity, phase, condition and rule of each issue of a result.
pub fn conditions(result: Handle) -> Json {
    let issues = table(engine().issues, result).done();
    let records = records(
        export(issues).done(),
        &["severity", "phase", "condition", "rule"],
    );
    release(issues);
    records
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory of a test's own, canonical and without a Windows
/// verbatim prefix, so its files are spelled as the engine spells the files
/// it reads and writes (D38). It is removed when the value drops.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-engine-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let canonical = std::fs::canonicalize(&directory).unwrap();
        let text = canonical.to_str().unwrap();
        Self(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text)))
    }

    /// Write `contents` to `name`, creating its parent directories.
    pub fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// A file in the directory, as the engine spells it: absolute, with
    /// forward slashes.
    pub fn spelled(&self, name: &str) -> String {
        format!("{}/{name}", self.0.to_str().unwrap().replace('\\', "/"))
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The shared truth's lifetime study, as the installed lifetime suites
/// write it: a specification of two rows, its input and a project that
/// lists it.
pub fn lifetime_study(label: &str) -> Scratch {
    let lifetime = &family("interface")["lifetime"];
    let scratch = Scratch::new(label);
    for (name, key) in [
        ("spec.yaml", "specification_yaml"),
        ("input.csv", "input_csv"),
        ("project.yaml", "project_yaml"),
    ] {
        scratch.write(name, lifetime[key].as_str().unwrap());
    }
    scratch
}

/// A project whose specification calls the project function `echo` for
/// each of its rows, which pass `alpha` and `beta`; the function's own
/// tests pass `a`, empty text and a missing value, which short-circuits.
pub fn function_study(label: &str) -> Scratch {
    let scratch = Scratch::new(label);
    scratch.write(
        "spec.yaml",
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\n\
         input: {SRC: {path: input.csv, key: [ID]}}\n\
         output: {path: out.csv, columns: [ID, VALUE]}\ncolumns:\n\
         \x20 - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n\
         \x20 - {name: VALUE, type: str, label: Value, derivation: \
         {function: {name: echo, args: {x: SRC.NAME}}}}\n",
    );
    scratch.write("input.csv", "ID,NAME\n1,alpha\n2,beta\n");
    scratch.write(
        "functions/echo.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: echo\ndescription: The value itself.\n\
         params: [{name: x, type: str}]\nreturns: str\nimplementations: {python: pkg.echo}\n\
         tests:\n\
         \x20 - {id: normal, covers: [normal], args: {x: a}, result: a}\n\
         \x20 - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n\
         \x20 - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    scratch.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/echo.yaml]\nadam: {specs: [spec.yaml]}\n",
    );
    scratch
}
