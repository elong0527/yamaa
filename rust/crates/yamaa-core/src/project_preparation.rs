//! Capture, lower and admit project dependencies through the resolver port.
//! Code activation, study data and publication authority are absent from this service.
//!
//! The caller supplies one project file, which the decoder admits as a 1.0
//! project (SCHEMA-2). A project file that names a `parent` lowers composed
//! with its parent chain ([`crate::project_layers`], PROJECT-16). Each
//! captured file lowers once into its root ([`crate::ast::project`]); every
//! later read is of those roots. Paths a project writes resolve from the
//! project file, a parent's respelled from it so that each names the
//! location beside the file that writes it (PROJECT-4).
//!
//! A project lists its function files in `functions` and holds each host
//! language's lock in `environments.<host>.lock` (D8). Each listed function
//! file is read, whichever host languages it implements; the host reads only
//! its own lock (PROJECT-2). An engine that claims no host language, such as
//! the command line's, reads no `environments` entry, so no lock (#3030
//! section 2.10).
use crate::{
    ast::{self, CodelistSource},
    decoder::Decoder,
    front_end::v1::project as lower,
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
    Lock,
    Function,
    Codelist,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Lock,
    /// A function file the project lists, by its index in `functions`.
    FunctionFile(usize),
    Codelist(usize),
}
/// Closed structural targets are selected by the application, never by a host callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeTarget {
    /// The caller's project file, which declares `yamaa` and `type: project`.
    Project,
    /// A function file, which declares `yamaa` and `type: function`.
    FunctionFile,
    /// A codelist file, which declares `yamaa` and `type: codelists`.
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
    /// The name each admitted definition's function file declares, in
    /// listing order.
    pub functions: Vec<String>,
    /// The index in `functions` of the file each admitted definition was read
    /// from, parallel to [`SourceOrigins::functions`].
    pub function_files: Vec<usize>,
    pub codelists: Vec<usize>,
}
#[derive(Debug)]
pub struct PreparedEnvironment<D> {
    /// The project file lowered.
    pub project: ast::Project,
    pub origins: SourceOrigins,
    /// The caller-supplied project file.
    pub root: D,
    pub captures: Vec<DocumentCapture<D>>,
    pub lock: Option<LockCapture>,
    pub environment: Environment,
}
#[derive(Debug)]
pub struct RejectedEnvironment<E, D, X> {
    /// The host language, or none for an engine that claims no host language.
    pub host: Option<Language>,
    /// The project file lowered.
    pub project: ast::Project,
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
    /// The project file, admitted as `root`, names a `parent`, and its
    /// layers do not compose (PROJECT-16).
    #[error("project preparation layers: root={root:?}, failure={failure:?}")]
    Layers {
        root: Box<D>,
        failure: Box<crate::project_layers::Failure<E, X>>,
    },
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
    // A project file that names a parent lowers composed with its parent
    // chain (PROJECT-16).
    let composed = match D::admitted_layers(&root) {
        Some((schema, written))
            if crate::inheritance::reads_as_layer(crate::language::FileType::Project, written) =>
        {
            let identity = D::admitted_source(&root).identity.clone();
            let entry = crate::inheritance::Source {
                display_path: identity.clone(),
                identity,
            };
            let written = written.clone();
            match crate::project_layers::compose(
                schema,
                crate::language::LANGUAGE_VERSION,
                entry,
                written,
                port,
                decoder,
            ) {
                Ok(composed) => Some(composed.document),
                Err(failure) => {
                    return Err(Failure::Layers {
                        root: Box::new(root),
                        failure: Box::new(failure),
                    })
                }
            }
        }
        _ => None,
    };
    let document = composed
        .as_ref()
        .unwrap_or_else(|| D::admitted_document(&root));
    let project = match lower::project(document, None) {
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
    // Every path a project writes resolves from the project file (D8).
    let entry = D::admitted_source(&root).identity.clone();
    // The host reads only its own lock (PROJECT-2), and an engine that claims
    // no host language reads none.
    let written_lock = host
        .and_then(|host| project.environment(host))
        .and_then(|environment| environment.lock.as_ref())
        .map(|lock| lock.text.clone());
    let mut lock = None;
    if let Some(written) = &written_lock {
        let result = (|| {
            let source = capture(port, Kind::Lock, &entry, written)?;
            let kind = decoder.lock(&source).ok_or(CaptureFailure::Lock)?;
            Ok(LockCapture { source, kind })
        })();
        lock = collect(&mut collected.failures, (Origin::Lock, written), result)?;
    }
    let functions = capture_function_files(port, decoder, &project, &entry, &mut collected)?;
    let codelists = capture_codelists(port, decoder, &project, &entry, &mut collected)?;
    // One host language runs the whole build (D8).
    let draft = Draft {
        language: host,
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
                captures,
                lock,
                sources: failures,
                admission,
            })))
        }
    }
}

/// The function files a project lists, each relative to the project file
/// (D8). Every listed file is read, whichever host languages it implements.
fn capture_function_files<P: Resolver, D: Decoder>(
    port: &mut P,
    decoder: &mut D,
    project: &ast::Project,
    entry: &str,
    collected: &mut Collected<P::Error, D::Admitted, D::DocumentError>,
) -> Step<Option<Vec<ast::Function>>, P, D> {
    let mut functions = project.functions.as_ref().map(|_| Vec::new());
    for (index, declaration) in project.functions.iter().flatten().enumerate() {
        let written = &declaration.file.text;
        let result = capture_document(
            port,
            decoder,
            (Kind::Function, DecodeTarget::FunctionFile),
            entry,
            written,
            |document| lower::function(document, None),
        );
        let origin = Origin::FunctionFile(index);
        if let Some((document, definition)) =
            collect(&mut collected.failures, (origin.clone(), written), result)?
        {
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
/// relative to the project file, each declaring `type: codelists`
/// (P-TYPE-1).
fn capture_codelists<P: Resolver, D: Decoder>(
    port: &mut P,
    decoder: &mut D,
    project: &ast::Project,
    entry: &str,
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
                let result = capture_document(
                    port,
                    decoder,
                    (Kind::Codelist, DecodeTarget::CodelistFile),
                    entry,
                    &written.text,
                    |document| lower::codelists(document, None),
                );
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
