//! The authored declaration of a held metadata file that no longer matches its
//! snapshot. A refusal names the field that declared the file and the path as
//! written, with the file's own condition (FILE-10, FILE-11). Nothing here reads
//! a file: every fact comes from what preparation already holds.
use crate::{
    project_source::{DocumentCapture, LockCapture, Origin},
    specification_source::{CapturedDocument, LinkedSpecification, PreparedDocument},
};
use yamaa_core::{
    ast::{self, CodelistSource, FunctionSource},
    diagnostic::Diagnostic,
    project_function::Language,
    resource::ResourceFailure,
};

/// The project file, its lowered root and the environment documents it
/// linked, as captured.
pub(crate) struct Environment<'a> {
    /// The project file path as preparation was given it.
    pub(crate) written: &'a str,
    pub(crate) host: Language,
    pub(crate) root: &'a CapturedDocument,
    pub(crate) project: &'a ast::Project,
    pub(crate) environment_file: Option<&'a CapturedDocument>,
    pub(crate) lock: Option<&'a LockCapture>,
    pub(crate) captures: &'a [DocumentCapture],
}

/// A specification document declared by `field` as `written`: the document
/// itself, then its layers and dictionaries under their own fields.
pub(crate) fn specification(
    field: &str,
    written: &str,
    document: &PreparedDocument,
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    if document.source().identity == identity {
        return Some(failure.field_diagnostic(field, written));
    }
    document.resource_diagnostic_for_identity(identity, failure)
}

/// The producing specifications the consumer's `schema` links name, each
/// declared by `input.<dataset>.schema`.
pub(crate) fn links(
    consumer: &PreparedDocument,
    links: &[LinkedSpecification],
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    links.iter().find_map(|link| {
        let producer = link.document.as_deref()?;
        let written = consumer.written_input_origin(&link.dataset, "schema")?;
        let field = format!("input.{}.schema", link.dataset);
        specification(&field, written.written, producer, identity, failure)
    })
}

/// The specifications a project's sections list, captured for the submission
/// inventory in listing order, each declared by `<section>.specs[<index>]`.
pub(crate) fn inventory(
    project: &ast::Project,
    documents: &[PreparedDocument],
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    let declarations = project.submission.as_ref().ok()?;
    let mut listed = Vec::new();
    for section in &declarations.sections {
        let key = yamaa_core::project_scope::key(section.family);
        for (index, spec) in section.specs.iter().enumerate() {
            listed.push((format!("{key}.specs[{index}]"), spec.text.as_str()));
        }
    }
    listed
        .iter()
        .zip(documents)
        .find_map(|((field, written), document)| {
            specification(field, written, document, identity, failure)
        })
}

/// The environment's own documents: the project file, the environment file it
/// links for the host, and the lock, functions and codelists they declare.
pub(crate) fn environment(
    environment: &Environment<'_>,
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    if environment.root.source().identity == identity {
        return Some(failure.field_diagnostic("environment", environment.written));
    }
    // A 0.5 project declares its environment link and codelists, and the
    // linked environment file its lock and functions; a 1.0 project declares
    // its lock, function files and codelists itself (D8).
    let project = environment.project;
    let host = project.environment(environment.host);
    if environment
        .environment_file
        .is_some_and(|file| file.source().identity == identity)
    {
        let key = environment.host.key();
        let written = &host?.file.as_ref()?.text;
        return Some(failure.field_diagnostic(&format!("environments.{key}"), written));
    }
    if environment
        .lock
        .is_some_and(|lock| lock.source.identity == identity)
    {
        let written = &host?.lock.as_ref()?.text;
        let field = match project.header.surface {
            ast::Surface::V0_5 => "lock".into(),
            ast::Surface::V1 { .. } => format!("environments.{}.lock", environment.host.key()),
        };
        return Some(failure.field_diagnostic(&field, written));
    }
    let capture = environment
        .captures
        .iter()
        .find(|capture| capture.document.source().identity == identity)?;
    match &capture.origin {
        Origin::Function(name) => {
            let declaration = project.functions.as_deref()?.iter().find(|declaration| {
                declaration
                    .key
                    .as_ref()
                    .is_some_and(|key| key.name == *name)
            })?;
            let FunctionSource::File(written) = &declaration.source else {
                return None;
            };
            Some(failure.field_diagnostic(&format!("functions.{name}"), &written.text))
        }
        Origin::FunctionFile(index) => {
            let declaration = project.functions.as_deref()?.get(*index)?;
            let FunctionSource::File(written) = &declaration.source else {
                return None;
            };
            Some(failure.field_diagnostic(&format!("functions[{index}]"), &written.text))
        }
        Origin::Codelist(index) => {
            let CodelistSource::File(written) = &project.codelists.get(*index)?.source else {
                return None;
            };
            Some(failure.field_diagnostic(&format!("codelists[{index}]"), &written.text))
        }
        Origin::Lock | Origin::Environment(_) => None,
    }
}
