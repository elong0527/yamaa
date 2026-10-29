//! The engine boundary that both host bindings cross: the interface version
//! and feature handshake, its refusal envelope, the gate that every native
//! call passes, and the census of live native handles.
//!
//! `rust/interface/README.md` ("Version and feature handshake") is the
//! contract. The generated Python or R layer of a host package sends one
//! request through its binding's handshake export before any other native
//! call. The binding converts the host's request into a [`Value`], and its
//! [`Gate`] answers with an [`Outcome`]: the accepted record, or the
//! five-field refusal envelope. The first acceptance opens the gate; every
//! other native call of the binding passes it first and is refused until
//! then. A later call of the handshake answers its request and changes
//! nothing.
//!
//! Only the boundary version is negotiated, by exact equality: no ranges and
//! no retry with a lower version. The check order stays the same in every
//! boundary version, so that any two yamaa builds refuse each other cleanly:
//!
//! 1. The request must be a mapping that holds one `version`, a positive
//!    integer of at most 32 bits and never a boolean, or it is malformed,
//!    and the refusal names no requested version.
//! 2. The version must equal [`BOUNDARY_VERSION`], or it is a mismatch, and
//!    nothing else in the request is read.
//! 3. The request must hold exactly `version`, `required` and `optional`,
//!    each list must hold identifiers (`[a-z][a-z0-9_]*`), and no identifier
//!    may appear twice within or across them, or it is malformed.
//! 4. Every required identifier must be supported, or the refusal names the
//!    missing ones in request order.
//! 5. Otherwise the request is accepted: the required identifiers and then
//!    the supported optional ones, in request order, are enabled.
//!
//! Negotiation reads nothing, calls nothing and publishes nothing. The crate
//! depends on nothing else in the workspace, and the boundary version and the
//! identifiers are internal (#3030 D225): any release may change them with a
//! new boundary version, and an identifier is never reused with another
//! meaning.
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The boundary version, which the generated layers of both hosts request.
/// It moves with any change to what crosses the boundary that an older side
/// would misread.
pub const BOUNDARY_VERSION: u32 = 2;

/// The capability identifiers that every engine of this boundary version
/// supports, in order.
pub const CAPABILITIES: [&str; 3] = ["structured_records", "arrow_c_stream", "plan_reuse"];

/// Each format identifier with the compiled feature that names it in
/// `info()`'s `features`, in order. An engine supports the identifier of each
/// feature it compiles.
pub const FORMATS: [(&str, &str); 4] = [
    ("format_yaml", "yaml"),
    ("format_parquet", "parquet"),
    ("format_readstat", "readstat"),
    ("format_odm", "odm"),
];

/// The fields of a request, each exactly once.
const FIELDS: [&str; 3] = ["version", "required", "optional"];

/// How deep a request nests below its mapping: a field's list, then that
/// list's items. A binding converts no deeper, so deeper input reads as
/// [`Value::Other`] whatever its size, and the conversion never recurses
/// past it.
pub const DEPTH: usize = 2;

/// A request as a host built it, in the shapes that the check order tells
/// apart. Each binding converts its host's object structurally; nothing here
/// knows a host type.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// An exact integer within 64 bits: a Python `int` that is not a `bool`,
    /// or an element of an R integer vector, or a whole, finite element of an
    /// R double vector, as R writes integers.
    Integer(i64),
    /// Text.
    Text(String),
    /// A sequence: a Python list or tuple, or an R list without names.
    List(Vec<Value>),
    /// An R atomic vector, which reads as its element where it has one, and
    /// as a sequence of any length.
    Vector(Vec<Value>),
    /// The request's mapping, in order: a Python dict, or an R list with
    /// names. A key that is not text, or a name R cannot give as text, reads
    /// as the empty key, which names no field, so the check order still reads
    /// the version first.
    Map(Vec<(String, Value)>),
    /// Anything else, which no field of a request holds: a boolean, a float,
    /// a missing value, an integer beyond 64 bits, or anything nested deeper
    /// than [`DEPTH`].
    Other,
}

impl Value {
    /// The value that an R vector of one element reads as; any other value
    /// reads as itself.
    fn scalar(&self) -> &Value {
        match self {
            Value::Vector(items) => match items.as_slice() {
                [item] => item,
                _ => self,
            },
            other => other,
        }
    }

    /// The items of a sequence: a list, or an R vector of any length.
    fn items(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) | Value::Vector(items) => Some(items),
            _ => None,
        }
    }
}

/// Why a negotiation refused, one of the envelope's four reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The request asks for another boundary version.
    VersionMismatch,
    /// The request is not a well-formed request.
    MalformedRequest,
    /// A required identifier is unknown or unavailable.
    MissingFeature,
    /// The native side has no handshake. Only a generated layer that finds
    /// no handshake export gives this reason; no negotiation does.
    MissingInterface,
}

impl Reason {
    /// The reason as the envelope spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Reason::VersionMismatch => "version_mismatch",
            Reason::MalformedRequest => "malformed_request",
            Reason::MissingFeature => "missing_feature",
            Reason::MissingInterface => "missing_interface",
        }
    }
}

/// The refusal envelope, whose fields keep their meaning in every boundary
/// version; its `accepted` field, always false, is the host's to write. It
/// carries no text: each host renders the message of its reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The version the request asked for, or `None` when its version field
    /// is malformed.
    pub requested_version: Option<u32>,
    /// The version this side supports, which a negotiation always names;
    /// `None` only where the native side has no handshake.
    pub supported_version: Option<u32>,
    /// Why the negotiation refused.
    pub reason: Reason,
    /// The required identifiers this side lacks, in request order; empty for
    /// any other reason.
    pub missing_features: Vec<String>,
}

/// The accepted record; its `accepted` field, always true, is the host's to
/// write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    /// The negotiated boundary version.
    pub version: u32,
    /// The required identifiers and then the supported optional ones, in
    /// request order.
    pub enabled: Vec<String>,
    /// Every identifier the engine supports, in the boundary's order.
    pub supported: Vec<String>,
    /// The engine's release, which nothing negotiates.
    pub core_version: String,
    /// The language versions the engine reads, as `info()` reports them,
    /// which nothing negotiates.
    pub language_versions: Vec<String>,
}

/// What a negotiation answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The request is accepted.
    Accepted(Accepted),
    /// The request is refused.
    Refused(Refusal),
}

/// What an engine reports when it accepts a request: every identifier it
/// supports, its release and the language versions it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineFacts {
    supported: Vec<String>,
    core_version: String,
    language_versions: Vec<String>,
}

impl EngineFacts {
    /// The facts of an engine that compiles `features`, its release
    /// `core_version` and its `language_versions`, as `info()` reports them.
    pub fn new(
        features: &[impl AsRef<str>],
        core_version: &str,
        language_versions: &[impl AsRef<str>],
    ) -> Self {
        let compiled = |feature: &str| features.iter().any(|compiled| compiled.as_ref() == feature);
        let formats = FORMATS
            .iter()
            .filter(|(_, feature)| compiled(feature))
            .map(|(identifier, _)| *identifier);
        Self {
            supported: CAPABILITIES
                .iter()
                .copied()
                .chain(formats)
                .map(str::to_owned)
                .collect(),
            core_version: core_version.to_owned(),
            language_versions: language_versions
                .iter()
                .map(|version| version.as_ref().to_owned())
                .collect(),
        }
    }

    fn supports(&self, identifier: &str) -> bool {
        self.supported
            .iter()
            .any(|supported| supported == identifier)
    }
}

/// Answer `request` for `engine` in the check order of the module
/// documentation. Negotiation has no side effect; [`Gate::negotiate`] also
/// opens a binding's gate.
pub fn negotiate(request: &Value, engine: &EngineFacts) -> Outcome {
    let refuse = |requested_version, reason, missing_features| {
        Outcome::Refused(Refusal {
            requested_version,
            supported_version: Some(BOUNDARY_VERSION),
            reason,
            missing_features,
        })
    };
    let Value::Map(fields) = request else {
        return refuse(None, Reason::MalformedRequest, Vec::new());
    };
    let Some(version) = requested_version(fields) else {
        return refuse(None, Reason::MalformedRequest, Vec::new());
    };
    if version != BOUNDARY_VERSION {
        return refuse(Some(version), Reason::VersionMismatch, Vec::new());
    }
    let Some((required, optional)) = feature_lists(fields) else {
        return refuse(Some(version), Reason::MalformedRequest, Vec::new());
    };
    let missing: Vec<String> = required
        .iter()
        .filter(|identifier| !engine.supports(identifier))
        .map(|identifier| (*identifier).to_owned())
        .collect();
    if !missing.is_empty() {
        return refuse(Some(version), Reason::MissingFeature, missing);
    }
    let enabled = required
        .iter()
        .chain(
            optional
                .iter()
                .filter(|identifier| engine.supports(identifier)),
        )
        .map(|identifier| (*identifier).to_owned())
        .collect();
    Outcome::Accepted(Accepted {
        version,
        enabled,
        supported: engine.supported.clone(),
        core_version: engine.core_version.clone(),
        language_versions: engine.language_versions.clone(),
    })
}

/// The field `name` where the mapping holds it exactly once.
fn field<'a>(fields: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    let mut found = fields.iter().filter(|(key, _)| key == name);
    match (found.next(), found.next()) {
        (Some((_, value)), None) => Some(value),
        _ => None,
    }
}

/// Step 1: the one `version` field, a positive integer of at most 32 bits.
fn requested_version(fields: &[(String, Value)]) -> Option<u32> {
    match field(fields, "version")?.scalar() {
        Value::Integer(version) => u32::try_from(*version).ok().filter(|version| *version > 0),
        _ => None,
    }
}

/// Step 3: exactly the three fields, and two lists of identifiers with no
/// identifier twice within or across them.
fn feature_lists(fields: &[(String, Value)]) -> Option<(Vec<&str>, Vec<&str>)> {
    // `version` is there once (step 1), so three fields that hold `required`
    // and `optional` once each are exactly the three.
    if fields.len() != FIELDS.len() {
        return None;
    }
    let required = identifiers(field(fields, "required")?)?;
    let optional = identifiers(field(fields, "optional")?)?;
    let all: Vec<&str> = required.iter().chain(&optional).copied().collect();
    let repeated = all
        .iter()
        .enumerate()
        .any(|(index, identifier)| all[..index].contains(identifier));
    (!repeated).then_some((required, optional))
}

/// The identifiers of a list, or `None` when it is no list of identifiers.
fn identifiers(value: &Value) -> Option<Vec<&str>> {
    value
        .items()?
        .iter()
        .map(|item| match item.scalar() {
            Value::Text(text) if is_identifier(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Whether `text` is an identifier: `[a-z][a-z0-9_]*`.
fn is_identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// A binding's gate: closed until a negotiation is accepted, then open for
/// the life of the process; nothing closes it again. A binding keeps one in
/// a `static`, so a replaced native module starts closed.
#[derive(Debug, Default)]
pub struct Gate {
    open: AtomicBool,
}

impl Gate {
    /// A closed gate.
    pub const fn new() -> Self {
        Self {
            open: AtomicBool::new(false),
        }
    }

    /// Answer `request` for `engine`; an acceptance opens the gate, and a
    /// refusal leaves it as it was.
    pub fn negotiate(&self, request: &Value, engine: &EngineFacts) -> Outcome {
        let outcome = negotiate(request, engine);
        if matches!(outcome, Outcome::Accepted(_)) {
            self.open.store(true, Ordering::Release);
        }
        outcome
    }

    /// Let a native call through once a negotiation was accepted.
    pub fn pass(&self) -> Result<(), Closed> {
        if self.open.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Closed)
        }
    }
}

/// The refusal of a native call before the interface handshake succeeded,
/// which each binding gives as its ordinary boundary refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closed;

impl fmt::Display for Closed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "yamaa's native module has not negotiated its interface: load it \
             through the yamaa package, which negotiates first, or reinstall yamaa",
        )
    }
}

impl std::error::Error for Closed {}

/// The kinds of native handle that a census counts, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKind {
    /// A check's result.
    Check,
    /// A specification run.
    Run,
    /// A project run.
    ProjectRun,
    /// A submission.
    Submission,
    /// A selftest.
    SelfTest,
    /// The compiled plans that one lazy object keeps.
    Plans,
    /// A table, which only the Python binding hands out.
    Table,
}

impl HandleKind {
    /// Every kind, in the census's order.
    pub const ALL: [HandleKind; 7] = [
        HandleKind::Check,
        HandleKind::Run,
        HandleKind::ProjectRun,
        HandleKind::Submission,
        HandleKind::SelfTest,
        HandleKind::Plans,
        HandleKind::Table,
    ];

    /// The kind as the census names it.
    pub const fn as_str(self) -> &'static str {
        match self {
            HandleKind::Check => "check",
            HandleKind::Run => "run",
            HandleKind::ProjectRun => "project_run",
            HandleKind::Submission => "submission",
            HandleKind::SelfTest => "selftest",
            HandleKind::Plans => "plans",
            HandleKind::Table => "table",
        }
    }
}

/// The live native handles of one binding, by kind: each handle holds a
/// [`Live`] from its census while it lives. A binding keeps one in a
/// `static`; the lifetime suites read it through the binding's private
/// census accessor.
#[derive(Debug, Default)]
pub struct Census {
    live: [AtomicUsize; HandleKind::ALL.len()],
}

impl Census {
    /// A census with no live handle.
    pub const fn new() -> Self {
        Self {
            live: [const { AtomicUsize::new(0) }; HandleKind::ALL.len()],
        }
    }

    /// Count one live handle of `kind` until the returned [`Live`] drops.
    #[must_use = "the handle counts only while its Live is held"]
    pub fn hold(&'static self, kind: HandleKind) -> Live {
        self.live[kind as usize].fetch_add(1, Ordering::Relaxed);
        Live { census: self, kind }
    }

    /// The live handles of each kind, in the census's order.
    pub fn counts(&self) -> [(HandleKind, usize); HandleKind::ALL.len()] {
        HandleKind::ALL.map(|kind| (kind, self.live[kind as usize].load(Ordering::Relaxed)))
    }
}

/// One live handle in its binding's census, counted until it drops.
#[derive(Debug)]
pub struct Live {
    census: &'static Census,
    kind: HandleKind,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.census.live[self.kind as usize].fetch_sub(1, Ordering::Relaxed);
    }
}
