//! The authored declaration of a held metadata file that no longer matches its
//! snapshot. A refusal names the field that declared the file and the path as
//! written, with the file's own condition (FILE-10, FILE-11). Nothing here reads
//! a file: every fact comes from the documents preparation already holds.
use crate::{
    project_source::{DocumentCapture, LockCapture, Origin},
    specification_source::{CapturedDocument, LinkedSpecification, PreparedDocument},
};
use yamaa_core::{
    diagnostic::Diagnostic, project_function::Language, resource::ResourceFailure,
    schema::DocumentNode as N,
};

/// The project file and the environment documents it linked, as captured.
pub(crate) struct Environment<'a> {
    /// The project file path as preparation was given it.
    pub(crate) written: &'a str,
    pub(crate) host: Language,
    pub(crate) root: &'a CapturedDocument,
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
    root: &CapturedDocument,
    documents: &[PreparedDocument],
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    let project = &root.normalized().document;
    let locations = yamaa_core::submission_document::locate(project, project.root()).ok()?;
    let mut listed = Vec::new();
    for &(section, node) in locations.sections() {
        let N::Sequence(specs) = &project.nodes()[project.field(node, "specs")?] else {
            return None;
        };
        for (index, &spec) in specs.iter().enumerate() {
            let N::Text(written) = &project.nodes()[spec] else {
                return None;
            };
            let key = yamaa_core::project_scope::key(section);
            listed.push((format!("{key}.specs[{index}]"), written.as_str()));
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
    let text = |raw: &yamaa_core::schema::Document, node: usize| match &raw.nodes()[node] {
        N::Text(written) => Some(written.to_owned()),
        _ => None,
    };
    // The project declares its environment link and codelists; the linked
    // environment file declares its lock and functions.
    let project = &environment.root.normalized().document;
    let linked = environment
        .environment_file
        .map(|file| (file, &file.normalized().document));
    if linked.is_some_and(|(file, _)| file.source().identity == identity) {
        let key = environment.host.key();
        let environments = project.field(project.root(), "environments")?;
        let written = text(project, project.field(environments, key)?)?;
        return Some(failure.field_diagnostic(&format!("environments.{key}"), &written));
    }
    if environment
        .lock
        .is_some_and(|lock| lock.source.identity == identity)
    {
        let (_, raw) = linked?;
        let written = text(raw, raw.field(raw.root(), "lock")?)?;
        return Some(failure.field_diagnostic("lock", &written));
    }
    let capture = environment
        .captures
        .iter()
        .find(|capture| capture.document.source().identity == identity)?;
    match &capture.origin {
        Origin::Function(name) => {
            let (_, raw) = linked?;
            let functions = raw.field(raw.root(), "functions")?;
            let written = text(raw, raw.field(functions, name)?)?;
            Some(failure.field_diagnostic(&format!("functions.{name}"), &written))
        }
        Origin::Codelist(index) => {
            let codelists = project.field(project.root(), "codelists")?;
            let N::Sequence(codelists) = &project.nodes()[codelists] else {
                return None;
            };
            let written = text(project, *codelists.get(*index)?)?;
            Some(failure.field_diagnostic(&format!("codelists[{index}]"), &written))
        }
        Origin::Lock | Origin::Environment(_) => None,
    }
}
