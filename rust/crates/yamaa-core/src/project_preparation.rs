//! Capture, lower and admit project dependencies through the resolver port.
//! Code activation, study data and publication authority are absent from this service.
//!
//! The caller supplies one project file. Each captured file lowers once into
//! its root ([`crate::ast::project`]); every later read is of those roots.
//! The project file's own header selects the surface it and its dependencies
//! are read in. Paths a project writes resolve from the project file.
//!
//! - A 0.5 project's `environments` entry for the host language links that
//!   language's environment file, which lowers into the project root; every
//!   other entry stays unread. The lock and function paths an environment
//!   file writes resolve from that file.
//! - A 1.0 project lists its function files in `functions` and holds each host
//!   language's lock in `environments.<host>.lock` (D8). Each listed function
//!   file is read, whichever host languages it implements; the host reads only
//!   its own lock (PROJECT-2).
//!
//! An engine that claims no host language, such as the command line's, reads
//! no `environments` entry of either surface: no environment file and no lock
//! (#3030 section 2.10).
use crate::{
    ast::{self, CodelistSource, FunctionSource},
    decoder::Decoder,
    front_end::{v0_5::project as lower, v1::project as lower_v1},
    limits::{LimitExceeded, Limits},
    project_environment::{Draft, Environment, Finding, LockKind, LockReference},
    project_function::Language,
    project_function_document,
    resolver::{File, Name, Resolver},
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
    /// The environment file a 0.5 project `environments` key links.
    Environment(Language),
    Lock,
    /// A function file a 0.5 environment file keys by this logical name.
    Function(String),
    /// A function file a 1.0 project lists, by its index in `functions`.
    FunctionFile(usize),
    Codelist(usize),
}
/// Closed structural targets are selected by the application, never by a host callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeTarget {
    /// The caller's project file, whose own header selects its surface.
    Project,
    Environment,
    /// A 0.5 definition file, without a header.
    Function,
    /// A 0.5 codelist source file, without a header.
    Codelist,
    /// A 1.0 function file, which declares `yamaa` and `type: function`.
    FunctionFile,
    /// A 1.0 codelist file, which declares `yamaa` and `type: codelists`.
    CodelistFile,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureFailure<E, D, X> {
    #[error("project preparation port: {0:?}")]
    Port(E),
    #[error("project preparation interrupted: {0:?}")]
    Interrupted(E),
    /// The held lock is no packaging lock the engine reads.
    #[error("project preparation lock syntax")]
    Lock,
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
    /// The logical name of each admitted definition, in declaration order: a
    /// 0.5 environment file's key, or the name a 1.0 function file declares.
    pub functions: Vec<String>,
    /// For a 1.0 project, the index in `functions` of the file each admitted
    /// definition was read from, parallel to [`SourceOrigins::functions`].
    pub function_files: Vec<usize>,
    pub codelists: Vec<usize>,
}
#[derive(Debug)]
pub struct PreparedEnvironment<D> {
    /// The project file lowered, with the environment file the host reads.
    pub project: ast::Project,
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
    /// The host language, or none for an engine that claims no host language.
    pub host: Option<Language>,
    /// The project file lowered, with the environment file the host reads
    /// when that file lowered.
    pub project: ast::Project,
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

/// Resolve and capture one file the project names, under the shared size
/// cap. An interrupt aborts immediately; ordinary independent failures
/// collect.
fn capture<P: Resolver, D, X>(
    port: &mut P,
    kind: Kind,
    declaring: &str,
    written: &str,
) -> Result<Source, CaptureFailure<P::Error, D, X>> {
    let name = Name::Written {
        file: File::Project(kind),
        declaring,
        written,
    };
    let failed = |port: &P, error| {
        if port.is_interrupt(&error) {
            CaptureFailure::Interrupted(error)
        } else {
            CaptureFailure::Port(error)
        }
    };
    let identity = port
        .inspect(name)
        .map_err(|error| failed(port, error))?
        .identity;
    let bytes = port
        .capture(name)
        .map_err(|error| failed(port, error))?
        .bytes
        .to_vec();
    Limits::spec_bytes(bytes.len()).map_err(CaptureFailure::Limit)?;
    Ok(Source { identity, bytes })
}

/// One captured YAML dependency with its lowered root, or the retained
/// failure of that single source.
type Captured<P, D, T> = Result<
    (<D as Decoder>::Admitted, T),
    CaptureFailure<<P as Resolver>::Error, <D as Decoder>::Admitted, <D as Decoder>::DocumentError>,
>;

/// Capture, decode and lower one declared YAML dependency.
fn capture_document<P: Resolver, D: Decoder, T>(
    port: &mut P,
    decoder: &mut D,
    (kind, target): (Kind, DecodeTarget),
    declaring: &str,
    written: &str,
    lower: impl FnOnce(&Document) -> Result<T, Vec<project_function_document::Finding>>,
) -> Captured<P, D, T> {
    let source = capture(port, kind, declaring, written)?;
    let captured = decoder
        .admitted(source, target)
        .map_err(CaptureFailure::Structural)?;
    let document = D::admitted_document(&captured);
    match lower(document) {
        Ok(value) => Ok((captured, value)),
        Err(findings) => Err(CaptureFailure::Scalar {
            document: Box::new(captured),
            findings,
        }),
    }
}

/// The value of one preparation step, or the interrupt that aborts it.
type Step<T, P, D> = Result<
    T,
    Failure<<P as Resolver>::Error, <D as Decoder>::Admitted, <D as Decoder>::DocumentError>,
>;

/// The failures, origins and captures one preparation collects.
struct Collected<E, D, X> {
    failures: Vec<SourceFailure<E, D, X>>,
    origins: SourceOrigins,
    captures: Vec<DocumentCapture<D>>,
}

/// Keep one dependency's failure, or abort on an interrupt.
fn collect<E, D, X, T>(
    failures: &mut Vec<SourceFailure<E, D, X>>,
    (origin, written): (Origin, &str),
    result: Result<T, CaptureFailure<E, D, X>>,
) -> Result<Option<T>, Failure<E, D, X>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(CaptureFailure::Interrupted(error)) => Err(Failure::Interrupted {
            origin,
            written: written.into(),
            error,
        }),
        Err(error) => {
            failures.push(SourceFailure {
                origin,
                written: written.into(),
                error,
            });
            Ok(None)
        }
    }
}

/// Prepare the project file `source` for the host language `host`, or for an
/// engine that claims none (`None`), which reads no `environments` entry.
pub fn prepare<P: Resolver, D: Decoder>(
    decoder: &mut D,
    source: Source,
    host: Option<Language>,
    port: &mut P,
) -> PreparationResult<P::Error, D::Admitted, D::DocumentError> {
    Limits::spec_bytes(source.bytes.len()).map_err(Failure::RootLimit)?;
    let root = decoder
        .admitted(source, DecodeTarget::Project)
        .map_err(Failure::Root)?;
    // The project file's own header selects its surface (SCHEMA-2).
    let versioned = crate::language::declares_header(D::admitted_document(&root));
    let lowered = if versioned {
        lower_v1::project(D::admitted_document(&root), None)
    } else {
        lower::project(D::admitted_document(&root), None)
    };
    let mut project = match lowered {
        Ok(project) => project,
        Err(findings) => {
            return Err(Failure::RootScalar {
                root: Box::new(root),
                findings,
            })
        }
    };
    let mut collected = Collected {
        failures: Vec::new(),
        origins: SourceOrigins::default(),
        captures: Vec::new(),
    };
    let entry = D::admitted_source(&root).identity.clone();
    let (language, environment_file, declaring) = if versioned {
        // One host language runs the whole build, and a 1.0 project's paths
        // all resolve from the project file (D8).
        (host, None, entry.clone())
    } else {
        // The key states the language; an absent key leaves the host without code.
        let language = host.filter(|&host| project.environment(host).is_some());
        let environment_file = match host {
            Some(host) => {
                capture_environment(port, decoder, &mut project, host, &entry, &mut collected)?
            }
            None => None,
        };
        // An environment file's own paths resolve from that file.
        let declaring = environment_file
            .as_ref()
            .map_or(&entry, |captured| &D::admitted_source(captured).identity)
            .clone();
        (language, environment_file, declaring)
    };
    // The host reads only its own lock (PROJECT-2), and an engine that claims
    // no host language reads none.
    let written_lock = host
        .and_then(|host| project.environment(host))
        .and_then(|environment| environment.lock.as_ref())
        .map(|lock| lock.text.clone());
    let mut lock = None;
    if let Some(written) = &written_lock {
        let result = (|| {
            let source = capture(port, Kind::Lock, &declaring, written)?;
            let kind = decoder.lock(&source).ok_or(CaptureFailure::Lock)?;
            Ok(LockCapture { source, kind })
        })();
        lock = collect(&mut collected.failures, (Origin::Lock, written), result)?;
    }
    // A 0.5 project's functions come from the environment file the host's key
    // links, so a preparation without a host language has none.
    let functions = match (versioned, host) {
        (true, _) => capture_function_files(port, decoder, &project, &declaring, &mut collected)?,
        (false, Some(host)) => {
            capture_functions(port, decoder, &project, host, &declaring, &mut collected)?
        }
        (false, None) => None,
    };
    let codelists = capture_codelists(port, decoder, &project, &entry, versioned, &mut collected)?;
    let draft = Draft {
        surface: project.header.surface,
        language,
        lock: lock
            .as_ref()
            .zip(written_lock)
            .map(|(captured, written)| LockReference {
                written,
                kind: captured.kind,
            }),
        functions,
        codelists,
    };
    let Collected {
        failures,
        origins,
        captures,
    } = collected;
    let admission = Environment::admit_retained(host, draft);
    match admission {
        Ok(environment) if failures.is_empty() => Ok(PreparedEnvironment {
            project,
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
                project,
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

/// Capture the environment file a 0.5 project links for `host` and lower it
/// into the project root.
fn capture_environment<P: Resolver, D: Decoder>(
    port: &mut P,
    decoder: &mut D,
    project: &mut ast::Project,
    host: Language,
    entry: &str,
    collected: &mut Collected<P::Error, D::Admitted, D::DocumentError>,
) -> Step<Option<D::Admitted>, P, D> {
    let linked = project
        .environment(host)
        .and_then(|environment| environment.file.as_ref())
        .map(|file| file.text.clone());
    let Some(written) = linked else {
        return Ok(None);
    };
    let result = capture_document(
        port,
        decoder,
        (Kind::Environment, DecodeTarget::Environment),
        entry,
        &written,
        |document| lower::environment(document, None, host, project),
    );
    Ok(collect(
        &mut collected.failures,
        (Origin::Environment(host), &written),
        result,
    )?
    .map(|(captured, ())| captured))
}

/// The definitions a 0.5 environment file declares, inline or as definition
/// files relative to `declaring`.
fn capture_functions<P: Resolver, D: Decoder>(
    port: &mut P,
    decoder: &mut D,
    project: &ast::Project,
    host: Language,
    declaring: &str,
    collected: &mut Collected<P::Error, D::Admitted, D::DocumentError>,
) -> Step<Option<Vec<ast::Function>>, P, D> {
    let mut functions = project.functions.as_ref().map(|_| Vec::new());
    for declaration in project.functions.iter().flatten() {
        let name = declaration
            .key
            .as_ref()
            .map(|key| key.name.clone())
            .unwrap_or_default();
        match &declaration.source {
            FunctionSource::Inline(definition) => {
                collected.origins.functions.push(name);
                functions
                    .as_mut()
                    .expect("present functions")
                    .push(definition.as_ref().clone());
            }
            FunctionSource::File(written) => {
                let result = capture_document(
                    port,
                    decoder,
                    (Kind::Function, DecodeTarget::Function),
                    declaring,
                    &written.text,
                    |document| lower::function(document, None, &name, host),
                );
                let origin = Origin::Function(name.clone());
                if let Some((document, definition)) = collect(
                    &mut collected.failures,
                    (origin.clone(), &written.text),
                    result,
                )? {
                    collected.origins.functions.push(name);
                    collected
                        .captures
                        .push(DocumentCapture { origin, document });
                    functions
                        .as_mut()
                        .expect("present functions")
                        .push(definition);
                }
            }
        }
    }
    Ok(functions)
}

/// The function files a 1.0 project lists, each relative to the project file
/// (D8). Every listed file is read, whichever host languages it implements.
fn capture_function_files<P: Resolver, D: Decoder>(
    port: &mut P,
    decoder: &mut D,
    project: &ast::Project,
    declaring: &str,
    collected: &mut Collected<P::Error, D::Admitted, D::DocumentError>,
) -> Step<Option<Vec<ast::Function>>, P, D> {
    let mut functions = project.functions.as_ref().map(|_| Vec::new());
    for (index, declaration) in project.functions.iter().flatten().enumerate() {
        let FunctionSource::File(written) = &declaration.source else {
            unreachable!("a 1.0 project lists function files only")
        };
        let result = capture_document(
            port,
            decoder,
            (Kind::Function, DecodeTarget::FunctionFile),
            declaring,
            &written.text,
            |document| lower_v1::function(document, None),
        );
        let origin = Origin::FunctionFile(index);
        if let Some((document, definition)) = collect(
            &mut collected.failures,
            (origin.clone(), &written.text),
            result,
        )? {
            collected
                .origins
                .functions
                .push(definition.name.name.clone());
            collected.origins.function_files.push(index);
            collected
                .captures
                .push(DocumentCapture { origin, document });
            functions
                .as_mut()
                .expect("present functions")
                .push(definition);
        }
    }
    Ok(functions)
}

/// The codelist sources a project declares, inline or as codelist files
/// relative to the project file: header-less in 0.5, and declaring
/// `type: codelists` in 1.0 (P-TYPE-1).
fn capture_codelists<P: Resolver, D: Decoder>(
    port: &mut P,
    decoder: &mut D,
    project: &ast::Project,
    entry: &str,
    versioned: bool,
    collected: &mut Collected<P::Error, D::Admitted, D::DocumentError>,
) -> Step<Vec<ast::Codelists>, P, D> {
    let mut codelists = Vec::new();
    for (index, declaration) in project.codelists.iter().enumerate() {
        match &declaration.source {
            CodelistSource::Inline(source) => {
                collected.origins.codelists.push(index);
                codelists.push(source.as_ref().clone());
            }
            CodelistSource::File(written) => {
                let result = if versioned {
                    capture_document(
                        port,
                        decoder,
                        (Kind::Codelist, DecodeTarget::CodelistFile),
                        entry,
                        &written.text,
                        |document| lower_v1::codelists(document, None),
                    )
                } else {
                    capture_document(
                        port,
                        decoder,
                        (Kind::Codelist, DecodeTarget::Codelist),
                        entry,
                        &written.text,
                        |document| lower::codelists(document, None),
                    )
                };
                let origin = Origin::Codelist(index);
                if let Some((document, source)) = collect(
                    &mut collected.failures,
                    (origin.clone(), &written.text),
                    result,
                )? {
                    collected.origins.codelists.push(index);
                    collected
                        .captures
                        .push(DocumentCapture { origin, document });
                    codelists.push(source);
                }
            }
        }
    }
    Ok(codelists)
}
