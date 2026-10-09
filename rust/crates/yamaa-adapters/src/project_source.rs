//! Typed capture of environment dependencies before any code or study authority.
//! Hosts apply their admitted path policy and supply held bytes; no source is hashed.
use crate::specification_source::{CapturedDocument, CapturedSchema, Error, Source};
use std::sync::Arc;
use yamaa_core::{
    project_environment::{Draft, Environment, Finding, LockKind, LockReference},
    project_environment_document::{self, Declaration},
    project_function::Language,
    project_function_document,
    project_limits::{AdmissionError, Limits as SemanticLimits},
    project_terminology::Source as TerminologySource,
    project_terminology_document,
};
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
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub sources: usize,
    pub bytes: usize,
    pub identity_bytes: usize,
    pub nodes: usize,
    pub semantic: SemanticLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sources: 128,
            bytes: 16_777_216,
            identity_bytes: 65_536,
            nodes: 262_144,
            semantic: SemanticLimits::default(),
        }
    }
}
/// Remaining total quotas are supplied before the host opens a dependency.
pub struct Request<'a> {
    pub kind: Kind,
    pub declaring_source: &'a str,
    pub written: &'a str,
    pub remaining_bytes: usize,
    pub remaining_identity_bytes: usize,
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
#[derive(Debug)]
pub enum CaptureFailure<E> {
    Port(E),
    Interrupted(E),
    Protocol,
    Limit(&'static str),
    Structural(Error),
    Scalar {
        document: Box<CapturedDocument>,
        findings: Vec<project_function_document::Finding>,
    },
}
#[derive(Debug)]
pub struct SourceFailure<E> {
    pub origin: Origin,
    pub written: String,
    pub error: CaptureFailure<E>,
}
#[derive(Debug)]
pub struct DocumentCapture {
    pub origin: Origin,
    pub document: CapturedDocument,
}
#[derive(Debug)]
pub struct LockCapture {
    pub source: Source,
    pub kind: LockKind,
}
/// Successful typed-source indices map back to authored declarations even when
/// a different source fails capture and cannot contribute a partial model.
#[derive(Debug, Default)]
pub struct SourceOrigins {
    pub functions: Vec<String>,
    pub codelists: Vec<usize>,
}
#[derive(Debug)]
pub struct PreparedEnvironment {
    pub origins: SourceOrigins,
    pub root: CapturedDocument,
    pub captures: Vec<DocumentCapture>,
    pub lock: Option<LockCapture>,
    pub environment: Environment,
}
/// Preserve captured provenance while moving definitions into immutable shared
/// execution metadata. Construction performs no recapture or activation.
#[derive(Debug)]
pub struct OwnedEnvironment {
    origins: SourceOrigins,
    root: CapturedDocument,
    captures: Vec<DocumentCapture>,
    lock: Option<LockCapture>,
    environment: yamaa_core::project_environment::ExecutionEnvironment,
}
impl PreparedEnvironment {
    pub fn into_owned(self) -> OwnedEnvironment {
        OwnedEnvironment {
            origins: self.origins,
            root: self.root,
            captures: self.captures,
            lock: self.lock,
            environment: self.environment.into_execution(),
        }
    }
}
impl OwnedEnvironment {
    pub fn origins(&self) -> &SourceOrigins {
        &self.origins
    }
    pub fn root(&self) -> &CapturedDocument {
        &self.root
    }
    pub fn captures(&self) -> &[DocumentCapture] {
        &self.captures
    }
    pub fn lock(&self) -> Option<&LockCapture> {
        self.lock.as_ref()
    }
    pub fn environment(&self) -> &yamaa_core::project_environment::ExecutionEnvironment {
        &self.environment
    }
}
#[derive(Debug)]
pub struct RejectedEnvironment<E> {
    pub host: Language,
    pub draft: Box<Draft>,
    pub origins: SourceOrigins,
    pub root: CapturedDocument,
    pub captures: Vec<DocumentCapture>,
    pub lock: Option<LockCapture>,
    pub sources: Vec<SourceFailure<E>>,
    pub admission: Option<AdmissionError<Finding>>,
}
#[derive(Debug)]
pub enum Failure<E> {
    Interrupted {
        origin: Origin,
        written: String,
        error: E,
    },
    Root(Error),
    RootScalar {
        root: Box<CapturedDocument>,
        findings: Vec<project_function_document::Finding>,
    },
    Rejected(Box<RejectedEnvironment<E>>),
}
struct Budget {
    limits: Limits,
    sources: usize,
    bytes: usize,
    identities: usize,
    nodes: usize,
}
fn add(
    current: &mut usize,
    amount: usize,
    maximum: usize,
    code: &'static str,
) -> Result<(), &'static str> {
    *current = current
        .checked_add(amount)
        .filter(|&n| n <= maximum)
        .ok_or(code)?;
    Ok(())
}
impl Budget {
    fn source(&mut self, source: &Source) -> Result<(), &'static str> {
        add(&mut self.sources, 1, self.limits.sources, "sources")?;
        add(
            &mut self.bytes,
            source.bytes.len(),
            self.limits.bytes,
            "bytes",
        )?;
        add(
            &mut self.identities,
            source.identity.len(),
            self.limits.identity_bytes,
            "identity_bytes",
        )
    }
    fn document(&mut self, document: &CapturedDocument) -> Result<(), &'static str> {
        add(
            &mut self.nodes,
            document.normalized().document.nodes().len(),
            self.limits.nodes,
            "nodes",
        )
    }
    fn capture<P: CapturePort>(
        &mut self,
        port: &mut P,
        kind: Kind,
        declaring: &str,
        written: &str,
    ) -> Result<Reply, CaptureFailure<P::Error>> {
        if self.sources >= self.limits.sources {
            return Err(CaptureFailure::Limit("sources"));
        }
        let reply = port
            .capture(Request {
                kind,
                declaring_source: declaring,
                written,
                remaining_bytes: self.limits.bytes - self.bytes,
                remaining_identity_bytes: self.limits.identity_bytes - self.identities,
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
        self.source(source).map_err(CaptureFailure::Limit)?;
        Ok(reply)
    }
}

pub fn prepare<P: CapturePort>(
    schema: Arc<CapturedSchema>,
    source: Source,
    host: Language,
    port: &mut P,
    limits: Limits,
) -> Result<PreparedEnvironment, Failure<P::Error>> {
    let mut budget = Budget {
        limits,
        sources: 0,
        bytes: 0,
        identities: 0,
        nodes: 0,
    };
    budget
        .source(&source)
        .map_err(|code| Failure::Root(Error::Limit(code)))?;
    let root = schema.prepare_structural(source).map_err(Failure::Root)?;
    budget
        .document(&root)
        .map_err(|code| Failure::Root(Error::Limit(code)))?;
    let document = &root.normalized().document;
    let declarations = match project_environment_document::decode(document, document.root()) {
        Ok(declarations) => declarations,
        Err(findings) => {
            return Err(Failure::RootScalar {
                root: Box::new(root),
                findings,
            })
        }
    };
    // Bound declared capture attempts, including failed host replies, before any
    // dependency port. A quota failure is not a truncated semantic result.
    let paths = declarations
        .functions
        .iter()
        .flatten()
        .filter(|entry| matches!(entry.declaration, Declaration::Path(_)))
        .count()
        + declarations
            .codelists
            .iter()
            .filter(|entry| matches!(entry.declaration, Declaration::Path(_)))
            .count();
    if paths
        .checked_add(1 + usize::from(declarations.lock.is_some()))
        .is_none_or(|count| count > limits.sources)
    {
        return Err(Failure::Root(Error::Limit("sources")));
    }
    let mut failures = Vec::new();
    let mut origins = SourceOrigins::default();
    let lock_was_written = declarations.lock.is_some();
    let mut captures = Vec::new();
    let mut lock = None;
    if let Some(written) = &declarations.lock {
        let result = (|| match budget.capture(port, Kind::Lock, &root.source().identity, written)? {
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
                        budget.capture(port, Kind::Function, &root.source().identity, &written)?
                    else {
                        return Err(CaptureFailure::Protocol);
                    };
                    let captured = schema
                        .prepare_named_class(source, "function_definition_class")
                        .map_err(CaptureFailure::Structural)?;
                    budget.document(&captured).map_err(CaptureFailure::Limit)?;
                    let document = &captured.normalized().document;
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
                        budget.capture(port, Kind::Codelist, &root.source().identity, &written)?
                    else {
                        return Err(CaptureFailure::Protocol);
                    };
                    let captured = schema
                        .prepare_named_class(source, "codelist_source_class")
                        .map_err(CaptureFailure::Structural)?;
                    budget.document(&captured).map_err(CaptureFailure::Limit)?;
                    let document = &captured.normalized().document;
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
    let admission = Environment::admit_retained_with_limits(host, draft, limits.semantic);
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
                Some(AdmissionError::Findings(mut findings)) => {
                    // A failed capture does not make an authored reference absent.
                    if lock_was_written {
                        findings.retain(|finding| !matches!(finding, Finding::MissingLock));
                    }
                    if findings.is_empty() {
                        None
                    } else {
                        Some(AdmissionError::Findings(findings))
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
