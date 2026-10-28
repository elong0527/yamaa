//! The authored declaration of a held metadata file that no longer matches its
//! snapshot. A refusal names the field that declared the file and the path as
//! written, with the file's own condition (FILE-10, FILE-11). Nothing here reads
//! a file: every fact comes from what preparation already holds.
use crate::{
    project_source::{DocumentCapture, LockCapture, Origin},
    specification_source::{
        CapturedDocument, CapturedParent, LinkedSpecification, PreparedDocument,
    },
};
use yamaa_core::{
    ast::{self, CodelistSource},
    diagnostic::Diagnostic,
    project_function::Language,
    resource::ResourceFailure,
};

/// The project file, its lowered root and the documents it names, as
/// captured.
pub(crate) struct Environment<'a> {
    /// The project file path as preparation was given it.
    pub(crate) written: &'a str,
    /// The host language, or none for an engine that claims no host language,
    /// which holds no lock.
    pub(crate) host: Option<Language>,
    pub(crate) root: &'a CapturedDocument,
    pub(crate) parents: &'a [CapturedParent],
    pub(crate) project: &'a ast::Project,
    pub(crate) lock: Option<&'a LockCapture>,
    pub(crate) captures: &'a [DocumentCapture],
}

/// What one specification's preparation holds that declares a metadata file:
/// the entry, as written, and its document; the producing specifications its
/// `schema` links name; and, with a project, the specifications the project
/// lists for the submission inventory and the project's own documents.
pub(crate) struct Held<'a> {
    pub(crate) entry: &'a str,
    pub(crate) document: &'a PreparedDocument,
    pub(crate) links: &'a [LinkedSpecification],
    pub(crate) project: Option<&'a ast::Project>,
    pub(crate) inventory: &'a [PreparedDocument],
    pub(crate) environment: Option<Environment<'a>>,
}

/// The declaration of the held file `identity` locates, searched from the
/// entry outward: the entry with its layers and dictionaries, the producers
/// its links name, the specifications the project lists, then the project's
/// own documents.
pub(crate) fn declaration(
    held: &Held<'_>,
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    specification(
        "specification",
        held.entry,
        held.document,
        identity,
        failure,
    )
    .or_else(|| links(held.document, held.links, identity, failure))
    .or_else(|| inventory(held.project?, held.inventory, identity, failure))
    .or_else(|| environment(held.environment.as_ref()?, identity, failure))
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

/// The environment's own documents: the project file and the lock, function
/// files and codelists it declares (D8).
pub(crate) fn environment(
    environment: &Environment<'_>,
    identity: &str,
    failure: ResourceFailure,
) -> Option<Diagnostic> {
    if environment.root.source().identity == identity {
        return Some(failure.field_diagnostic("environment", environment.written));
    }
    let mut declaring = environment.root.source().identity.as_str();
    let mut document = &environment.root.raw().document;
    for parent in environment.parents {
        if parent.source().identity == identity {
            let node = document.field(document.root(), "parent")?;
            let yamaa_core::schema::DocumentNode::Text(written) = &document.nodes()[node] else {
                return None;
            };
            return Some(yamaa_core::inheritance::parent_diagnostic(
                failure, declaring, written,
            ));
        }
        declaring = &parent.source().identity;
        document = &parent.raw().document;
    }
    let project = environment.project;
    if environment
        .lock
        .is_some_and(|lock| lock.source.identity == identity)
    {
        let host = environment.host?;
        let written = &project.environment(host)?.lock.as_ref()?.text;
        let field = format!("environments.{}.lock", host.key());
        return Some(failure.field_diagnostic(&field, written));
    }
    let capture = environment
        .captures
        .iter()
        .find(|capture| capture.document.source().identity == identity)?;
    match &capture.origin {
        Origin::FunctionFile(index) => {
            let written = &project.functions.as_deref()?.get(*index)?.file;
            Some(failure.field_diagnostic(&format!("functions[{index}]"), &written.text))
        }
        Origin::Codelist(index) => {
            let CodelistSource::File(written) = &project.codelists.get(*index)?.source else {
                return None;
            };
            Some(failure.field_diagnostic(&format!("codelists[{index}]"), &written.text))
        }
        Origin::Lock => None,
    }
}
