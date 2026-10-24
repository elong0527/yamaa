//! The safe client over the engine's C ABI, through which a host binding
//! reaches the engine (the design on #2870, sections 3.3 to 3.6): the gate
//! that holds the negotiated function table, handles each owned by one
//! value, typed calls of the table's entries, and a host's callbacks behind
//! the engine's callback ABI.
//!
//! A binding answers its generated layer's request with [`Gate::negotiate`],
//! which sends it to the engine as the C request a client sends and reads
//! the outcome back; the first acceptance opens the gate, which hands out
//! the [`Engine`] every other native call goes through. A binding that
//! links the engine passes its negotiation entry, `yamaa_engine::negotiate`.
//!
//! # Safety
//!
//! Every unsafe operation here calls an entry of the negotiated table, reads
//! what an entry wrote, or is one of the callbacks the engine calls. Their
//! soundness rests on the contract of [`crate::abi`]: the table an accepted
//! negotiation returns is static, an entry writes its `out` on done and its
//! `refusal` on a refusal, everything it hands out stays valid until its one
//! release. Ordinary calls borrow their callbacks only while the call runs.
//! Streamed calls take an owned, thread-safe callback context, kept through
//! the run and readers: their probe may run on a consuming thread, and project
//! functions run only on the originating thread.
use std::any::Any;
use std::ffi::{c_int, c_void};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::abi::{self, Handle, Str};
use crate::{Accepted, Closed, HandleKind, Outcome, Reason, Refusal, Value};

/// An engine's negotiation entry: `yamaa_engine::negotiate` when a binding
/// links the engine.
pub type Negotiate = unsafe extern "C" fn(
    request: *const abi::Request,
    outcome: *mut abi::Outcome,
) -> *const abi::Engine;

/// A binding's gate: closed until a negotiation is accepted, then holding
/// the engine's table for the life of the process; nothing closes it again.
/// A binding keeps one in a `static`, so a replaced native module starts
/// closed.
#[derive(Debug, Default)]
pub struct Gate {
    table: AtomicPtr<abi::Engine>,
}

impl Gate {
    /// A closed gate.
    pub const fn new() -> Self {
        Self {
            table: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// Answer `request` through the engine that `negotiate` reaches. The
    /// request crosses as a client sends it: a version field that is no
    /// positive 32-bit integer as 0, which the engine reads as malformed,
    /// and fields that are not exactly the version and two lists of texts as
    /// the request's header alone, which the engine reads no further than
    /// its version. The host derives the enabled and the missing identifiers
    /// from the supported ones. An acceptance opens the gate; a refusal, or a
    /// later acceptance, leaves it as it was. An accepted table that this
    /// client cannot read, of another boundary version or shorter than its
    /// own, leaves the gate closed and answers a version mismatch that names
    /// the table's version.
    ///
    /// # Safety
    ///
    /// `negotiate` is an engine's negotiation entry.
    pub unsafe fn negotiate(&self, request: &Value, negotiate: Negotiate) -> Outcome {
        let sent = Sent::of(request);
        let (required, optional) = (strs(&sent.required), strs(&sent.optional));
        let request = abi::Request {
            size: if sent.lists {
                size_of::<abi::Request>()
            } else {
                std::mem::offset_of!(abi::Request, required)
            } as u32,
            version: sent.version,
            required: required.as_ptr(),
            required_len: required.len(),
            optional: optional.as_ptr(),
            optional_len: optional.len(),
        };
        // SAFETY: an outcome is valid all zeros.
        let mut outcome: abi::Outcome = unsafe { MaybeUninit::zeroed().assume_init() };
        outcome.size = size_of::<abi::Outcome>() as u32;
        // SAFETY: the caller's contract; the request and the outcome are
        // valid for the call.
        let table = unsafe { negotiate(&request, &mut outcome) };
        // SAFETY: the outcome points only at static texts.
        let supported = unsafe { listed(outcome.supported, outcome.supported_len) };
        let answer = if outcome.accepted == 1 {
            let enabled = sent.required.iter().chain(
                sent.optional
                    .iter()
                    .filter(|identifier| supported.contains(identifier)),
            );
            Outcome::Accepted(Accepted {
                version: outcome.requested_version,
                enabled: enabled.cloned().collect(),
                // SAFETY: as above.
                core_version: unsafe { listed(&outcome.core_version, 1) }.concat(),
                // SAFETY: as above.
                language_versions: unsafe {
                    listed(outcome.language_versions, outcome.language_versions_len)
                },
                supported,
            })
        } else {
            let reason = match outcome.reason {
                abi::REASON_VERSION_MISMATCH => Reason::VersionMismatch,
                abi::REASON_MISSING_FEATURE => Reason::MissingFeature,
                abi::REASON_MISSING_INTERFACE => Reason::MissingInterface,
                _ => Reason::MalformedRequest,
            };
            let missing_features = match reason {
                Reason::MissingFeature => sent
                    .required
                    .iter()
                    .filter(|identifier| !supported.contains(identifier))
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            };
            let version = |version: u32| (version != 0).then_some(version);
            Outcome::Refused(Refusal {
                requested_version: version(outcome.requested_version),
                supported_version: version(outcome.supported_version),
                reason,
                missing_features,
            })
        };
        if !matches!(answer, Outcome::Accepted(_)) || table.is_null() {
            return answer;
        }
        // SAFETY: an engine's table starts with its size and version, which
        // every boundary version keeps; nothing past them is read here.
        let (size, version) = unsafe {
            (
                std::ptr::addr_of!((*table).size).read(),
                std::ptr::addr_of!((*table).version).read(),
            )
        };
        if version != crate::BOUNDARY_VERSION || (size as usize) < size_of::<abi::Engine>() {
            return Outcome::Refused(Refusal {
                requested_version: Some(crate::BOUNDARY_VERSION),
                supported_version: (version != 0).then_some(version),
                reason: Reason::VersionMismatch,
                missing_features: Vec::new(),
            });
        }
        let empty = std::ptr::null_mut();
        let table = table.cast_mut();
        let _ = (self.table).compare_exchange(empty, table, Ordering::AcqRel, Ordering::Acquire);
        answer
    }

    /// The engine, once a negotiation was accepted.
    pub fn pass(&self) -> Result<Engine, Closed> {
        let table = self.table.load(Ordering::Acquire);
        // SAFETY: the gate holds only the static table an accepted
        // negotiation of an engine's entry returned, of this client's
        // boundary version and at least its size.
        unsafe { table.as_ref() }.map(Engine).ok_or(Closed)
    }
}

/// A request as it crosses: its version, or 0, and its two lists of texts
/// when it holds exactly those fields.
struct Sent {
    version: u32,
    lists: bool,
    required: Vec<String>,
    optional: Vec<String>,
}

impl Sent {
    fn of(request: &Value) -> Sent {
        let fields = match request {
            Value::Map(fields) => fields.as_slice(),
            _ => &[],
        };
        let field = |name: &str| {
            let mut found = fields.iter().filter(|(key, _)| key == name);
            match (found.next(), found.next()) {
                (Some((_, value)), None) => Some(value),
                _ => None,
            }
        };
        let version = match field("version").map(Value::scalar) {
            Some(Value::Integer(version)) => u32::try_from(*version).unwrap_or(0),
            _ => 0,
        };
        let texts = |value: Option<&Value>| -> Option<Vec<String>> {
            value?
                .items()?
                .iter()
                .map(|item| match item.scalar() {
                    Value::Text(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };
        let lists = (fields.len() == 3 && field("version").is_some())
            .then(|| Some((texts(field("required"))?, texts(field("optional"))?)))
            .flatten();
        let lists_sent = lists.is_some();
        let (required, optional) = lists.unwrap_or_default();
        Sent {
            version,
            lists: lists_sent,
            required,
            optional,
        }
    }
}

fn strs(texts: &[String]) -> Vec<Str> {
    texts.iter().map(|text| Str::of(text)).collect()
}

/// The texts `len` items at `items` borrow, copied; text that is not UTF-8
/// is replaced, and a list no slice can hold is empty.
///
/// # Safety
///
/// A non-null `items` points at `len` texts, each valid for reads.
unsafe fn listed(items: *const Str, len: usize) -> Vec<String> {
    if items.is_null() || len == 0 || len > isize::MAX as usize / size_of::<Str>() {
        return Vec::new();
    }
    // SAFETY: the caller's contract.
    let items = unsafe { std::slice::from_raw_parts(items, len) };
    // SAFETY: as above, for each text.
    items.iter().map(|item| unsafe { owned(*item) }).collect()
}

/// `text`, copied; text that is not UTF-8 is replaced.
///
/// # Safety
///
/// A non-null `text.ptr` points at `text.len` readable bytes.
unsafe fn owned(text: Str) -> String {
    // SAFETY: the caller's contract.
    unsafe { borrowed(text) }.map_or_else(String::new, |bytes| {
        String::from_utf8_lossy(bytes).into_owned()
    })
}

/// The bytes `text` borrows, or `None` for no text or a length no slice can
/// have.
///
/// # Safety
///
/// As [`owned`].
unsafe fn borrowed<'a>(text: Str) -> Option<&'a [u8]> {
    if text.ptr.is_null() || text.len > isize::MAX as usize {
        return None;
    }
    // SAFETY: the caller's contract.
    Some(unsafe { std::slice::from_raw_parts(text.ptr, text.len) })
}

/// The negotiated engine: its table, which lives for the process.
#[derive(Clone, Copy)]
pub struct Engine(&'static abi::Engine);

impl std::fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Engine")
    }
}

/// A handle the engine handed out, given back once when this value drops.
#[derive(Debug)]
pub struct Owned {
    engine: Engine,
    handle: Handle,
}

impl Owned {
    /// The handle, for the engine's entries.
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// The engine that handed the handle out.
    pub fn engine(&self) -> Engine {
        self.engine
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        self.engine.release(self.handle);
    }
}

/// Why a call answered no value.
#[derive(Debug)]
pub enum Refused {
    /// The engine refused the operation: D39's text, the issues that explain
    /// it, a table, and the files a refused save wrote.
    Engine {
        message: String,
        issues: Option<Owned>,
        paths: Vec<String>,
    },
    /// The call was cancelled: the host raises the interrupt it kept.
    Cancelled,
    /// The boundary refused the call, for this reason.
    Boundary(String),
}

/// A nested value `resolve()` returns: numbers stay exact, an integer beyond
/// 64 bits or a float beyond binary64's range as its exact spelling.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Number(String),
    List(Vec<Node>),
    Map(Vec<(String, Node)>),
}

/// The engine information (D41, P-INFO-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub version: String,
    pub language_versions: Vec<String>,
    pub features: Vec<String>,
    pub host: String,
    pub host_version: Option<String>,
    pub platform: String,
}

/// How `read_odm` reads an ODM file.
#[derive(Clone, Debug)]
pub struct OdmOptions<'a> {
    pub archive_member: Option<&'a str>,
    pub max_archive_members: u64,
    pub max_expanded_bytes: u64,
    pub max_xml_depth: u64,
}

/// An exported Arrow C stream, released when this value drops unless taken.
#[derive(Debug)]
pub struct Stream(abi::ArrowArrayStream);

impl Stream {
    /// The C stream, which its new holder releases.
    pub fn into_raw(self) -> abi::ArrowArrayStream {
        let mut this = std::mem::ManuallyDrop::new(self);
        std::mem::replace(&mut this.0, abi::ArrowArrayStream::released())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        if let Some(release) = self.0.release {
            // SAFETY: the engine exported the stream with this release,
            // which its holder calls once.
            unsafe { release(&mut self.0) };
        }
    }
}

/// The host language of a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// No host language: the command line's (D123).
    None,
    Python,
    R,
}

/// The options of one call: the host, its runtime and its callbacks, which
/// the call borrows while it runs.
#[derive(Clone, Copy, Debug)]
pub struct Call<'a> {
    record: abi::Call,
    borrowed: PhantomData<&'a ()>,
}

impl<'a> Call<'a> {
    /// Own callbacks for a streamed run, which can keep them after its initial
    /// call returns. They may be released on any thread. This call is consumed
    /// by `Engine::spec_stream` and cannot be reused.
    pub fn with_owned<H: Callbacks + Send + Sync + 'static>(
        self,
        host: H,
        functions: bool,
    ) -> StreamCall<'a> {
        let mut record = self.with(&host, functions).record;
        record.context = root(host);
        StreamCall {
            record,
            borrowed: PhantomData,
        }
    }

    /// A call from `language`'s host on its runtime, with no callbacks.
    pub fn new(language: Language, runtime_name: &'a str, runtime_version: &'a str) -> Self {
        let host = match language {
            Language::None => abi::HOST_NONE,
            Language::Python => abi::HOST_PYTHON,
            Language::R => abi::HOST_R,
        };
        Call {
            record: abi::Call {
                size: size_of::<abi::Call>() as u32,
                host,
                runtime_name: Str::of(runtime_name),
                runtime_version: Str::of(runtime_version),
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
            },
            borrowed: PhantomData,
        }
    }

    /// The same call with `host`'s probe, and, when `functions`, its
    /// project functions and their lock verification. Every callback is
    /// then `host`'s or none, so none of an earlier host's is left to run
    /// with this host's context.
    pub fn with<H: Callbacks>(mut self, host: &'a H, functions: bool) -> Self {
        let record = &mut self.record;
        // The engine releases the context before the call returns; the host
        // outlives the call, so there is nothing to free.
        record.context = abi::Root {
            pointer: (host as *const H).cast_mut().cast(),
            release: None,
        };
        record.probe = Some(probe::<H>);
        record.bind = functions.then_some(bind::<H> as abi::Bind);
        record.invoke = functions.then_some(invoke::<H> as abi::Invoke);
        record.call_batch = functions.then_some(call_batch::<H> as abi::CallBatch);
        record.is_interrupt = functions.then_some(is_interrupt::<H> as abi::IsInterrupt);
        record.providers = functions.then_some(providers::<H> as abi::Query);
        record.standard_library = functions.then_some(standard_library::<H> as abi::Query);
        record.distribution_version = functions.then_some(distribution_version::<H> as abi::Query);
        record.environment = functions.then_some(environment::<H> as abi::Query);
        record.integer_digits = functions.then_some(integer_digits::<H> as abi::Query);
        record.compare_locked = functions.then_some(compare_locked::<H> as abi::CompareLocked);
        self
    }
}

/// One owned callback context for a streamed call. An unused call releases it.
pub struct StreamCall<'a> {
    record: abi::Call,
    borrowed: PhantomData<&'a ()>,
}

impl Drop for StreamCall<'_> {
    fn drop(&mut self) {
        if let Some(release) = self.record.context.release.take() {
            // SAFETY: this call still owns its context when it was not sent.
            unsafe { release(self.record.context.pointer) };
        }
    }
}

/// Call an entry with fresh destinations and read what it answered.
fn answer<T>(
    engine: Engine,
    entry: impl FnOnce(*mut T, *mut abi::Refusal) -> abi::Status,
) -> Result<T, Refused> {
    let (mut out, mut refusal) = (MaybeUninit::uninit(), MaybeUninit::<abi::Refusal>::uninit());
    let status = entry(out.as_mut_ptr(), refusal.as_mut_ptr());
    let refused = || {
        // SAFETY: an entry that refuses writes its refusal.
        let refusal = unsafe { refusal.assume_init_read() };
        let issues = (refusal.issues != 0).then(|| Owned {
            engine,
            handle: refusal.issues,
        });
        (text(refusal.message), issues, texts(refusal.paths))
    };
    match status {
        // SAFETY: an entry that is done writes its value.
        abi::DONE => Ok(unsafe { out.assume_init() }),
        abi::REFUSED => {
            let (message, issues, paths) = refused();
            Err(Refused::Engine {
                message,
                issues,
                paths,
            })
        }
        abi::BOUNDARY => Err(Refused::Boundary(refused().0)),
        abi::CANCELLED => Err(Refused::Cancelled),
        other => Err(Refused::Boundary(format!(
            "the engine answered status {other}"
        ))),
    }
}

/// The items of a buffer the engine handed out, copied, then the buffer
/// released.
fn copied<T, U>(mut buffer: abi::Buffer<T>, copy: impl Fn(&T) -> U) -> Vec<U> {
    let items = match buffer.len {
        0 => Vec::new(),
        len if buffer.ptr.is_null() || len > isize::MAX as usize / size_of::<T>().max(1) => {
            Vec::new()
        }
        // SAFETY: a buffer's items stay valid until it is released.
        len => unsafe { std::slice::from_raw_parts(buffer.ptr, len) }
            .iter()
            .map(copy)
            .collect(),
    };
    if let Some(release) = buffer.release {
        // SAFETY: the engine handed the buffer out with this release, which
        // its holder calls once.
        unsafe { release(&mut buffer) };
    }
    items
}

fn text(buffer: abi::Text) -> String {
    String::from_utf8_lossy(&copied(buffer, |byte| *byte)).into_owned()
}

fn texts(buffer: abi::Texts) -> Vec<String> {
    // SAFETY: the texts borrow the buffer, valid until it is released.
    copied(buffer, |text| unsafe { owned(*text) })
}

fn information(buffer: abi::Info) -> Option<Info> {
    copied(buffer, |record| {
        // SAFETY: the record borrows the buffer, valid until it is released.
        unsafe {
            Info {
                version: owned(record.version),
                language_versions: listed(record.language_versions, record.language_versions_len),
                features: listed(record.features, record.features_len),
                host: owned(record.host),
                host_version: (!record.host_version.ptr.is_null())
                    .then(|| owned(record.host_version)),
                platform: owned(record.platform),
            }
        }
    })
    .pop()
}

/// The tree a buffer of nodes in pre-order holds.
fn tree(buffer: abi::Nodes) -> Node {
    let nodes = copied(buffer, |node| {
        // SAFETY: the node borrows the buffer, valid until it is released.
        let (key, text) = unsafe { (owned(node.key), owned(node.text)) };
        (*node, key, text)
    });
    fn build(nodes: &mut std::slice::Iter<'_, (abi::Node, String, String)>) -> Node {
        let Some((node, _, text)) = nodes.next() else {
            return Node::Null;
        };
        match node.kind {
            abi::NODE_BOOL => Node::Bool(node.int != 0),
            abi::NODE_INT => Node::Int(node.int),
            abi::NODE_FLOAT => Node::Float(node.float),
            abi::NODE_TEXT => Node::Text(text.clone()),
            abi::NODE_NUMBER => Node::Number(text.clone()),
            abi::NODE_LIST => Node::List((0..node.children).map(|_| build(nodes)).collect()),
            abi::NODE_MAP => Node::Map(
                (0..node.children)
                    .map(|_| {
                        let key = nodes.as_slice().first().map(|(_, key, _)| key.clone());
                        (key.unwrap_or_default(), build(nodes))
                    })
                    .collect(),
            ),
            _ => Node::Null,
        }
    }
    build(&mut nodes.iter())
}

/// The live handles of each kind, and what else the engine holds for its
/// holders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Census {
    pub handles: Vec<(HandleKind, u64)>,
    pub buffers: u64,
    pub streams: u64,
    pub arrays: u64,
}

impl Engine {
    fn owned(self, handle: Handle) -> Owned {
        Owned {
            engine: self,
            handle,
        }
    }

    fn handle(self, handle: Result<Handle, Refused>) -> Result<Owned, Refused> {
        handle.map(|handle| self.owned(handle))
    }

    fn optional(self, handle: Result<Handle, Refused>) -> Result<Option<Owned>, Refused> {
        handle.map(|handle| (handle != 0).then(|| self.owned(handle)))
    }

    pub fn spec_check(
        self,
        call: &Call<'_>,
        spec: &str,
        project: Option<&str>,
    ) -> Result<Owned, Refused> {
        let (spec, project) = (Str::of(spec), Str::optional(project));
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.spec_check)(&call.record, spec, project, out, refusal)
        });
        self.handle(handle)
    }

    /// [`Engine::spec_check`] of a fragment on the parent a caller names
    /// (#3694).
    pub fn spec_check_on(
        self,
        call: &Call<'_>,
        spec: &str,
        project: Option<&str>,
        parent: &str,
    ) -> Result<Owned, Refused> {
        let (spec, project, parent) = (Str::of(spec), Str::optional(project), Str::of(parent));
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.spec_check_on)(&call.record, spec, project, parent, out, refusal)
        });
        self.handle(handle)
    }

    pub fn project_check(self, call: &Call<'_>, project: &str) -> Result<Owned, Refused> {
        let project = Str::of(project);
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.project_check)(&call.record, project, out, refusal)
        });
        self.handle(handle)
    }

    /// Run a specification, reusing the plans holder `plans` keeps, or
    /// compiling afresh when `plans` is 0. A holder is any plans handle
    /// that is live, whoever owns it.
    pub fn spec_run(
        self,
        call: &Call<'_>,
        spec: &str,
        project: Option<&str>,
        plans: Handle,
    ) -> Result<Owned, Refused> {
        let (spec, project) = (Str::of(spec), Str::optional(project));
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.spec_run)(&call.record, spec, project, plans, out, refusal)
        });
        self.handle(handle)
    }

    /// Start a streamed specification run, transferring its owned callbacks.
    pub fn spec_stream(
        self,
        mut call: StreamCall<'_>,
        spec: &str,
        project: Option<&str>,
        plans: Handle,
    ) -> Result<Owned, Refused> {
        let record = call.record;
        call.record.context = abi::Root::NONE;
        // SAFETY: the texts and record are valid for the call; its context is
        // owned and the engine takes it on every outcome, including refusal.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.spec_stream)(
                &record,
                Str::of(spec),
                Str::optional(project),
                plans,
                out,
                refusal,
            )
        });
        self.handle(handle)
    }

    /// Whether a run or table holds a streamed run's output.
    pub fn is_stream(self, handle: Handle) -> Result<bool, Refused> {
        // SAFETY: the destinations are fresh, and the engine validates handles.
        answer(self, |out, refusal| unsafe {
            (self.0.is_stream)(handle, out, refusal)
        })
        .map(|value| value != 0)
    }

    /// Run a project's section, or all of it, with `plans` as
    /// [`Engine::spec_run`] takes it.
    pub fn project_run(
        self,
        call: &Call<'_>,
        project: &str,
        section: Option<&str>,
        plans: Handle,
    ) -> Result<Owned, Refused> {
        let (project, section) = (Str::of(project), Str::optional(section));
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.project_run)(&call.record, project, section, plans, out, refusal)
        });
        self.handle(handle)
    }

    pub fn project_submission(
        self,
        call: &Call<'_>,
        project: &str,
        section: Option<&str>,
        created: Option<&str>,
    ) -> Result<Owned, Refused> {
        let (project, section) = (Str::of(project), Str::optional(section));
        let created = Str::optional(created);
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.project_submission)(&call.record, project, section, created, out, refusal)
        });
        self.handle(handle)
    }

    pub fn read_odm(
        self,
        call: &Call<'_>,
        source: &str,
        options: &OdmOptions<'_>,
    ) -> Result<Owned, Refused> {
        let source = Str::of(source);
        let options = abi::OdmOptions {
            archive_member: Str::optional(options.archive_member),
            max_archive_members: options.max_archive_members,
            max_expanded_bytes: options.max_expanded_bytes,
            max_xml_depth: options.max_xml_depth,
        };
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.read_odm)(&call.record, source, &options, out, refusal)
        });
        self.handle(handle)
    }

    pub fn lint(self, paths: &[String], fix: bool) -> Result<Owned, Refused> {
        let paths = strs(paths);
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.lint)(paths.as_ptr(), paths.len(), u32::from(fix), out, refusal)
        });
        self.handle(handle)
    }

    pub fn selftest(self, call: &Call<'_>) -> Result<Owned, Refused> {
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.selftest)(&call.record, out, refusal)
        });
        self.handle(handle)
    }

    pub fn plans(self) -> Result<Owned, Refused> {
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.plans_new)(out, refusal)
        });
        self.handle(handle)
    }

    pub fn spec_resolve(
        self,
        call: &Call<'_>,
        spec: &str,
        project: Option<&str>,
    ) -> Result<Node, Refused> {
        let (spec, project) = (Str::of(spec), Str::optional(project));
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.spec_resolve)(&call.record, spec, project, out, refusal)
        })
        .map(tree)
    }

    pub fn project_spec(
        self,
        call: &Call<'_>,
        project: &str,
        name: &str,
    ) -> Result<String, Refused> {
        let (project, name) = (Str::of(project), Str::of(name));
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.project_spec)(&call.record, project, name, out, refusal)
        })
        .map(text)
    }

    pub fn project_names(self, call: &Call<'_>, project: &str) -> Result<Vec<String>, Refused> {
        let project = Str::of(project);
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.project_names)(&call.record, project, out, refusal)
        })
        .map(texts)
    }

    pub fn spec_summary(self, spec: &str) -> Result<Vec<String>, Refused> {
        let spec = Str::of(spec);
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.spec_summary)(spec, out, refusal)
        })
        .map(texts)
    }

    pub fn project_summary(self, project: &str) -> Result<Vec<String>, Refused> {
        let project = Str::of(project);
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.project_summary)(project, out, refusal)
        })
        .map(texts)
    }

    pub fn schema(self, file_type: &str) -> Result<String, Refused> {
        let file_type = Str::of(file_type);
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.schema)(file_type, out, refusal)
        })
        .map(text)
    }

    pub fn info(self, call: &Call<'_>) -> Result<Info, Refused> {
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.info)(&call.record, out, refusal)
        })
        .and_then(|buffer| {
            information(buffer).ok_or_else(|| Refused::Boundary("no information".into()))
        })
    }

    fn member<T>(
        self,
        entry: abi::OnMember<T>,
        result: Handle,
        member: Option<&str>,
    ) -> Result<T, Refused> {
        let member = Str::optional(member);
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            entry(result, member, out, refusal)
        })
    }

    fn of<T>(self, entry: abi::OnHandle<T>, handle: Handle) -> Result<T, Refused> {
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe { entry(handle, out, refusal) })
    }

    pub fn status(self, result: Handle, member: Option<&str>) -> Result<String, Refused> {
        self.member(self.0.status, result, member).map(text)
    }

    pub fn issues(self, result: Handle, member: Option<&str>) -> Result<Owned, Refused> {
        let handle = self.member(self.0.issues, result, member);
        self.handle(handle)
    }

    pub fn summary(self, result: Handle, member: Option<&str>) -> Result<Vec<String>, Refused> {
        self.member(self.0.summary, result, member).map(texts)
    }

    pub fn output(self, result: Handle, member: Option<&str>) -> Result<Option<Owned>, Refused> {
        let handle = self.member(self.0.output, result, member);
        self.optional(handle)
    }

    pub fn report(self, result: Handle, member: Option<&str>) -> Result<Option<Owned>, Refused> {
        let handle = self.member(self.0.report, result, member);
        self.optional(handle)
    }

    pub fn save(self, call: &Call<'_>, result: Handle) -> Result<Vec<String>, Refused> {
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.save)(&call.record, result, out, refusal)
        })
        .map(texts)
    }

    pub fn names(self, result: Handle) -> Result<Vec<String>, Refused> {
        self.of(self.0.names, result).map(texts)
    }

    pub fn created(self, result: Handle) -> Result<String, Refused> {
        self.of(self.0.created, result).map(text)
    }

    pub fn observations(
        self,
        result: Handle,
        include_measurements: bool,
    ) -> Result<Option<String>, Refused> {
        let include = u32::from(include_measurements);
        // SAFETY: the arguments are valid for the call.
        answer(self, |out, refusal| unsafe {
            (self.0.observations)(result, include, out, refusal)
        })
        .map(|buffer| buffer.release.is_some().then(|| text(buffer)))
    }

    pub fn cases(self, result: Handle) -> Result<Owned, Refused> {
        let handle = self.of(self.0.cases, result);
        self.handle(handle)
    }

    pub fn selftest_info(self, result: Handle) -> Result<Info, Refused> {
        self.of(self.0.selftest_info, result).and_then(|buffer| {
            information(buffer).ok_or_else(|| Refused::Boundary("no information".into()))
        })
    }

    pub fn rows(self, table: Handle) -> Result<u64, Refused> {
        self.of(self.0.rows, table)
    }

    pub fn columns(self, table: Handle) -> Result<Vec<String>, Refused> {
        self.of(self.0.columns, table).map(texts)
    }

    pub fn preview(self, table: Handle) -> Result<String, Refused> {
        self.of(self.0.preview, table).map(text)
    }

    /// A fresh stream over a table, with its own cursor.
    pub fn export(self, table: Handle) -> Result<Stream, Refused> {
        answer(self, |out: *mut abi::ArrowArrayStream, refusal| {
            // SAFETY: the destination holds a released stream, which the
            // export fills.
            unsafe {
                out.write(abi::ArrowArrayStream::released());
                (self.0.export)(table, out, refusal)
            }
        })
        .map(Stream)
    }

    /// The issues table of no issues, which a host attaches to a refusal of
    /// its own.
    pub fn no_issues(self) -> Result<Owned, Refused> {
        // SAFETY: the arguments are valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.no_issues)(out, refusal)
        });
        self.handle(handle)
    }

    /// The release's complete condition catalogue, including retired entries.
    pub fn conditions(self) -> Result<Owned, Refused> {
        // SAFETY: the destinations are fresh and valid for the call.
        let handle = answer(self, |out, refusal| unsafe {
            (self.0.conditions)(out, refusal)
        });
        self.handle(handle)
    }

    /// Give a handle back; a released handle, or 0, does nothing.
    pub fn release(self, handle: Handle) {
        // SAFETY: any handle may be given back.
        unsafe { (self.0.release)(handle) };
    }

    pub fn census(self) -> Census {
        let mut record = MaybeUninit::<abi::Census>::zeroed();
        // SAFETY: the census writes its record, which is valid all zeros.
        let record = unsafe {
            (self.0.census)(record.as_mut_ptr());
            record.assume_init()
        };
        Census {
            handles: HandleKind::ALL.into_iter().zip(record.handles).collect(),
            buffers: record.buffers,
            streams: record.streams,
            arrays: record.arrays,
        }
    }
}

// -- a host's callbacks ---------------------------------------------------------

/// A scalar the engine passes a function, borrowing its text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalarRef<'a> {
    Missing,
    Text(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Whole days since 1970-01-01.
    Date(i64),
    /// Whole seconds since 1970-01-01T00:00:00.
    DateTime(i64),
}

/// A scalar a function returns.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Missing,
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Whole days since 1970-01-01.
    Date(i64),
    /// Whole seconds since 1970-01-01T00:00:00.
    DateTime(i64),
}

/// A function's declared result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnType {
    Str,
    Int,
    Float,
    Date,
    DateTime,
}

/// A project function's identity: its name and the logical call a host
/// resolves.
#[derive(Clone, Copy, Debug)]
pub struct Identity<'a> {
    pub name: &'a str,
    pub call: &'a str,
}

/// A function's admitted signature.
#[derive(Clone, Debug)]
pub struct Signature<'a> {
    pub parameters: Vec<&'a str>,
    pub returns: ColumnType,
    pub may_return_missing: bool,
}

/// One named argument.
#[derive(Clone, Copy, Debug)]
pub struct Argument<'a> {
    pub name: &'a str,
    pub value: ScalarRef<'a>,
}

/// One named batch argument: one value per row.
#[derive(Clone, Debug)]
pub struct BatchArgument<'a> {
    pub name: &'a str,
    pub values: Vec<ScalarRef<'a>>,
}

/// The kind of a function failure, as [`crate::abi`]'s `ERROR_*` codes name
/// it; `Engine` is a failure of the engine's own, such as a lock finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Engine,
    Exception,
    Interrupted,
    Rejected,
    Representation,
    SdkBoundary,
    Internal,
    Protocol,
    MissingBinding,
}

impl FailureKind {
    fn code(self) -> u32 {
        match self {
            FailureKind::Engine => 0,
            FailureKind::Exception => abi::ERROR_EXCEPTION,
            FailureKind::Interrupted => abi::ERROR_INTERRUPTED,
            FailureKind::Rejected => abi::ERROR_REJECTED,
            FailureKind::Representation => abi::ERROR_REPRESENTATION,
            FailureKind::SdkBoundary => abi::ERROR_SDK_BOUNDARY,
            FailureKind::Internal => abi::ERROR_INTERNAL,
            FailureKind::Protocol => abi::ERROR_PROTOCOL,
            FailureKind::MissingBinding => abi::ERROR_MISSING_BINDING,
        }
    }

    fn of(code: u32) -> FailureKind {
        [
            FailureKind::Exception,
            FailureKind::Interrupted,
            FailureKind::Rejected,
            FailureKind::Representation,
            FailureKind::SdkBoundary,
            FailureKind::Internal,
            FailureKind::Protocol,
            FailureKind::MissingBinding,
        ]
        .into_iter()
        .find(|kind| kind.code() == code)
        .unwrap_or(FailureKind::Engine)
    }
}

/// A host failure: its kind, the exception's class and its message (or the
/// reason, the logical call or the internal code its kind gives), a refused
/// value's type, and the original host object, which the engine keeps.
#[derive(Debug)]
pub struct Failure<O> {
    pub kind: FailureKind,
    pub class: String,
    pub message: String,
    pub returned: Option<String>,
    pub payload: Option<O>,
}

impl<O> Failure<O> {
    /// A failure of `kind` with `message` and nothing else.
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Failure {
            kind,
            class: String::new(),
            message: message.into(),
            returned: None,
            payload: None,
        }
    }

    /// A reply outside the protocol.
    pub fn protocol() -> Self {
        Failure::new(FailureKind::Protocol, "")
    }
}

/// How a call of a project function failed: the function raised, or the
/// host could not represent what it returned.
#[derive(Debug)]
pub enum CallFailure<O> {
    Raised(Failure<O>),
    Invalid(Failure<O>),
}

/// An installed distribution's version, as its metadata records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    Missing,
    Version(String),
    Unreadable,
}

/// One lock finding a host compares: the package, the reason, the expected
/// version and the actual one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockFinding {
    pub package: String,
    pub reason: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// A host's callbacks: the probe, and its project functions with their lock
/// verification. Ordinary callbacks run on the calling thread; streamed
/// callbacks follow the retained context's thread contract. A panic in one
/// is contained and answered as a failure of the host's own.
/// The provided methods are the engine's own defaults.
pub trait Callbacks {
    /// A host object the engine keeps: a bound function, or a failure's
    /// original object. The engine drops it on the thread that releases
    /// what keeps it. Interpreter objects therefore need a thread-safe owner
    /// (the R binding uses its checked `ThreadBound` wrapper).
    type Object: Send + Sync + 'static;

    /// Whether the call is cancelled; a host that answers true keeps the
    /// interrupt it found and raises it once the call returns.
    fn cancelled(&self) -> bool {
        false
    }

    fn bind(
        &self,
        _identity: &Identity<'_>,
        _signature: &Signature<'_>,
    ) -> Result<Self::Object, Failure<Self::Object>> {
        Err(Failure::protocol())
    }

    fn invoke(
        &self,
        _function: &Self::Object,
        _arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallFailure<Self::Object>> {
        Err(CallFailure::Raised(Failure::protocol()))
    }

    /// Call `function` once per row, stopping at the first failure.
    fn call_batch(
        &self,
        function: &Self::Object,
        rows: usize,
        arguments: &[BatchArgument<'_>],
    ) -> Result<Vec<Scalar>, CallFailure<Self::Object>> {
        (0..rows)
            .map(|row| {
                let arguments: Vec<Argument<'_>> = arguments
                    .iter()
                    .filter_map(|argument| {
                        let value = *argument.values.get(row)?;
                        Some(Argument {
                            name: argument.name,
                            value,
                        })
                    })
                    .collect();
                self.invoke(function, &arguments)
            })
            .collect()
    }

    /// Whether a failure is the host's interrupt.
    fn is_interrupt(&self, kind: FailureKind, _payload: Option<&Self::Object>) -> bool {
        kind == FailureKind::Interrupted
    }

    fn providers(&self, _modules: &[&str]) -> Result<Vec<(String, String)>, Failure<Self::Object>> {
        Err(Failure::protocol())
    }

    fn standard_library(&self, _module: &str) -> Result<bool, Failure<Self::Object>> {
        Err(Failure::protocol())
    }

    fn distribution_version(
        &self,
        _distribution: &str,
    ) -> Result<Installed, Failure<Self::Object>> {
        Err(Failure::protocol())
    }

    fn environment(&self) -> Result<Vec<(String, String)>, Failure<Self::Object>> {
        Err(Failure::protocol())
    }

    /// The limit on the digits of an integer read from text; 0 turns it off.
    fn integer_digits(&self) -> Result<usize, Failure<Self::Object>> {
        Err(Failure::protocol())
    }

    fn compare_locked(
        &self,
        _packages: &[(&str, &str)],
        _calls: &[&str],
    ) -> Result<Vec<LockFinding>, Failure<Self::Object>> {
        Err(Failure::protocol())
    }
}

/// What a reply points into, which its release frees.
#[derive(Default)]
struct Held {
    texts: Vec<String>,
    strs: Vec<Str>,
    scalars: Vec<abi::Scalar>,
}

impl Held {
    /// Keep `text` and borrow it.
    fn text(&mut self, text: String) -> Str {
        self.texts.push(text);
        Str::of(&self.texts[self.texts.len() - 1])
    }

    fn optional(&mut self, text: Option<String>) -> Str {
        text.map_or(Str::NONE, |text| self.text(text))
    }

    fn scalar(&mut self, scalar: Scalar) -> abi::Scalar {
        let (kind, int, float, text) = match scalar {
            Scalar::Missing => (abi::SCALAR_MISSING, 0, 0.0, Str::NONE),
            Scalar::Text(text) => (abi::SCALAR_TEXT, 0, 0.0, self.text(text)),
            Scalar::Int(value) => (abi::SCALAR_INT, value, 0.0, Str::NONE),
            Scalar::Float(value) => (abi::SCALAR_FLOAT, 0, value, Str::NONE),
            Scalar::Bool(value) => (abi::SCALAR_BOOL, i64::from(value), 0.0, Str::NONE),
            Scalar::Date(days) => (abi::SCALAR_DATE, days, 0.0, Str::NONE),
            Scalar::DateTime(seconds) => (abi::SCALAR_DATETIME, seconds, 0.0, Str::NONE),
        };
        abi::Scalar {
            kind,
            int,
            float,
            text,
        }
    }
}

unsafe extern "C" fn release_reply(reply: *mut abi::Reply) {
    // SAFETY: the engine passes the reply the client filled, whose private
    // data is the box it leaked, released once.
    unsafe {
        let reply = &mut *reply;
        if !reply.private_data.is_null() {
            drop(Box::from_raw(reply.private_data.cast::<Held>()));
            reply.private_data = std::ptr::null_mut();
        }
    }
}

unsafe extern "C" fn release_object<O: 'static>(pointer: *mut c_void) {
    // SAFETY: the pointer is the box the client leaked for the object,
    // released once.
    drop(unsafe { Box::from_raw(pointer.cast::<O>()) });
}

/// The host's object behind a root the client made.
///
/// # Safety
///
/// `pointer` is null or the box [`root`] made for an `O`.
unsafe fn object<'a, O>(pointer: *mut c_void) -> Option<&'a O> {
    // SAFETY: the caller's contract.
    unsafe { pointer.cast::<O>().as_ref() }
}

fn root<O: 'static>(object: O) -> abi::Root {
    abi::Root {
        pointer: Box::into_raw(Box::new(object)).cast(),
        release: Some(release_object::<O>),
    }
}

/// The host a callback's context is.
///
/// # Safety
///
/// `context` is the pointer to the host a [`Call::with`] borrowed for the
/// call, or a [`Call::with_owned`] transferred for the streamed run.
unsafe fn host<'a, H>(context: *mut c_void) -> &'a H {
    // SAFETY: the caller's contract.
    unsafe { &*context.cast::<H>() }
}

/// Fill `reply` with what a callback answered, keeping its memory until the
/// engine releases the reply, and say how it answered.
fn reply<O: 'static>(
    reply: &mut abi::Reply,
    held: Held,
    answered: Result<(), CallFailure<O>>,
) -> c_int {
    let mut held = Box::new(held);
    let status = match answered {
        Ok(()) => abi::ANSWERED,
        Err(failure) => {
            let (status, failure) = match failure {
                CallFailure::Raised(failure) => (abi::RAISED, failure),
                CallFailure::Invalid(failure) => (abi::INVALID, failure),
            };
            reply.error = abi::HostError {
                kind: failure.kind.code(),
                class: held.text(failure.class),
                message: held.text(failure.message),
                returned: held.optional(failure.returned),
                payload: failure.payload.map_or(abi::Root::NONE, root),
            };
            status
        }
    };
    reply.private_data = Box::into_raw(held).cast();
    reply.release = Some(release_reply);
    status
}

/// A panic of the host's own code, answered as a failure of its own.
fn contained<O>() -> CallFailure<O> {
    CallFailure::Raised(Failure::new(FailureKind::Internal, "function_boundary"))
}

/// UTF-8 text the bytes hold, or `None`.
fn utf8_bytes(bytes: Option<&[u8]>) -> Option<&str> {
    std::str::from_utf8(bytes?).ok()
}

/// The texts `len` items at `items` borrow, or `None` when one is not UTF-8.
///
/// # Safety
///
/// A non-null `items` points at `len` texts, each valid for reads for the
/// callback.
unsafe fn borrowed_texts<'a>(items: *const Str, len: usize) -> Option<Vec<&'a str>> {
    if items.is_null() || len == 0 {
        return Some(Vec::new());
    }
    if len > isize::MAX as usize / size_of::<Str>() {
        return None;
    }
    // SAFETY: the caller's contract.
    let items = unsafe { std::slice::from_raw_parts(items, len) };
    items
        .iter()
        // SAFETY: as above, for each text.
        .map(|item| unsafe { borrowed(*item) }.and_then(|bytes| std::str::from_utf8(bytes).ok()))
        .collect()
}

/// A scalar the engine passes, borrowing its text.
///
/// # Safety
///
/// The scalar's text is valid for reads for the callback.
unsafe fn scalar_ref<'a>(scalar: &abi::Scalar) -> Option<ScalarRef<'a>> {
    Some(match scalar.kind {
        abi::SCALAR_MISSING => ScalarRef::Missing,
        // SAFETY: the caller's contract.
        abi::SCALAR_TEXT => {
            ScalarRef::Text(std::str::from_utf8(unsafe { borrowed(scalar.text) }?).ok()?)
        }
        abi::SCALAR_INT => ScalarRef::Int(scalar.int),
        abi::SCALAR_FLOAT => ScalarRef::Float(scalar.float),
        abi::SCALAR_BOOL => ScalarRef::Bool(scalar.int != 0),
        abi::SCALAR_DATE => ScalarRef::Date(scalar.int),
        abi::SCALAR_DATETIME => ScalarRef::DateTime(scalar.int),
        _ => return None,
    })
}

/// Run a callback's host code, containing a panic.
fn guarded<T, O>(run: impl FnOnce() -> Result<T, CallFailure<O>>) -> Result<T, CallFailure<O>> {
    catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|_: Box<dyn Any + Send>| Err(contained()))
}

/// Run a query's host code, containing a panic.
fn queried<T, O>(run: impl FnOnce() -> Result<T, Failure<O>>) -> Result<T, CallFailure<O>> {
    guarded(|| run().map_err(CallFailure::Raised))
}

unsafe extern "C" fn probe<H: Callbacks>(context: *mut c_void) -> c_int {
    // SAFETY: see the module documentation.
    let host = unsafe { host::<H>(context) };
    match catch_unwind(AssertUnwindSafe(|| host.cancelled())) {
        Ok(false) => abi::GO_ON,
        Ok(true) => abi::CANCEL,
        // A panic is outside the protocol, which the call refuses.
        Err(_) => c_int::MAX,
    }
}

unsafe extern "C" fn bind<H: Callbacks>(
    context: *mut c_void,
    identity: *const abi::Identity,
    signature: *const abi::Signature,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation; the engine passes its records
    // for the callback and a fresh reply.
    let (host, identity, signature, out) =
        unsafe { (host::<H>(context), &*identity, &*signature, &mut *out) };
    let answered = guarded(|| {
        // SAFETY: as above.
        let (name, call) = unsafe { (borrowed(identity.name), borrowed(identity.call)) };
        // SAFETY: as above.
        let parameters = unsafe { borrowed_texts(signature.parameters, signature.parameters_len) };
        let returns = match signature.returns {
            abi::COLUMN_STR => ColumnType::Str,
            abi::COLUMN_INT => ColumnType::Int,
            abi::COLUMN_FLOAT => ColumnType::Float,
            abi::COLUMN_DATE => ColumnType::Date,
            abi::COLUMN_DATETIME => ColumnType::DateTime,
            _ => return Err(CallFailure::Raised(Failure::protocol())),
        };
        let utf8 = utf8_bytes;
        let (Some(name), Some(call), Some(parameters)) = (utf8(name), utf8(call), parameters)
        else {
            return Err(CallFailure::Raised(Failure::protocol()));
        };
        let identity = Identity { name, call };
        let signature = Signature {
            parameters,
            returns,
            may_return_missing: signature.may_return_missing != 0,
        };
        host.bind(&identity, &signature)
            .map_err(CallFailure::Raised)
    });
    match answered {
        Ok(function) => {
            out.function = root(function);
            reply::<H::Object>(out, Held::default(), Ok(()))
        }
        Err(failure) => reply(out, Held::default(), Err(failure)),
    }
}

unsafe extern "C" fn invoke<H: Callbacks>(
    context: *mut c_void,
    function: *mut c_void,
    arguments: *const abi::Argument,
    arguments_len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, out) = unsafe { (host::<H>(context), &mut *out) };
    let mut held = Held::default();
    let answered = guarded(|| {
        // SAFETY: the function is the root the client made at bind.
        let function = unsafe { object::<H::Object>(function) };
        // SAFETY: the engine passes its arguments for the callback.
        let arguments = unsafe { arguments_of(arguments, arguments_len) };
        let (Some(function), Some(arguments)) = (function, arguments) else {
            return Err(CallFailure::Raised(Failure::protocol()));
        };
        host.invoke(function, &arguments)
    });
    let answered = answered.map(|result| out.scalar = held.scalar(result));
    reply(out, held, answered)
}

/// The arguments the engine passes, borrowing their texts.
///
/// # Safety
///
/// A non-null `arguments` points at `len` arguments, valid for reads for the
/// callback.
unsafe fn arguments_of<'a>(
    arguments: *const abi::Argument,
    len: usize,
) -> Option<Vec<Argument<'a>>> {
    if arguments.is_null() || len == 0 {
        return Some(Vec::new());
    }
    if len > isize::MAX as usize / size_of::<abi::Argument>() {
        return None;
    }
    // SAFETY: the caller's contract.
    let arguments = unsafe { std::slice::from_raw_parts(arguments, len) };
    arguments
        .iter()
        .map(|argument| {
            // SAFETY: as above.
            let name = std::str::from_utf8(unsafe { borrowed(argument.name) }?).ok()?;
            // SAFETY: as above.
            let value = unsafe { scalar_ref(&argument.value) }?;
            Some(Argument { name, value })
        })
        .collect()
}

unsafe extern "C" fn call_batch<H: Callbacks>(
    context: *mut c_void,
    function: *mut c_void,
    rows: usize,
    arguments: *const abi::BatchArgument,
    arguments_len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, out) = unsafe { (host::<H>(context), &mut *out) };
    let mut held = Held::default();
    let answered = guarded(|| {
        // SAFETY: the function is the root the client made at bind.
        let function = unsafe { object::<H::Object>(function) };
        // SAFETY: the engine passes its arguments for the callback.
        let arguments = unsafe { batch_of(arguments, arguments_len) };
        let (Some(function), Some(arguments)) = (function, arguments) else {
            return Err(CallFailure::Raised(Failure::protocol()));
        };
        host.call_batch(function, rows, &arguments)
    });
    let answered = answered.map(|results| {
        held.scalars = results
            .into_iter()
            .map(|result| held.scalar(result))
            .collect();
        (out.scalars, out.scalars_len) = (held.scalars.as_ptr(), held.scalars.len());
    });
    reply(out, held, answered)
}

/// The batch arguments the engine passes, borrowing their texts.
///
/// # Safety
///
/// As [`arguments_of`], for batch arguments and each one's values.
unsafe fn batch_of<'a>(
    arguments: *const abi::BatchArgument,
    len: usize,
) -> Option<Vec<BatchArgument<'a>>> {
    if arguments.is_null() || len == 0 {
        return Some(Vec::new());
    }
    if len > isize::MAX as usize / size_of::<abi::BatchArgument>() {
        return None;
    }
    // SAFETY: the caller's contract.
    let arguments = unsafe { std::slice::from_raw_parts(arguments, len) };
    arguments
        .iter()
        .map(|argument| {
            // SAFETY: as above.
            let name = std::str::from_utf8(unsafe { borrowed(argument.name) }?).ok()?;
            let values = if argument.values.is_null() || argument.values_len == 0 {
                &[][..]
            } else if argument.values_len > isize::MAX as usize / size_of::<abi::Scalar>() {
                return None;
            } else {
                // SAFETY: as above.
                unsafe { std::slice::from_raw_parts(argument.values, argument.values_len) }
            };
            // SAFETY: as above, for each value.
            let values = values
                .iter()
                .map(|value| unsafe { scalar_ref(value) })
                .collect::<Option<_>>()?;
            Some(BatchArgument { name, values })
        })
        .collect()
}

unsafe extern "C" fn is_interrupt<H: Callbacks>(
    context: *mut c_void,
    kind: u32,
    payload: *mut c_void,
) -> c_int {
    // SAFETY: see the module documentation; a payload is a root the client
    // made.
    let (host, payload) = unsafe { (host::<H>(context), object::<H::Object>(payload)) };
    let classified = catch_unwind(AssertUnwindSafe(|| {
        host.is_interrupt(FailureKind::of(kind), payload)
    }));
    // A panic leaves the engine's default.
    c_int::from(classified.unwrap_or(kind == abi::ERROR_INTERRUPTED))
}

/// Answer a query with `texts` rows into `out`.
fn texts_reply<O: 'static>(
    out: &mut abi::Reply,
    answered: Result<Vec<String>, CallFailure<O>>,
) -> c_int {
    let mut held = Held::default();
    let answered = answered.map(|texts| {
        held.strs = texts.into_iter().map(|text| held.text(text)).collect();
        (out.texts, out.texts_len) = (held.strs.as_ptr(), held.strs.len());
    });
    reply(out, held, answered)
}

/// Answer a query with a scalar into `out`.
fn scalar_reply<O: 'static>(
    out: &mut abi::Reply,
    answered: Result<Scalar, CallFailure<O>>,
) -> c_int {
    let mut held = Held::default();
    let answered = answered.map(|scalar| out.scalar = held.scalar(scalar));
    reply(out, held, answered)
}

/// The texts a query is asked, and its host and reply.
///
/// # Safety
///
/// As the queries' callbacks receive them.
unsafe fn asked<'a, H>(
    context: *mut c_void,
    texts: *const Str,
    len: usize,
    out: *mut abi::Reply,
) -> (&'a H, Option<Vec<&'a str>>, &'a mut abi::Reply) {
    // SAFETY: the caller's contract.
    unsafe { (host::<H>(context), borrowed_texts(texts, len), &mut *out) }
}

fn pairs(pairs: Vec<(String, String)>) -> Vec<String> {
    pairs
        .into_iter()
        .flat_map(|(key, value)| [key, value])
        .collect()
}

unsafe extern "C" fn providers<H: Callbacks>(
    context: *mut c_void,
    texts: *const Str,
    len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, modules, out) = unsafe { asked::<H>(context, texts, len, out) };
    let answered = queried(|| {
        host.providers(&modules.ok_or_else(Failure::protocol)?)
            .map(pairs)
    });
    texts_reply(out, answered)
}

unsafe extern "C" fn standard_library<H: Callbacks>(
    context: *mut c_void,
    texts: *const Str,
    len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, module, out) = unsafe { asked::<H>(context, texts, len, out) };
    let answered = queried(|| match module.as_deref() {
        Some([module]) => host.standard_library(module).map(Scalar::Bool),
        _ => Err(Failure::protocol()),
    });
    scalar_reply(out, answered)
}

unsafe extern "C" fn distribution_version<H: Callbacks>(
    context: *mut c_void,
    texts: *const Str,
    len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, distribution, out) = unsafe { asked::<H>(context, texts, len, out) };
    let answered = queried(|| match distribution.as_deref() {
        Some([distribution]) => Ok(match host.distribution_version(distribution)? {
            Installed::Missing => Scalar::Missing,
            Installed::Version(version) => Scalar::Text(version),
            Installed::Unreadable => Scalar::Bool(false),
        }),
        _ => Err(Failure::protocol()),
    });
    scalar_reply(out, answered)
}

unsafe extern "C" fn environment<H: Callbacks>(
    context: *mut c_void,
    texts: *const Str,
    len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, _, out) = unsafe { asked::<H>(context, texts, len, out) };
    texts_reply(out, queried(|| host.environment().map(pairs)))
}

unsafe extern "C" fn integer_digits<H: Callbacks>(
    context: *mut c_void,
    texts: *const Str,
    len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, _, out) = unsafe { asked::<H>(context, texts, len, out) };
    let answered = queried(|| {
        let digits = host.integer_digits()?;
        i64::try_from(digits)
            .map(Scalar::Int)
            .map_err(|_| Failure::protocol())
    });
    scalar_reply(out, answered)
}

unsafe extern "C" fn compare_locked<H: Callbacks>(
    context: *mut c_void,
    packages: *const Str,
    packages_len: usize,
    calls: *const Str,
    calls_len: usize,
    out: *mut abi::Reply,
) -> c_int {
    // SAFETY: see the module documentation.
    let (host, out) = unsafe { (host::<H>(context), &mut *out) };
    // SAFETY: the engine passes its texts for the callback.
    let (packages, calls) = unsafe {
        (
            borrowed_texts(packages, packages_len),
            borrowed_texts(calls, calls_len),
        )
    };
    let mut held = Held::default();
    let answered = queried(|| {
        let (Some(packages), Some(calls)) = (packages, calls) else {
            return Err(Failure::protocol());
        };
        let packages: Vec<(&str, &str)> = packages
            .chunks(2)
            .filter_map(|pair| match pair {
                [name, version] => Some((*name, *version)),
                _ => None,
            })
            .collect();
        host.compare_locked(&packages, &calls)
    });
    let answered = answered.map(|findings| {
        held.strs = findings
            .into_iter()
            .flat_map(|finding| {
                [
                    Some(finding.package),
                    Some(finding.reason),
                    finding.expected,
                    finding.actual,
                ]
            })
            .map(|text| held.optional(text))
            .collect();
        (out.texts, out.texts_len) = (held.strs.as_ptr(), held.strs.len());
    });
    reply(out, held, answered)
}

// -- civil dates ----------------------------------------------------------------

/// The civil date `days` whole days after 1970-01-01, within the years 1 to
/// 9999.
pub fn civil_date(days: i64) -> Option<(i32, u8, u8)> {
    // Howard Hinnant's days-to-civil, in eras of 400 years.
    let days = days.checked_add(719_468)?;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    if !(1..=9999).contains(&year) {
        return None;
    }
    Some((year as i32, month as u8, day as u8))
}

/// The whole days from 1970-01-01 to a civil date.
pub fn epoch_days(year: i32, month: u8, day: u8) -> i64 {
    // Howard Hinnant's civil-to-days.
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date and time `seconds` whole seconds after
/// 1970-01-01T00:00:00, within the years 1 to 9999.
pub fn civil_datetime(seconds: i64) -> Option<(i32, u8, u8, u8, u8, u8)> {
    let (days, second_of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_date(days)?;
    let (hour, minute, second) = (
        second_of_day / 3600,
        second_of_day % 3600 / 60,
        second_of_day % 60,
    );
    Some((year, month, day, hour as u8, minute as u8, second as u8))
}

/// The client's own side of the ABI, driven as the engine drives it: the
/// batch trampoline, which no engine path calls yet (only core's registry
/// activation batches, which no facade entry reaches), the callbacks a call
/// offers, and the gate's check of the table an engine returns.
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// One call's arguments: each name with its integer, or `None` for a
    /// missing value.
    type Arguments = Vec<(String, Option<i64>)>;

    /// A host whose bound function adds its offset to the sum of its
    /// arguments, or answers missing when one is missing, keeping the
    /// arguments of each call; its probe answers `cancelled`.
    #[derive(Default)]
    struct Adder {
        calls: RefCell<Vec<Arguments>>,
        cancelled: bool,
    }

    impl Callbacks for Adder {
        type Object = i64;

        fn cancelled(&self) -> bool {
            self.cancelled
        }

        fn invoke(
            &self,
            offset: &i64,
            arguments: &[Argument<'_>],
        ) -> Result<Scalar, CallFailure<i64>> {
            let values: Arguments = arguments
                .iter()
                .map(|argument| match argument.value {
                    ScalarRef::Int(value) => (argument.name.to_owned(), Some(value)),
                    _ => (argument.name.to_owned(), None),
                })
                .collect();
            let sum: Option<i64> = values.iter().map(|(_, value)| *value).sum();
            self.calls.borrow_mut().push(values);
            Ok(sum.map_or(Scalar::Missing, |sum| Scalar::Int(offset + sum)))
        }
    }

    fn int(value: i64) -> abi::Scalar {
        abi::Scalar {
            kind: abi::SCALAR_INT,
            int: value,
            ..abi::Scalar::MISSING
        }
    }

    #[test]
    fn a_batch_call_invokes_the_bound_function_once_per_row_in_row_order() {
        let host = Adder::default();
        let call = Call::new(Language::R, "r", "4.6.1").with(&host, true);
        let (x, y) = (
            [int(1), int(2), int(3)],
            [int(10), int(20), abi::Scalar::MISSING],
        );
        let arguments = [("x", &x), ("y", &y)].map(|(name, values)| abi::BatchArgument {
            name: Str::of(name),
            values: values.as_ptr(),
            values_len: values.len(),
        });
        // The bound function, rooted as the bind trampoline roots it.
        let function = root(100_i64);
        let mut reply = abi::Reply::empty();
        let batch = call
            .record
            .call_batch
            .expect("a call with functions batches");
        // SAFETY: the context is `host`, which outlives the call; the
        // function is the root the client made for its object; the arguments
        // and the reply outlive the call.
        let status = unsafe {
            let context = call.record.context.pointer;
            let (rows, count) = (x.len(), arguments.len());
            batch(
                context,
                function.pointer,
                rows,
                arguments.as_ptr(),
                count,
                &mut reply,
            )
        };
        assert_eq!(status, abi::ANSWERED, "the batch call answers");
        // SAFETY: an answered batch holds one result per row in `scalars`.
        let results = unsafe { std::slice::from_raw_parts(reply.scalars, reply.scalars_len) };
        let results: Vec<(u32, i64)> = results.iter().map(|r| (r.kind, r.int)).collect();
        // SAFETY: the reply's release and the function's release each run
        // once, after the reply was read, as the engine runs them.
        unsafe {
            (reply
                .release
                .take()
                .expect("a filled reply has its release"))(&mut reply);
            (function.release.expect("a root has its release"))(function.pointer);
        }
        let row = |x: i64, y: Option<i64>| vec![("x".to_owned(), Some(x)), ("y".to_owned(), y)];
        assert_eq!(
            (results, host.calls.take()),
            (
                vec![
                    (abi::SCALAR_INT, 111),
                    (abi::SCALAR_INT, 122),
                    (abi::SCALAR_MISSING, 0),
                ],
                vec![row(1, Some(10)), row(2, Some(20)), row(3, None)],
            ),
            "a batch of three rows whose last y is missing, through the default batch call"
        );
    }

    #[test]
    fn a_later_host_replaces_every_callback_of_an_earlier_one() {
        let (first, second) = (
            Adder::default(),
            Adder {
                cancelled: true,
                ..Adder::default()
            },
        );
        let record = Call::new(Language::Python, "python", "3.12.4")
            .with(&first, true)
            .with(&second, false)
            .record;
        let offered = [
            ("bind", record.bind.is_some()),
            ("invoke", record.invoke.is_some()),
            ("call_batch", record.call_batch.is_some()),
            ("is_interrupt", record.is_interrupt.is_some()),
            ("providers", record.providers.is_some()),
            ("standard_library", record.standard_library.is_some()),
            (
                "distribution_version",
                record.distribution_version.is_some(),
            ),
            ("environment", record.environment.is_some()),
            ("integer_digits", record.integer_digits.is_some()),
            ("compare_locked", record.compare_locked.is_some()),
        ];
        let offered: Vec<&str> = offered
            .into_iter()
            .filter_map(|(name, offered)| offered.then_some(name))
            .collect();
        let probe = record.probe.expect("a call with a host has its probe");
        // SAFETY: the context is `second`, whose probe this is, and which
        // outlives the call.
        let probed = unsafe { probe(record.context.pointer) };
        assert_eq!(
            (offered, probed),
            (Vec::<&str>::new(), abi::CANCEL),
            "a host that offers no functions after one that did: no function \
             callback is left, and the probe asks the later host"
        );
    }

    /// A table header as an engine of another build might return it: only
    /// the size and version every boundary version keeps.
    #[repr(C)]
    struct Header {
        size: u32,
        version: u32,
    }

    static TABLES: [Header; 2] = [
        Header {
            size: size_of::<abi::Engine>() as u32,
            version: crate::BOUNDARY_VERSION + 1,
        },
        Header {
            size: size_of::<Header>() as u32,
            version: crate::BOUNDARY_VERSION,
        },
    ];

    /// A negotiation entry that accepts any request and returns table
    /// `INDEX` of [`TABLES`].
    unsafe extern "C" fn accepting<const INDEX: usize>(
        _request: *const abi::Request,
        outcome: *mut abi::Outcome,
    ) -> *const abi::Engine {
        // SAFETY: the client passes its outcome for the call.
        let outcome = unsafe { &mut *outcome };
        (outcome.accepted, outcome.requested_version) = (1, crate::BOUNDARY_VERSION);
        outcome.supported_version = crate::BOUNDARY_VERSION;
        std::ptr::addr_of!(TABLES[INDEX]).cast()
    }

    #[test]
    fn a_table_this_client_cannot_read_keeps_the_gate_closed() {
        let request = Value::Map(Vec::new());
        let cases: [(&str, Negotiate, Option<u32>); 2] = [
            (
                "a table of the next boundary version",
                accepting::<0>,
                Some(crate::BOUNDARY_VERSION + 1),
            ),
            (
                "a table shorter than this client's",
                accepting::<1>,
                Some(crate::BOUNDARY_VERSION),
            ),
        ];
        for (label, entry, supported_version) in cases {
            let gate = Gate::new();
            // SAFETY: the entry answers as a negotiation entry does.
            let outcome = unsafe { gate.negotiate(&request, entry) };
            assert_eq!(
                (outcome, gate.pass().map(|_| ())),
                (
                    Outcome::Refused(Refusal {
                        requested_version: Some(crate::BOUNDARY_VERSION),
                        supported_version,
                        reason: Reason::VersionMismatch,
                        missing_features: Vec::new(),
                    }),
                    Err(Closed),
                ),
                "{label}"
            );
        }
    }
}
