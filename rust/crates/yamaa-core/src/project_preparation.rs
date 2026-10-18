//! Capture, structurally decode and admit project dependencies through native ports.
//! Code activation, study data and publication authority are absent from this service.
//!
//! The caller supplies one project file. Its `environments` entry for the host
//! language links that language's environment file; every other entry stays
//! unread. Paths the project writes resolve from the project file, and the lock
//! and function paths an environment file writes resolve from that file.
use crate::{
    limits::{LimitExceeded, Limits},
    project_document,
    project_environment::{Draft, Environment, Finding, LockKind, LockReference},
    project_environment_document::{self, Declaration, EnvironmentSources},
    project_function::Language,
    project_function_document,
    project_terminology::Source as TerminologySource,
    project_terminology_document,
    schema::Document,
};

/// Retained host identity and exact byte snapshot; identity is never a digest.
#[derive(Clone, Debug)]
pub struct Source {
    pub identity: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Environment,
    Lock,
    Function,
    Codelist,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The environment file a project `environments` key links.
    Environment(Language),
    Lock,
    Function(String),
    Codelist(usize),
}
/// One metadata dependency request; each captured file uses the shared size cap.
pub struct Request<'a> {
    pub kind: Kind,
    pub declaring_source: &'a str,
    pub written: &'a str,
}
#[derive(Debug)]
pub enum Reply {
    Document(Source),
    /// The host parses the held packaging-tool syntax to classify the lock.
    /// Installed package versions are verified only by the activation port.
    Lock {
        source: Source,
        kind: LockKind,
    },
}
pub trait ProjectCapturePort {
    type Error;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error>;
    /// Hosts identify user interrupts without changing their original payload.
    /// An interrupt aborts immediately; ordinary independent failures collect.
    fn is_interrupt(&self, _error: &Self::Error) -> bool {
        false
    }
}

/// Closed structural targets are selected by the application, never by a host callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeTarget {
    Project,
    Environment,
    Function,
    Codelist,
}

/// Decode and structurally prepare captured bytes without semantic lowering or effects.
/// The owned document retains the exact source and its normalized context arena.
/// Implementations bound physical parsing/normalization before allocating storage.
pub trait ProjectDecodePort {
    type Error;
    type Document;
    fn decode(
        &mut self,
        source: Source,
        target: DecodeTarget,
    ) -> Result<Self::Document, Self::Error>;
    fn document(document: &Self::Document) -> &Document;
    fn source(document: &Self::Document) -> &Source;
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureFailure<E, D, X> {
    #[error("project preparation port: {0:?}")]
    Port(E),
    #[error("project preparation interrupted: {0:?}")]
    Interrupted(E),
    #[error("project preparation protocol")]
    Protocol,
    #[error("project preparation limit: {0:?}")]
    Limit(LimitExceeded),
    #[error("project preparation structural: {0:?}")]
    Structural(X),
    #[error("project preparation scalar: document={document:?}, findings={findings:?}")]
    Scalar {
        document: Box<D>,
        findings: Vec<project_function_document::Finding>,
    },
}
#[derive(Debug, thiserror::Error)]
#[error("project preparation: origin={origin:?}, written={written:?}, error={error:?}")]
pub struct SourceFailure<E, D, X> {
    pub origin: Origin,
    pub written: String,
    pub error: CaptureFailure<E, D, X>,
}
#[derive(Debug)]
pub struct DocumentCapture<D> {
    pub origin: Origin,
    pub document: D,
}
#[derive(Debug)]
pub struct LockCapture {
    pub source: Source,
    pub kind: LockKind,
}
/// Successful typed-source indices retain their authored declaration coordinates.
#[derive(Debug, Default)]
pub struct SourceOrigins {
    pub functions: Vec<String>,
    pub codelists: Vec<usize>,
}
#[derive(Debug)]
pub struct PreparedEnvironment<D> {
    pub origins: SourceOrigins,
    /// The caller-supplied project file.
    pub root: D,
    /// The environment file the project links for the host language.
    pub environment_file: Option<D>,
    pub captures: Vec<DocumentCapture<D>>,
    pub lock: Option<LockCapture>,
    pub environment: Environment,
}
#[derive(Debug)]
pub struct RejectedEnvironment<E, D, X> {
    pub host: Language,
    pub draft: Box<Draft>,
    pub origins: SourceOrigins,
    pub root: D,
    pub environment_file: Option<D>,
    pub captures: Vec<DocumentCapture<D>>,
    pub lock: Option<LockCapture>,
    pub sources: Vec<SourceFailure<E, D, X>>,
    pub admission: Option<Vec<Finding>>,
}
#[derive(Debug, thiserror::Error)]
pub enum Failure<E, D, X> {
    #[error(
        "project preparation interrupted: origin={origin:?}, written={written:?}, error={error:?}"
    )]
    Interrupted {
        origin: Origin,
        written: String,
        error: E,
    },
    #[error("project preparation root: {0:?}")]
    Root(X),
    #[error("project preparation root limit: {0:?}")]
    RootLimit(LimitExceeded),
    #[error("project preparation root scalar: root={root:?}, findings={findings:?}")]
    RootScalar {
        root: Box<D>,
        findings: Vec<project_function_document::Finding>,
    },
    #[error("project preparation rejected: {0:?}")]
    Rejected(Box<RejectedEnvironment<E, D, X>>),
}

/// Preparation moves both admitted metadata and original boundary evidence.
pub type PreparationResult<E, D, X> = Result<PreparedEnvironment<D>, Failure<E, D, X>>;

fn capture<P: ProjectCapturePort, D, X>(
    port: &mut P,
    kind: Kind,
    declaring: &str,
    written: &str,
) -> Result<Reply, CaptureFailure<P::Error, D, X>> {
    let reply = port
        .capture(Request {
            kind,
            declaring_source: declaring,
            written,
        })
        .map_err(|error| {
            if port.is_interrupt(&error) {
                CaptureFailure::Interrupted(error)
            } else {
                CaptureFailure::Port(error)
            }
        })?;
    let source = match &reply {
        Reply::Document(source) => source,
        Reply::Lock { source, .. } => source,
    };
    Limits::spec_bytes(source.bytes.len()).map_err(CaptureFailure::Limit)?;
    Ok(reply)
}

/// One captured YAML dependency with its decoded declarations, or the retained
/// failure of that single source.
type Captured<P, D, T> = Result<
    (<D as ProjectDecodePort>::Document, T),
    CaptureFailure<
        <P as ProjectCapturePort>::Error,
        <D as ProjectDecodePort>::Document,
        <D as ProjectDecodePort>::Error,
    >,
>;

/// Capture, decode and structurally admit one declared YAML dependency.
fn capture_document<P: ProjectCapturePort, D: ProjectDecodePort, T>(
    port: &mut P,
    decoder: &mut D,
    (kind, target): (Kind, DecodeTarget),
    declaring: &str,
    written: &str,
    decode: impl FnOnce(&Document) -> Result<T, Vec<project_function_document::Finding>>,
) -> Captured<P, D, T> {
    let Reply::Document(source) = capture(port, kind, declaring, written)? else {
        return Err(CaptureFailure::Protocol);
    };
    let captured = decoder
        .decode(source, target)
        .map_err(CaptureFailure::Structural)?;
    let document = D::document(&captured);
    match decode(document) {
        Ok(value) => Ok((captured, value)),
        Err(findings) => Err(CaptureFailure::Scalar {
            document: Box::new(captured),
            findings,
        }),
    }
}

pub fn prepare<P: ProjectCapturePort, D: ProjectDecodePort>(
    decoder: &mut D,
    source: Source,
    host: Language,
    port: &mut P,
) -> PreparationResult<P::Error, D::Document, D::Error> {
    Limits::spec_bytes(source.bytes.len()).map_err(Failure::RootLimit)?;
    let root = decoder
        .decode(source, DecodeTarget::Project)
        .map_err(Failure::Root)?;
    let document = D::document(&root);
    let declarations = match project_document::decode(document, document.root()) {
        Ok(declarations) => declarations,
        Err(findings) => {
            return Err(Failure::RootScalar {
                root: Box::new(root),
                findings,
            })
        }
    };
    let mut failures = Vec::new();
    let mut origins = SourceOrigins::default();
    let mut captures = Vec::new();
    let mut lock = None;
    // The key states the language; an absent key leaves the host without code.
    let language = declarations.environments.get(host).map(|_| host);
    let mut environment_file = None;
    let mut sources = EnvironmentSources::default();
    if let Some(written) = declarations.environments.get(host) {
        let origin = Origin::Environment(host);
        match capture_document(
            port,
            decoder,
            (Kind::Environment, DecodeTarget::Environment),
            &D::source(&root).identity,
            written,
            |document| project_environment_document::decode(document, document.root()),
        ) {
            Ok((captured, decoded)) => {
                environment_file = Some(captured);
                sources = decoded;
            }
            Err(CaptureFailure::Interrupted(error)) => {
                return Err(Failure::Interrupted {
                    origin,
                    written: written.into(),
                    error,
                })
            }
            Err(error) => failures.push(SourceFailure {
                origin,
                written: written.into(),
                error,
            }),
        }
    }
    // An environment file's own paths resolve from that file.
    let environment_identity = environment_file
        .as_ref()
        .map(|captured| D::source(captured).identity.clone());
    if let (Some(written), Some(declaring)) = (&sources.lock, &environment_identity) {
        let result = (|| match capture(port, Kind::Lock, declaring, written)? {
            Reply::Lock { source, kind } => Ok(LockCapture { source, kind }),
            _ => Err(CaptureFailure::Protocol),
        })();
        match result {
            Ok(captured) => lock = Some(captured),
            Err(CaptureFailure::Interrupted(error)) => {
                return Err(Failure::Interrupted {
                    origin: Origin::Lock,
                    written: written.clone(),
                    error,
                })
            }
            Err(error) => failures.push(SourceFailure {
                origin: Origin::Lock,
                written: written.clone(),
                error,
            }),
        }
    }
    let mut functions = sources.functions.as_ref().map(|_| Vec::new());
    for entry in sources.functions.into_iter().flatten() {
        match entry.declaration {
            Declaration::Inline(def) => {
                origins.functions.push(entry.name);
                functions.as_mut().expect("present functions").push(*def);
            }
            Declaration::Path(written) => {
                let declaring = environment_identity
                    .as_deref()
                    .expect("functions come from a captured environment file");
                match capture_document(
                    port,
                    decoder,
                    (Kind::Function, DecodeTarget::Function),
                    declaring,
                    &written,
                    |document| {
                        project_function_document::decode(document, document.root(), &entry.name)
                    },
                ) {
                    Ok((document, definition)) => {
                        origins.functions.push(entry.name.clone());
                        captures.push(DocumentCapture {
                            origin: Origin::Function(entry.name),
                            document,
                        });
                        functions
                            .as_mut()
                            .expect("present functions")
                            .push(definition);
                    }
                    Err(CaptureFailure::Interrupted(error)) => {
                        return Err(Failure::Interrupted {
                            origin: Origin::Function(entry.name),
                            written,
                            error,
                        })
                    }
                    Err(error) => failures.push(SourceFailure {
                        origin: Origin::Function(entry.name),
                        written,
                        error,
                    }),
                }
            }
        }
    }
    let mut codelists: Vec<TerminologySource> = Vec::new();
    for (index, entry) in declarations.codelists.into_iter().enumerate() {
        match entry.declaration {
            Declaration::Inline(source) => {
                origins.codelists.push(index);
                codelists.push(*source);
            }
            Declaration::Path(written) => {
                match capture_document(
                    port,
                    decoder,
                    (Kind::Codelist, DecodeTarget::Codelist),
                    &D::source(&root).identity,
                    &written,
                    |document| project_terminology_document::decode(document, document.root()),
                ) {
                    Ok((document, source)) => {
                        origins.codelists.push(index);
                        captures.push(DocumentCapture {
                            origin: Origin::Codelist(index),
                            document,
                        });
                        codelists.push(source);
                    }
                    Err(CaptureFailure::Interrupted(error)) => {
                        return Err(Failure::Interrupted {
                            origin: Origin::Codelist(index),
                            written,
                            error,
                        })
                    }
                    Err(error) => failures.push(SourceFailure {
                        origin: Origin::Codelist(index),
                        written,
                        error,
                    }),
                }
            }
        }
    }
    let draft = Draft {
        language,
        lock: lock
            .as_ref()
            .zip(sources.lock)
            .map(|(captured, written)| LockReference {
                written,
                kind: captured.kind,
            }),
        functions,
        codelists,
    };
    let admission = Environment::admit_retained(host, draft);
    match admission {
        Ok(environment) if failures.is_empty() => Ok(PreparedEnvironment {
            origins,
            root,
            environment_file,
            captures,
            lock,
            environment,
        }),
        result => {
            let (draft, admission) = match result {
                Ok(environment) => (Box::new(environment.into_draft()), None),
                Err(rejected) => (rejected.draft, Some(rejected.error)),
            };
            Err(Failure::Rejected(Box::new(RejectedEnvironment {
                host,
                draft,
                origins,
                root,
                environment_file,
                captures,
                lock,
                sources: failures,
                admission,
            })))
        }
    }
}
