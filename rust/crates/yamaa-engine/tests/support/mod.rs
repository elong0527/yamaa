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
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};

use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::{Array, Int64Array, StringArray};
use serde_json::Value as Json;
use yamaa_boundary::abi::{self, Handle, Str};
use yamaa_boundary::{HandleKind, BOUNDARY_VERSION, CAPABILITIES, FORMATS};

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

/// The function table, negotiated once with the request the generated
/// layers send: the boundary version, the capabilities and the formats.
pub fn engine() -> &'static abi::Engine {
    static ENGINE: OnceLock<&'static abi::Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let required: Vec<Str> = CAPABILITIES.iter().map(|name| Str::of(name)).collect();
        let optional: Vec<Str> = FORMATS.iter().map(|(name, _)| Str::of(name)).collect();
        let request = abi::Request {
            size: size_of::<abi::Request>() as u32,
            version: BOUNDARY_VERSION,
            required: required.as_ptr(),
            required_len: required.len(),
            optional: optional.as_ptr(),
            optional_len: optional.len(),
        };
        let mut outcome = outcome(size_of::<abi::Outcome>());
        // SAFETY: the request and the outcome are valid for the call.
        let table = unsafe { yamaa_engine::negotiate(&request, &mut outcome) };
        // SAFETY: an accepted negotiation returns the static table.
        unsafe { table.as_ref() }.expect("the generated request is accepted")
    })
}

/// An outcome of `size` bytes, its fields zero.
pub fn outcome(size: usize) -> abi::Outcome {
    // SAFETY: an outcome is valid all zeros.
    let mut outcome: abi::Outcome = unsafe { MaybeUninit::zeroed().assume_init() };
    outcome.size = size as u32;
    outcome
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
}

/// Call an entry with fresh destinations; a refusal's issues and paths are
/// released here.
pub fn answer<T>(entry: impl FnOnce(*mut T, *mut abi::Refusal) -> abi::Status) -> Answer<T> {
    let mut out = MaybeUninit::<T>::uninit();
    let mut refusal = MaybeUninit::<abi::Refusal>::uninit();
    let status = entry(out.as_mut_ptr(), refusal.as_mut_ptr());
    let refused = || {
        // SAFETY: the entry wrote the refusal.
        let refusal = unsafe { refusal.assume_init_read() };
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

/// Release a buffer the engine handed out.
fn released<T>(mut buffer: abi::Buffer<T>) {
    if let Some(release) = buffer.release {
        // SAFETY: the engine handed the buffer out with this release.
        unsafe { release(&mut buffer) };
    }
}

/// The items of a buffer the engine handed out.
fn items<T>(buffer: &abi::Buffer<T>) -> &[T] {
    if buffer.len == 0 {
        &[]
    } else {
        // SAFETY: a buffer's items stay valid until it is released.
        unsafe { std::slice::from_raw_parts(buffer.ptr, buffer.len) }
    }
}

fn copied(text: Str) -> String {
    let bytes = if text.len == 0 {
        &[][..]
    } else {
        // SAFETY: the text borrows a buffer that is not released yet.
        unsafe { std::slice::from_raw_parts(text.ptr, text.len) }
    };
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Copy a text the engine handed out, and release it.
pub fn text(buffer: abi::Text) -> String {
    let text = String::from_utf8(items(&buffer).to_vec()).unwrap();
    released(buffer);
    text
}

/// Copy a list of texts the engine handed out, and release it.
pub fn texts(buffer: abi::Texts) -> Vec<String> {
    let texts = items(&buffer).iter().map(|item| copied(*item)).collect();
    released(buffer);
    texts
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
    /// The same census with `count` more handles of `kind`.
    pub fn plus(&self, kind: &str, count: u64) -> Census {
        let mut census = self.clone();
        for (name, held) in &mut census.handles {
            if *name == kind {
                *held += count;
            }
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

/// A test host: its callbacks answer as the test chooses and count what they
/// hand over and get back. A call takes the host as its context.
pub struct Host<'a> {
    probe: Box<dyn Fn() -> c_int + 'a>,
    invoke: Box<dyn Fn(&str) -> Invoked + 'a>,
    /// The roots made and released, by what they hold: the callbacks'
    /// context, bound functions and kept payloads.
    roots: [Cell<(usize, usize)>; 3],
    /// The replies made that point into the host's memory, and released.
    replies: Cell<(usize, usize)>,
}

const CONTEXT: usize = 0;
const FUNCTION: usize = 1;
const PAYLOAD: usize = 2;

/// A root the host made: what it holds, and the host that counts it.
struct Mark {
    host: *const Host<'static>,
    kind: usize,
}

fn counted(cell: &Cell<(usize, usize)>, made: usize, released: usize) {
    let (was_made, was_released) = cell.get();
    cell.set((was_made + made, was_released + released));
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
        Host {
            probe: Box::new(probe),
            ..self
        }
    }

    pub fn invoking(self, invoke: impl Fn(&str) -> Invoked + 'a) -> Self {
        Host {
            invoke: Box::new(invoke),
            ..self
        }
    }

    /// The roots made and released: the context, bound functions and kept
    /// payloads.
    pub fn roots(&self) -> [(&'static str, (usize, usize)); 3] {
        [
            ("context", self.roots[CONTEXT].get()),
            ("function", self.roots[FUNCTION].get()),
            ("payload", self.roots[PAYLOAD].get()),
        ]
    }

    pub fn replies(&self) -> (usize, usize) {
        self.replies.get()
    }

    fn root(&self, kind: usize) -> abi::Root {
        counted(&self.roots[kind], 1, 0);
        let mark = Mark {
            host: (self as *const Host<'a>).cast(),
            kind,
        };
        abi::Root {
            pointer: Box::into_raw(Box::new(mark)).cast(),
            release: Some(release_root),
        }
    }

    /// A call from the Python host with this host's callbacks.
    pub fn call(&self) -> abi::Call {
        counted(&self.roots[CONTEXT], 1, 0);
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
    counted(&unsafe { host(context) }.roots[CONTEXT], 0, 1);
}

unsafe extern "C" fn release_root(pointer: *mut c_void) {
    // SAFETY: the pointer is a mark the host boxed, released once.
    let mark = unsafe { Box::from_raw(pointer.cast::<Mark>()) };
    // SAFETY: the host outlives every root it made.
    counted(&unsafe { &*mark.host }.roots[mark.kind], 0, 1);
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

/// The memory a text reply points into, and the host that counts it.
struct ReplyText {
    host: *const Host<'static>,
    _bytes: Vec<u8>,
}

unsafe extern "C" fn release_reply(reply: *mut abi::Reply) {
    // SAFETY: the reply's private data is the text the host boxed.
    let text = unsafe { Box::from_raw((*reply).private_data.cast::<ReplyText>()) };
    // SAFETY: the host outlives the call.
    counted(&unsafe { &*text.host }.replies, 0, 1);
}

unsafe extern "C" fn invoke(
    context: *mut c_void,
    _: *mut c_void,
    arguments: *const abi::Argument,
    arguments_len: usize,
    reply: *mut abi::Reply,
) -> c_int {
    // SAFETY: the engine passes the call's context.
    let host = unsafe { host(context) };
    // SAFETY: the engine passes its arguments for the call.
    let value = unsafe { std::slice::from_raw_parts(arguments, arguments_len) }[0].value;
    let argument = (value.kind == abi::SCALAR_TEXT).then(|| copied(value.text));
    // A panic of the host's own code is contained here, as a binding's
    // callbacks contain theirs, and answered as its failure.
    let invoked = catch_unwind(AssertUnwindSafe(|| {
        (host.invoke)(argument.as_deref().unwrap_or_default())
    }));
    // SAFETY: the engine passes a fresh reply, which the host fills.
    let reply = unsafe { &mut *reply };
    let bytes = match (invoked, argument) {
        (Ok(Invoked::Echo), Some(text)) => text.into_bytes(),
        (Ok(Invoked::Echo), None) => return abi::ANSWERED,
        (Ok(Invoked::Bytes(bytes)), _) => bytes.to_vec(),
        (Ok(Invoked::Interrupt), _) => {
            reply.error = abi::HostError {
                kind: abi::ERROR_INTERRUPTED,
                payload: host.root(PAYLOAD),
                ..abi::HostError::NONE
            };
            return abi::RAISED;
        }
        (Err(_), _) => {
            reply.error = abi::HostError {
                kind: abi::ERROR_INTERNAL,
                message: Str::of("function_boundary"),
                ..abi::HostError::NONE
            };
            return abi::RAISED;
        }
    };
    counted(&host.replies, 1, 0);
    reply.scalar = abi::Scalar {
        kind: abi::SCALAR_TEXT,
        text: Str {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        },
        ..abi::Scalar::MISSING
    };
    reply.release = Some(release_reply);
    reply.private_data = Box::into_raw(Box::new(ReplyText {
        host: (host as *const Host<'_>).cast(),
        _bytes: bytes,
    }))
    .cast();
    abi::ANSWERED
}

/// A specification run with `call`.
pub fn spec_run(
    call: &abi::Call,
    specification: &str,
    project: Option<&str>,
    plans: Handle,
) -> Answer<Handle> {
    answer(|out, refusal| {
        // SAFETY: the arguments are valid for the call.
        unsafe {
            (engine().spec_run)(
                call,
                Str::of(specification),
                Str::optional(project),
                plans,
                out,
                refusal,
            )
        }
    })
}

/// A result's status, or its member's.
pub fn status(result: Handle, member: Option<&str>) -> Answer<String> {
    // SAFETY: the arguments are valid for the call.
    answer(|out, refusal| unsafe { (engine().status)(result, Str::optional(member), out, refusal) })
        .map(text)
}

/// One of the accessors that answer a table handle: `output`, `issues` or
/// `report`.
pub type TableEntry =
    unsafe extern "C" fn(Handle, Str, *mut Handle, *mut abi::Refusal) -> abi::Status;

pub fn table(entry: TableEntry, result: Handle) -> Answer<Handle> {
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

impl<T> Answer<T> {
    pub fn map<U>(self, convert: impl FnOnce(T) -> U) -> Answer<U> {
        match self {
            Answer::Done(value) => Answer::Done(convert(value)),
            Answer::Refused(message) => Answer::Refused(message),
            Answer::Cancelled => Answer::Cancelled,
            Answer::Boundary(message) => Answer::Boundary(message),
        }
    }
}

/// A host's reader over an exported stream, which takes it.
pub fn reader(stream: abi::ArrowArrayStream) -> ArrowArrayStreamReader {
    let mut stream = stream;
    // SAFETY: the engine's stream has the C stream interface's layout; the
    // reader moves it out and leaves it released.
    let stream = unsafe { FFI_ArrowArrayStream::from_raw((&mut stream as *mut abi::ArrowArrayStream).cast()) };
    ArrowArrayStreamReader::try_new(stream).unwrap()
}

/// The `columns` of each row a stream holds, as a JSON object per row:
/// integers, texts and nulls. Reading takes the stream and releases it.
pub fn records(stream: abi::ArrowArrayStream, columns: &[&str]) -> Vec<Json> {
    let mut records = Vec::new();
    for batch in reader(stream) {
        let batch = batch.unwrap();
        for row in 0..batch.num_rows() {
            let record = columns.iter().map(|name| {
                let column = batch.column_by_name(name).unwrap();
                let value = if column.is_null(row) {
                    Json::Null
                } else if let Some(integers) = column.as_any().downcast_ref::<Int64Array>() {
                    Json::from(integers.value(row))
                } else {
                    let texts = column.as_any().downcast_ref::<StringArray>().unwrap();
                    Json::from(texts.value(row))
                };
                ((*name).to_owned(), value)
            });
            records.push(Json::Object(record.collect()));
        }
    }
    records
}

/// The severity, phase, condition and rule of each issue of a result.
pub fn conditions(result: Handle) -> Vec<Json> {
    let issues = table(engine().issues, result).done();
    let records = records(export(issues).done(), &["severity", "phase", "condition", "rule"]);
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
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
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
         \x20 - {name: VALUE, type: str, label: Value, derivation: {function: {name: echo, args: {x: SRC.NAME}}}}\n",
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
