//! Unused records, callbacks and function table of the retired engine C ABI,
//! kept as `repr(C)` types until the final removal step of #3877 (the design
//! on #2870, sections 3.4 to 3.6).
//!
//! The former engine crate has been removed (D281). No workspace consumer
//! implements or calls this table or its [`client`](crate::client).
//! Both host bindings call the `yamaa` facade directly; their active
//! negotiation and native-call gate use [`crate::Gate`]. The sections below
//! document the legacy contract while these declarations remain.
//!
//! The ABI is internal (#3030 D225): any release may change it with a new
//! [`BOUNDARY_VERSION`](crate::BOUNDARY_VERSION). Within one boundary version
//! the function table and each record with a `size` field only grow at their
//! end, and a reader treats a field past the writer's `size` as null. Kept in
//! every boundary version, so that any two yamaa builds refuse each other
//! cleanly, are the negotiation entry's signature, the [`Request`] header
//! (`size`, `version`), the fixed part of [`Outcome`] (`size` to `reason`),
//! the [`Engine`] table's header (`size`, `version`) and the check order of
//! the crate documentation.
//!
//! # Calls
//!
//! An entry borrows its arguments for the call: each [`Str`] and list stays
//! valid until the entry returns, and the engine copies what it keeps. It
//! returns a [`Status`]: on [`DONE`] it has written its value to `out`; on
//! [`REFUSED`] and [`BOUNDARY`] it has written `refusal`; on [`CANCELLED`] it
//! has written neither, and the host raises the interrupt it kept (D46,
//! D186). An entry writes `out` and `refusal` without reading them, except
//! [`Engine::export`], whose `out` holds a released stream. An entry given a
//! null `out` answers [`BOUNDARY`] and writes why into `refusal`; one given a
//! null `refusal` answers [`BOUNDARY`] and writes nothing; neither does
//! anything else, and an entry that takes a call takes its context either
//! way. Every entry contains a panic, which it answers as [`BOUNDARY`];
//! nothing unwinds across the ABI.
//!
//! # Memory
//!
//! No heap is shared. Everything the engine hands out is a [`Buffer`], an
//! exported Arrow stream or a handle: its holder calls the buffer's or the
//! stream's `release` exactly once, and gives each handle back to
//! [`Engine::release`]. What a host hands to the engine and the engine keeps
//! (a bound function, a kept exception or condition, the callbacks' context)
//! is a [`Root`], which the engine releases exactly once. A [`Reply`] that
//! points into host memory carries its own `release`, which the engine calls
//! once after it copied what it keeps.
//!
//! # Threads
//!
//! Calls are synchronous. The engine calls a host's callbacks only on the
//! thread of the entry that received them, while that entry runs, and
//! releases a root on the thread that drops the last owner of it: the
//! entry's, or the thread that releases the handle keeping it.
//! `rust/interface/README.md` ("Ownership, threading and cancellation")
//! states the rules this ABI implements.
use std::ffi::{c_char, c_int, c_void};

use crate::HandleKind;

/// A handle: a slot and its generation in the engine's handle table, never
/// an address. 0 is no handle.
pub type Handle = u64;

/// What an entry answers.
pub type Status = i32;

/// The entry wrote its value to `out`.
pub const DONE: Status = 0;
/// The engine refused the operation; `refusal` holds D39's error text, the
/// issues that explain it and the files a refused save wrote.
pub const REFUSED: Status = 1;
/// The call was cancelled: it wrote nothing, and the host raises the
/// interrupt it kept.
pub const CANCELLED: Status = 2;
/// The boundary refused the call: a bad argument, an expired, busy or other
/// kind of handle, or a contained panic; `refusal` holds the reason alone.
pub const BOUNDARY: Status = 3;

/// Borrowed UTF-8 text: `len` bytes at `ptr`, with no terminator. A null
/// `ptr` is no text, such as an optional argument left out; present text,
/// even empty, has a non-null `ptr`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Str {
    pub ptr: *const u8,
    pub len: usize,
}

impl Str {
    /// No text.
    pub const NONE: Str = Str {
        ptr: std::ptr::null(),
        len: 0,
    };

    /// `text`, borrowed.
    pub fn of(text: &str) -> Str {
        Str {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    /// `text` borrowed, or no text.
    pub fn optional(text: Option<&str>) -> Str {
        text.map_or(Str::NONE, Str::of)
    }
}

/// Memory the engine hands out: `len` items at `ptr`, valid until the holder
/// calls `release` once, which sets `release` to null. A buffer whose
/// `release` is null holds nothing and needs no release.
#[repr(C)]
#[derive(Debug)]
pub struct Buffer<T> {
    pub ptr: *const T,
    pub len: usize,
    pub release: Option<unsafe extern "C" fn(*mut Buffer<T>)>,
    pub private_data: *mut c_void,
}

impl<T> Buffer<T> {
    /// A buffer that holds nothing.
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
            release: None,
            private_data: std::ptr::null_mut(),
        }
    }
}

/// Text the engine hands out, as UTF-8 bytes.
pub type Text = Buffer<u8>;
/// A list of texts the engine hands out, each item borrowed from the buffer.
pub type Texts = Buffer<Str>;
/// `resolve()`'s nested value, as nodes in pre-order.
pub type Nodes = Buffer<Node>;
/// The engine information `info()` returns, one record.
pub type Info = Buffer<InfoRecord>;

/// A host object the engine keeps: a bound function, a kept exception or
/// condition, or the callbacks' context. The engine calls `release`, when it
/// is not null, with `pointer` exactly once, when it no longer needs the
/// object. A null `pointer` is no object, and its `release` is never called.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Root {
    pub pointer: *mut c_void,
    pub release: Option<unsafe extern "C" fn(*mut c_void)>,
}

impl Root {
    /// No object.
    pub const NONE: Root = Root {
        pointer: std::ptr::null_mut(),
        release: None,
    };
}

/// What a refused call wrote.
#[repr(C)]
#[derive(Debug)]
pub struct Refusal {
    /// D39's error text when [`REFUSED`]; the boundary's reason when
    /// [`BOUNDARY`].
    pub message: Text,
    /// When [`REFUSED`], the issues table that explains the refusal, a table
    /// handle the holder releases; 0 when [`BOUNDARY`].
    pub issues: Handle,
    /// When [`REFUSED`], the files a refused save wrote before it stopped,
    /// in publication order; empty otherwise.
    pub paths: Texts,
}

/// The host language of a call: none, for an engine that claims none (D123).
pub const HOST_NONE: u32 = 0;
/// The Python host.
pub const HOST_PYTHON: u32 = 1;
/// The R host.
pub const HOST_R: u32 = 2;

/// The options of one call: the host, its runtime and its callbacks. An
/// entry that takes a call takes its `context`, whatever it answers, and
/// releases it once when it no longer needs the callbacks. Except for
/// `spec_stream`, this is at the latest as it returns. A streamed call keeps
/// an owned context through its run and readers; its probe may run on a reader's
/// thread and must guard access to its interpreter. Project functions run only
/// on the thread that started the run. A callback left null is one the host does not offer: without
/// `call_batch` the engine calls `invoke` row by row, without `is_interrupt`
/// a failure of kind [`ERROR_INTERRUPTED`] is the host's interrupt, and
/// without the lock callbacks the engine refuses to verify a declared lock.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Call {
    /// The size of the caller's struct, in bytes.
    pub size: u32,
    /// [`HOST_PYTHON`], [`HOST_R`] or [`HOST_NONE`].
    pub host: u32,
    /// The runtime's name, such as `python` or `r`.
    pub runtime_name: Str,
    /// The runtime's version; `info()` and `selftest()` report it as the
    /// host version.
    pub runtime_version: Str,
    /// The callbacks' context, which every callback receives.
    pub context: Root,
    /// Whether the call is cancelled, asked at each of the engine's
    /// cancellation points (D74).
    pub probe: Option<Probe>,
    /// The host-functions port (#3261): binding a project function.
    pub bind: Option<Bind>,
    /// Calling a bound project function once.
    pub invoke: Option<Invoke>,
    /// Calling a bound project function once per batch.
    pub call_batch: Option<CallBatch>,
    /// Whether a function failure is the host's interrupt.
    pub is_interrupt: Option<IsInterrupt>,
    /// The five installed-distribution queries with which the engine
    /// verifies a uv lock (Python, D172).
    pub providers: Option<Query>,
    pub standard_library: Option<Query>,
    pub distribution_version: Option<Query>,
    pub environment: Option<Query>,
    pub integer_digits: Option<Query>,
    /// The comparison of the versions the engine decoded from an renv lock
    /// with the installed ones (R).
    pub compare_locked: Option<CompareLocked>,
}

/// The call goes on.
pub const GO_ON: c_int = 0;
/// The call is cancelled: the host has an interrupt pending, which it keeps
/// and raises again once the call returns.
pub const CANCEL: c_int = 1;

/// Whether the call is cancelled: [`GO_ON`] or [`CANCEL`]. Any other answer
/// is a fault of the host, which the call answers as [`BOUNDARY`].
pub type Probe = unsafe extern "C" fn(context: *mut c_void) -> c_int;

/// A callback answered: the reply holds its answer.
pub const ANSWERED: c_int = 0;
/// A callback failed: `reply.error` holds the failure.
pub const RAISED: c_int = 1;
/// `invoke` or `call_batch` could not represent what the function returned:
/// `reply.error` holds why.
pub const INVALID: c_int = 2;

/// Resolve `identity` to a host callable with the parameters of `signature`,
/// without calling project code: [`ANSWERED`] with `reply.function`, or
/// [`RAISED`].
pub type Bind = unsafe extern "C" fn(
    context: *mut c_void,
    identity: *const Identity,
    signature: *const Signature,
    reply: *mut Reply,
) -> c_int;
/// Call the bound `function` once with named arguments in declaration
/// order: [`ANSWERED`] with `reply.scalar`, [`RAISED`] or [`INVALID`].
pub type Invoke = unsafe extern "C" fn(
    context: *mut c_void,
    function: *mut c_void,
    arguments: *const Argument,
    arguments_len: usize,
    reply: *mut Reply,
) -> c_int;
/// Call the bound `function` once for `rows` rows: [`ANSWERED`] with one
/// result per row in `reply.scalars`, [`RAISED`] or [`INVALID`].
pub type CallBatch = unsafe extern "C" fn(
    context: *mut c_void,
    function: *mut c_void,
    rows: usize,
    arguments: *const BatchArgument,
    arguments_len: usize,
    reply: *mut Reply,
) -> c_int;
/// Whether a failure of `kind`, an `ERROR_*` code or 0 for a failure of the
/// engine's own, holding the kept host object `payload` or null, is the
/// host's interrupt: 1 or 0.
pub type IsInterrupt =
    unsafe extern "C" fn(context: *mut c_void, kind: u32, payload: *mut c_void) -> c_int;
/// One installed-distribution query, asked with its texts: [`ANSWERED`] or
/// [`RAISED`]. `providers` is asked the top-level modules and answers
/// (module, distribution) pairs in `reply.texts`; `standard_library` is asked
/// one module and answers a boolean `reply.scalar`; `distribution_version` is
/// asked one distribution and answers its version as text, a missing scalar
/// when none of that name is installed, or any other kind when its metadata
/// records no version text; `environment` is asked nothing and answers the
/// marker environment as (name, value) pairs; `integer_digits` is asked
/// nothing and answers the interpreter's limit on the digits of an integer
/// read from text as an integer, 0 when the limit is off.
pub type Query = unsafe extern "C" fn(
    context: *mut c_void,
    texts: *const Str,
    texts_len: usize,
    reply: *mut Reply,
) -> c_int;
/// Compare the locked versions, (package, version) pairs in `packages`, with
/// the installed ones for the functions whose logical calls are `calls`:
/// [`ANSWERED`] with the findings in `reply.texts`, four items each (the
/// package, the reason, the expected version or no text, the actual version
/// or no text), or [`RAISED`].
pub type CompareLocked = unsafe extern "C" fn(
    context: *mut c_void,
    packages: *const Str,
    packages_len: usize,
    calls: *const Str,
    calls_len: usize,
    reply: *mut Reply,
) -> c_int;

/// What a callback answers into. The engine passes it with every field null
/// and takes every [`Root`] in it, whatever the answer: a root it does not
/// keep it releases at once.
#[repr(C)]
#[derive(Debug)]
pub struct Reply {
    /// `bind`: the bound function, which the engine keeps.
    pub function: Root,
    /// `invoke`, `standard_library`, `distribution_version` and
    /// `integer_digits`: the answer.
    pub scalar: Scalar,
    /// [`RAISED`] and [`INVALID`]: the failure.
    pub error: HostError,
    /// Releases the host memory this reply points into, or null when it
    /// points into none; the engine calls it once, after it copied what it
    /// keeps, whatever the answer.
    pub release: Option<unsafe extern "C" fn(*mut Reply)>,
    pub private_data: *mut c_void,
    /// `call_batch`: one result per row.
    pub scalars: *const Scalar,
    pub scalars_len: usize,
    /// `providers`, `environment` and `compare_locked`: the answer's texts.
    pub texts: *const Str,
    pub texts_len: usize,
}

impl Reply {
    /// A reply with every field null.
    pub const fn empty() -> Self {
        Self {
            function: Root::NONE,
            scalar: Scalar::MISSING,
            error: HostError::NONE,
            release: None,
            private_data: std::ptr::null_mut(),
            scalars: std::ptr::null(),
            scalars_len: 0,
            texts: std::ptr::null(),
            texts_len: 0,
        }
    }
}

/// A raised host exception: `class`, `message` and the original `payload`.
pub const ERROR_EXCEPTION: u32 = 1;
/// The host's interrupt, with the original `payload`.
pub const ERROR_INTERRUPTED: u32 = 2;
/// A returned value the host refused: `message` is the reason, and
/// `returned` the value's type or no text.
pub const ERROR_REJECTED: u32 = 3;
/// A value the host could not represent: `message`, and the `payload`.
pub const ERROR_REPRESENTATION: u32 = 4;
/// A failure of the host's own SDK, with the `payload`.
pub const ERROR_SDK_BOUNDARY: u32 = 5;
/// A failure of the host binding itself, such as a panic it contained:
/// `message` is its code, `function_boundary`, `host_classification`,
/// `host_port` or `host_thread`.
pub const ERROR_INTERNAL: u32 = 6;
/// A reply outside the protocol.
pub const ERROR_PROTOCOL: u32 = 7;
/// No callable for the logical call in `message`.
pub const ERROR_MISSING_BINDING: u32 = 8;

/// A host failure, one of the facade's function errors (#3261).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HostError {
    /// One of the `ERROR_*` codes.
    pub kind: u32,
    pub class: Str,
    pub message: Str,
    pub returned: Str,
    /// The original host object, which the failure keeps for as long as the
    /// result that retains it lives.
    pub payload: Root,
}

impl HostError {
    /// No failure.
    pub const NONE: HostError = HostError {
        kind: 0,
        class: Str::NONE,
        message: Str::NONE,
        returned: Str::NONE,
        payload: Root::NONE,
    };
}

/// A missing value.
pub const SCALAR_MISSING: u32 = 0;
/// Text, in `text`.
pub const SCALAR_TEXT: u32 = 1;
/// An integer, in `int`.
pub const SCALAR_INT: u32 = 2;
/// A float, in `float`; a value that is not finite is missing.
pub const SCALAR_FLOAT: u32 = 3;
/// A boolean, 0 or 1 in `int`.
pub const SCALAR_BOOL: u32 = 4;
/// A date, as whole days since 1970-01-01 in `int`.
pub const SCALAR_DATE: u32 = 5;
/// A zone-free datetime, as whole seconds since 1970-01-01T00:00:00 in
/// `int`.
pub const SCALAR_DATETIME: u32 = 6;

/// A function's argument or result: one of the facade's scalars.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Scalar {
    /// One of the `SCALAR_*` codes.
    pub kind: u32,
    pub int: i64,
    pub float: f64,
    pub text: Str,
}

impl Scalar {
    /// The missing value.
    pub const MISSING: Scalar = Scalar {
        kind: SCALAR_MISSING,
        int: 0,
        float: 0.0,
        text: Str::NONE,
    };
}

/// A project function's identity: its name and the logical call a host
/// resolves.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub name: Str,
    pub call: Str,
}

/// A function's declared result: text.
pub const COLUMN_STR: u32 = 0;
/// An integer.
pub const COLUMN_INT: u32 = 1;
/// A float.
pub const COLUMN_FLOAT: u32 = 2;
/// A date.
pub const COLUMN_DATE: u32 = 3;
/// A datetime.
pub const COLUMN_DATETIME: u32 = 4;

/// A function's admitted signature: its parameters' names in declaration
/// order, its declared result (`COLUMN_*`) and whether it may return a
/// missing value (0 or 1).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Signature {
    pub parameters: *const Str,
    pub parameters_len: usize,
    pub returns: u32,
    pub may_return_missing: u32,
}

/// One named argument.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Argument {
    pub name: Str,
    pub value: Scalar,
}

/// One named batch argument: one value per row.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BatchArgument {
    pub name: Str,
    pub values: *const Scalar,
    pub values_len: usize,
}

/// A null.
pub const NODE_NULL: u32 = 0;
/// A boolean, 0 or 1 in `int`.
pub const NODE_BOOL: u32 = 1;
/// An integer within 64 bits, in `int`.
pub const NODE_INT: u32 = 2;
/// A float, in `float`.
pub const NODE_FLOAT: u32 = 3;
/// Text, in `text`.
pub const NODE_TEXT: u32 = 4;
/// A number no 64-bit type holds exactly, as its exact spelling in `text`:
/// an integer beyond 64 bits, or a float beyond binary64's finite range.
pub const NODE_NUMBER: u32 = 5;
/// A list of `children` items.
pub const NODE_LIST: u32 = 6;
/// A mapping of `children` entries, each a node with its `key`.
pub const NODE_MAP: u32 = 7;

/// One node of a nested value, in pre-order: a list's or mapping's children
/// follow it, each with its own children before the next.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Node {
    /// One of the `NODE_*` codes.
    pub kind: u32,
    /// A mapping entry's key; no text for the root and for a list's items.
    pub key: Str,
    /// How many direct children a list or mapping has.
    pub children: usize,
    pub int: i64,
    pub float: f64,
    pub text: Str,
}

/// The engine information (D41, P-INFO-1), its six fields in order.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InfoRecord {
    pub version: Str,
    pub language_versions: *const Str,
    pub language_versions_len: usize,
    pub features: *const Str,
    pub features_len: usize,
    pub host: Str,
    /// No text for the command line.
    pub host_version: Str,
    pub platform: Str,
}

/// How `read_odm` reads an ODM file.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OdmOptions {
    /// The archive member to read, or no text.
    pub archive_member: Str,
    pub max_archive_members: u64,
    pub max_expanded_bytes: u64,
    pub max_xml_depth: u64,
}

/// What the engine holds for its holders, for the lifetime tests.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Census {
    /// The live handles of each kind, in the order of [`HandleKind::ALL`].
    pub handles: [u64; HandleKind::ALL.len()],
    /// The buffers handed out and not released yet.
    pub buffers: u64,
    /// The exported Arrow streams not released yet.
    pub streams: u64,
    /// The Arrow schemas and arrays the streams emitted, not released yet.
    pub arrays: u64,
}

/// A negotiation request, kept in every boundary version up to its
/// `version`. A request whose `size` stops before the lists, or whose
/// version is not this boundary version, is read no further.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Request {
    /// The size of the caller's struct, in bytes.
    pub size: u32,
    /// The boundary version asked for; 0 is malformed.
    pub version: u32,
    pub required: *const Str,
    pub required_len: usize,
    pub optional: *const Str,
    pub optional_len: usize,
}

/// The request was accepted.
pub const REASON_NONE: u32 = 0;
/// `version_mismatch`.
pub const REASON_VERSION_MISMATCH: u32 = 1;
/// `malformed_request`.
pub const REASON_MALFORMED_REQUEST: u32 = 2;
/// `missing_feature`: the required identifiers that `supported` lacks, in
/// request order, are missing.
pub const REASON_MISSING_FEATURE: u32 = 3;
/// `missing_interface`, which only a host gives, finding no entry.
pub const REASON_MISSING_INTERFACE: u32 = 4;

/// What a negotiation answers. It points only at static data, so it needs
/// no release. The engine writes no field past the caller's `size`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    /// The size of the caller's struct, in bytes.
    pub size: u32,
    /// 1 when accepted, 0 when refused.
    pub accepted: u32,
    /// The version the request asked for; 0 when its version field is
    /// malformed.
    pub requested_version: u32,
    /// The engine's boundary version.
    pub supported_version: u32,
    /// One of the `REASON_*` codes.
    pub reason: u32,
    /// From here, written on acceptance and on a missing feature: every
    /// identifier the engine supports, in the boundary's order. The host
    /// enables the required identifiers and then the supported optional
    /// ones, in request order.
    pub supported: *const Str,
    pub supported_len: usize,
    /// The engine's release.
    pub core_version: Str,
    /// The language versions the engine reads, as `info()` reports them.
    pub language_versions: *const Str,
    pub language_versions_len: usize,
}

/// An entry of a call: [`Engine::selftest`] and [`Engine::info`].
pub type OnCall<T> =
    unsafe extern "C" fn(call: *const Call, out: *mut T, refusal: *mut Refusal) -> Status;
/// An entry of one path, read without the engine: the printed lines of a
/// lazy object (D37) and a file type's schema.
pub type OnPath<T> = unsafe extern "C" fn(path: Str, out: *mut T, refusal: *mut Refusal) -> Status;
/// An entry of a call and one path: a project's check and its names.
pub type OnFile<T> = unsafe extern "C" fn(
    call: *const Call,
    path: Str,
    out: *mut T,
    refusal: *mut Refusal,
) -> Status;
/// An entry of a call, a path and an optional second text: a specification
/// with its project or none, or a project with a name.
pub type OnPair<T> = unsafe extern "C" fn(
    call: *const Call,
    path: Str,
    second: Str,
    out: *mut T,
    refusal: *mut Refusal,
) -> Status;
/// A run: a specification with its project, or a project with its section,
/// reusing the plans holder `plans`, or none for 0.
pub type OnRun = unsafe extern "C" fn(
    call: *const Call,
    path: Str,
    second: Str,
    plans: Handle,
    out: *mut Handle,
    refusal: *mut Refusal,
) -> Status;
/// A result accessor: a result handle and, for a project run's member, its
/// name, or no text for the result itself.
pub type OnMember<T> =
    unsafe extern "C" fn(result: Handle, member: Str, out: *mut T, refusal: *mut Refusal) -> Status;
/// An entry of one handle: a result's or a table's.
pub type OnHandle<T> =
    unsafe extern "C" fn(handle: Handle, out: *mut T, refusal: *mut Refusal) -> Status;

/// The engine's function table, which a negotiation returns on acceptance.
/// A host reads no entry past `size`. An entry that answers a handle hands
/// out a new one, which the host gives back to `release`.
#[repr(C)]
pub struct Engine {
    pub size: u32,
    /// The boundary version the table implements.
    pub version: u32,
    pub spec_check: OnPair<Handle>,
    pub project_check: OnFile<Handle>,
    pub spec_run: OnRun,
    pub project_run: OnRun,
    pub project_submission: unsafe extern "C" fn(
        call: *const Call,
        project: Str,
        section: Str,
        created: Str,
        out: *mut Handle,
        refusal: *mut Refusal,
    ) -> Status,
    /// A table handle; null `options` reads with the defaults.
    pub read_odm: unsafe extern "C" fn(
        call: *const Call,
        source: Str,
        options: *const OdmOptions,
        out: *mut Handle,
        refusal: *mut Refusal,
    ) -> Status,
    /// A table handle; `fix` is 0 or 1.
    pub lint: unsafe extern "C" fn(
        paths: *const Str,
        paths_len: usize,
        fix: u32,
        out: *mut Handle,
        refusal: *mut Refusal,
    ) -> Status,
    pub selftest: OnCall<Handle>,
    pub plans_new: unsafe extern "C" fn(out: *mut Handle, refusal: *mut Refusal) -> Status,
    pub spec_resolve: OnPair<Nodes>,
    pub project_spec: OnPair<Text>,
    pub project_names: OnFile<Texts>,
    pub spec_summary: OnPath<Texts>,
    pub project_summary: OnPath<Texts>,
    pub schema: OnPath<Text>,
    pub info: OnCall<Info>,
    pub status: OnMember<Text>,
    /// A table handle.
    pub issues: OnMember<Handle>,
    /// The printed lines (D37).
    pub summary: OnMember<Texts>,
    /// A table handle, or 0 when the run built no output.
    pub output: OnMember<Handle>,
    /// A table handle, or 0 when the run never reached execution.
    pub report: OnMember<Handle>,
    /// The paths written; the call's probe can cancel it.
    pub save: unsafe extern "C" fn(
        call: *const Call,
        result: Handle,
        out: *mut Texts,
        refusal: *mut Refusal,
    ) -> Status,
    pub names: OnHandle<Texts>,
    pub created: OnHandle<Text>,
    /// Private installed-run evidence, or an empty buffer when the run has
    /// none; `include_measurements` is 0 or 1.
    pub observations: unsafe extern "C" fn(
        result: Handle,
        include_measurements: u32,
        out: *mut Text,
        refusal: *mut Refusal,
    ) -> Status,
    /// A selftest's cases, a table handle.
    pub cases: OnHandle<Handle>,
    /// The engine information that ran a selftest's cases.
    pub selftest_info: OnHandle<Info>,
    pub rows: OnHandle<u64>,
    pub columns: OnHandle<Texts>,
    /// The table's printed preview (D167).
    pub preview: OnHandle<Text>,
    /// A fresh Arrow C stream over the table, with its own cursor over
    /// shared immutable buffers, into `out`, which holds a released stream.
    pub export: OnHandle<ArrowArrayStream>,
    /// Give a handle back; releasing a released handle, or 0, does nothing.
    pub release: unsafe extern "C" fn(handle: Handle) -> Status,
    pub census: unsafe extern "C" fn(out: *mut Census) -> Status,
    /// The issues table of no issues, a table handle, which a host attaches
    /// to a refusal of its own.
    pub no_issues: unsafe extern "C" fn(out: *mut Handle, refusal: *mut Refusal) -> Status,
    /// Start a streamed run. Unlike `spec_run`, this entry retains the call's
    /// callbacks and context until the run and all its readers are released.
    /// The context must therefore be owned, with a release callback.
    pub spec_stream: OnRun,
    /// Whether a run or table holds a streamed run's output (1 or 0).
    pub is_stream: OnHandle<u32>,
    /// The release's condition catalogue as a table handle.
    pub conditions: unsafe extern "C" fn(out: *mut Handle, refusal: *mut Refusal) -> Status,
    /// The check of a fragment, `path`, on the parent a caller names, with
    /// its project or none (#3694): a check handle, or the refusal of a
    /// parent given for a specification that declares `parent` or `include`.
    pub spec_check_on: unsafe extern "C" fn(
        call: *const Call,
        path: Str,
        project: Str,
        parent: Str,
        out: *mut Handle,
        refusal: *mut Refusal,
    ) -> Status,
}

/// The Arrow C data interface's schema.
#[repr(C)]
#[derive(Debug)]
pub struct ArrowSchema {
    pub format: *const c_char,
    pub name: *const c_char,
    pub metadata: *const c_char,
    pub flags: i64,
    pub n_children: i64,
    pub children: *mut *mut ArrowSchema,
    pub dictionary: *mut ArrowSchema,
    pub release: Option<unsafe extern "C" fn(*mut ArrowSchema)>,
    pub private_data: *mut c_void,
}

/// The Arrow C data interface's array.
#[repr(C)]
#[derive(Debug)]
pub struct ArrowArray {
    pub length: i64,
    pub null_count: i64,
    pub offset: i64,
    pub n_buffers: i64,
    pub n_children: i64,
    pub buffers: *mut *const c_void,
    pub children: *mut *mut ArrowArray,
    pub dictionary: *mut ArrowArray,
    pub release: Option<unsafe extern "C" fn(*mut ArrowArray)>,
    pub private_data: *mut c_void,
}

/// The Arrow C stream interface's stream.
#[repr(C)]
#[derive(Debug)]
pub struct ArrowArrayStream {
    pub get_schema: Option<unsafe extern "C" fn(*mut ArrowArrayStream, *mut ArrowSchema) -> c_int>,
    pub get_next: Option<unsafe extern "C" fn(*mut ArrowArrayStream, *mut ArrowArray) -> c_int>,
    pub get_last_error: Option<unsafe extern "C" fn(*mut ArrowArrayStream) -> *const c_char>,
    pub release: Option<unsafe extern "C" fn(*mut ArrowArrayStream)>,
    pub private_data: *mut c_void,
}

impl ArrowArrayStream {
    /// A released stream, which an export may fill.
    pub const fn released() -> Self {
        Self {
            get_schema: None,
            get_next: None,
            get_last_error: None,
            release: None,
            private_data: std::ptr::null_mut(),
        }
    }
}
