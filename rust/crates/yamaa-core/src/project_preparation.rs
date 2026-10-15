//! Capture, structurally decode and admit project dependencies through native ports.
//! Code activation, study data and publication authority are absent from this service.
use crate::{
    limits::{LimitExceeded, Limits},
    project_environment::{Draft, Environment, Finding, LockKind, LockReference},
    project_environment_document::{self, Declaration},
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
    Lock,
    Function,
    Codelist,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
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
pub trait CapturePort {
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
    Environment,
    Function,
    Codelist,
}

/// Decode and structurally prepare captured bytes without semantic lowering or effects.
/// The owned document retains the exact source and its normalized context arena.
/// Implementations bound physical parsing/normalization before allocating storage.
pub trait DecodePort {
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
    pub root: D,
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

fn capture<P: CapturePort, D, X>(
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

pub fn prepare<P: CapturePort, D: DecodePort>(
    decoder: &mut D,
    source: Source,
    host: Language,
    port: &mut P,
) -> PreparationResult<P::Error, D::Document, D::Error> {
    Limits::spec_bytes(source.bytes.len()).map_err(Failure::RootLimit)?;
    let root = decoder
        .decode(source, DecodeTarget::Environment)
        .map_err(Failure::Root)?;
    let document = D::document(&root);
    let declarations = match project_environment_document::decode(document, document.root()) {
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
    let lock_was_written = declarations.lock.is_some();
    let mut captures = Vec::new();
    let mut lock = None;
    if let Some(written) = &declarations.lock {
        let result = (|| match capture(port, Kind::Lock, &D::source(&root).identity, written)? {
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
    let mut functions = declarations.functions.as_ref().map(|_| Vec::new());
    for entry in declarations.functions.into_iter().flatten() {
        match entry.declaration {
            Declaration::Inline(def) => {
                origins.functions.push(entry.name);
                functions.as_mut().expect("present functions").push(*def);
            }
            Declaration::Path(written) => {
                let result = (|| {
                    let Reply::Document(source) =
                        capture(port, Kind::Function, &D::source(&root).identity, &written)?
                    else {
                        return Err(CaptureFailure::Protocol);
                    };
                    let captured = decoder
                        .decode(source, DecodeTarget::Function)
                        .map_err(CaptureFailure::Structural)?;
                    let document = D::document(&captured);
                    let definition = match project_function_document::decode(
                        document,
                        document.root(),
                        &entry.name,
                    ) {
                        Ok(definition) => definition,
                        Err(findings) => {
                            return Err(CaptureFailure::Scalar {
                                document: Box::new(captured),
                                findings,
                            })
                        }
                    };
                    Ok((captured, definition))
                })();
                match result {
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
                let result = (|| {
                    let Reply::Document(source) =
                        capture(port, Kind::Codelist, &D::source(&root).identity, &written)?
                    else {
                        return Err(CaptureFailure::Protocol);
                    };
                    let captured = decoder
                        .decode(source, DecodeTarget::Codelist)
                        .map_err(CaptureFailure::Structural)?;
                    let document = D::document(&captured);
                    let source =
                        match project_terminology_document::decode(document, document.root()) {
                            Ok(source) => source,
                            Err(findings) => {
                                return Err(CaptureFailure::Scalar {
                                    document: Box::new(captured),
                                    findings,
                                })
                            }
                        };
                    Ok((captured, source))
                })();
                match result {
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
        language: declarations.language,
        lock: lock
            .as_ref()
            .zip(declarations.lock)
            .map(|(captured, written)| LockReference {
                written,
                kind: captured.kind,
            }),
        functions,
        codelists,
        has_study: declarations.study.is_some(),
        submissions: declarations
            .submissions
            .into_iter()
            .map(|(kind, _)| kind)
            .collect(),
    };
    let admission = Environment::admit_retained(host, draft);
    match admission {
        Ok(environment) if failures.is_empty() => Ok(PreparedEnvironment {
            origins,
            root,
            captures,
            lock,
            environment,
        }),
        result => {
            let (draft, admission) = match result {
                Ok(environment) => (Box::new(environment.into_draft()), None),
                Err(rejected) => (rejected.draft, Some(rejected.error)),
            };
            let admission = match admission {
                Some(mut findings) => {
                    // A failed capture does not make an authored reference absent.
                    if lock_was_written {
                        findings.retain(|finding| !matches!(finding, Finding::MissingLock));
                    }
                    if findings.is_empty() {
                        None
                    } else {
                        Some(findings)
                    }
                }
                error => error,
            };
            Err(Failure::Rejected(Box::new(RejectedEnvironment {
                host,
                draft,
                origins,
                root,
                captures,
                lock,
                sources: failures,
                admission,
            })))
        }
    }
}
