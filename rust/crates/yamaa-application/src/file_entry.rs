//! Native capture of one original entry and the producing specifications its
//! `schema` links name. Capture retains metadata only: a linked producer is read
//! and validated as a specification, never compiled or executed.
use crate::{
    file_preparation,
    file_resources::{Error as ResourceError, Resources},
    project_source::{CapturedEnvironment, OwnedEnvironment},
    specification_source::{LinkedSpecification, PreparedDocument},
};

#[derive(Debug)]
pub enum Error {
    /// The entry, the caller's own path given as `path`, could not be
    /// resolved or captured (FILE-7, FILE-8, FILE-11).
    Caller {
        path: String,
        error: ResourceError,
    },
    Snapshot {
        identity: String,
        error: ResourceError,
    },
    Document(file_preparation::Error),
}
/// Identifies the original input declaration whose producing specification was
/// being read when a native failure occurred.
#[derive(Debug)]
pub struct Origin {
    pub dataset: String,
}
/// A refused capture still owns the original raw bytes, the prepared entry when
/// it was reached and the environment capture. No IO is retried.
pub struct RejectedEntry {
    error: Error,
    entry: String,
    origin: Option<Origin>,
    document: Option<PreparedDocument>,
    environment: CapturedEnvironment,
    resources: Resources,
}

impl RejectedEntry {
    pub(crate) fn retained_metadata_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_metadata_sources()
    }
    pub fn error(&self) -> &Error {
        &self.error
    }
    /// Retain the original root request.
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }
    /// The prepared entry document, when preparation reached it.
    pub fn document(&self) -> Option<&PreparedDocument> {
        self.document.as_ref()
    }
    pub fn environment(&self) -> &CapturedEnvironment {
        &self.environment
    }
    /// Raw captures include failed entry/parent decoding, independently of the
    /// successfully prepared documents. These held bytes require no reread.
    pub fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.resources.captured_sources()
    }
    /// Bounded raw rejection evidence. No preparation or IO is retried.
    pub fn retained_sources(&self) -> crate::file_resources::RetainedSources {
        self.resources.retain_sources()
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
}
impl std::fmt::Debug for RejectedEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedEntry")
            .field("error", &self.error)
            .field("entry", &self.entry)
            .field("origin", &self.origin)
            .field("capture_reads", &self.capture_reads())
            .finish_non_exhaustive()
    }
}

/// Constructed only by the approved original native capture boundary. Moving
/// this whole owner never supplies an arbitrary document or environment.
pub(crate) struct CapturedEntry {
    resources: Resources,
    written: String,
    environment: OwnedEnvironment,
    document: PreparedDocument,
    links: Vec<LinkedSpecification>,
}
pub(crate) type EntryParts = (
    Resources,
    String,
    PreparedDocument,
    Vec<LinkedSpecification>,
    OwnedEnvironment,
);
impl CapturedEntry {
    pub(crate) fn into_parts(self) -> EntryParts {
        (
            self.resources,
            self.written,
            self.document,
            self.links,
            self.environment,
        )
    }
}

/// Capture the entry, its inventory metadata and every producing specification
/// its `schema` links name, then verify all held snapshots. No activation,
/// study, runtime or publisher port is accepted or exposed. `written` names
/// the entry to `resources`; `given` is the path the caller passed, which an
/// entry that cannot be read names. With a `parent` the caller names, the
/// entry is a fragment checked on that file as its parent (#3694).
pub(crate) fn capture(
    mut resources: Resources,
    given: &str,
    written: &str,
    mut environment: OwnedEnvironment,
    parent: Option<yamaa_core::inheritance::CallerParent<'_>>,
) -> Result<CapturedEntry, Box<RejectedEntry>> {
    let written = written.to_owned();
    let mut document = None;
    let mut origin: Option<String> = None;
    let result = (|| {
        let prepared = match parent {
            Some(parent) => {
                file_preparation::prepare_fragment_from(&mut resources, &written, parent)
            }
            None => file_preparation::prepare_document_from(&mut resources, None, &written),
        };
        document = Some(prepared.map_err(|error| match error {
            file_preparation::Error::Entry(error) => Error::Caller {
                path: given.to_owned(),
                error,
            },
            error => Error::Document(error),
        })?);
        let root = document.as_ref().expect("captured entry");
        crate::file_inventory::prepare(
            &mut resources,
            &mut environment,
            yamaa_core::submission_inventory::requested(root.document()),
            &root.source().identity,
        )
        .map_err(Error::Document)?;
        let links = file_preparation::capture_links(&mut resources, root, &mut origin)
            .map_err(Error::Document)?;
        // Complete admission and the whole input inspection precede
        // selection: every metadata snapshot is verified unchanged, and one
        // that changed or disappeared fails at the field that declared it.
        resources.verify_held().map_err(|(identity, error)| {
            changed(root, &written, &links, &environment, identity, error)
        })?;
        Ok(links)
    })();
    match result {
        Ok(links) => Ok(CapturedEntry {
            resources,
            written,
            environment,
            document: document.expect("captured entry"),
            links,
        }),
        Err(error) => {
            let (_, environment) = environment.into_parts();
            Err(Box::new(RejectedEntry {
                error,
                entry: written,
                origin: origin.map(|dataset| Origin { dataset }),
                document,
                environment,
                resources,
            }))
        }
    }
}

/// A held snapshot that no longer matches: a file that changed or disappeared
/// fails with its own condition at the field that declared it, the path as
/// written, when `entry` and what it holds declare it (FILE-10, FILE-11); the
/// snapshot is named by its location, as the documents carry it, so this holds
/// on every resolver. Any other failure stays the snapshot's own refusal.
fn changed(
    document: &PreparedDocument,
    entry: &str,
    links: &[LinkedSpecification],
    environment: &OwnedEnvironment,
    identity: String,
    error: ResourceError,
) -> Error {
    use crate::changed_metadata as changed;
    use yamaa_core::resource::ResourceFailure as R;
    let held = changed::Held {
        entry,
        document,
        links,
        project: environment.project(),
        inventory: environment.inventory_documents(),
        environment: environment
            .root_optional()
            .zip(environment.project())
            .zip(environment.written())
            .map(|((root, project), written)| changed::Environment {
                written,
                host: environment.environment().host(),
                root,
                parents: environment.parents(),
                project,
                lock: environment.lock(),
                captures: environment.captures(),
            }),
    };
    let declared = crate::file_resources::resource_failure(&error)
        .filter(|failure| matches!(failure, R::ContentChanged | R::Missing))
        .and_then(|failure| changed::declaration(&held, &identity, failure));
    match declared {
        Some(diagnostic) => Error::Document(file_preparation::Error::ResourceFinding {
            error,
            diagnostic: Box::new(diagnostic),
        }),
        None => Error::Snapshot { identity, error },
    }
}

#[cfg(all(test, feature = "yaml"))]
mod inventory_rejection_tests {
    use super::*;

    #[test]
    fn entry_capture_keeps_inventory_failure_in_the_preparation_stage() {
        use yamaa_core::project_function::Language;
        let error = || {
            file_preparation::Error::Inventory(
                yamaa_core::submission_inventory::MetadataError::Shape,
            )
        };
        let (_, environment) =
            OwnedEnvironment::without_environment(Some(Language::Python)).into_parts();
        let base = crate::file_resolver::caller_path(&std::env::current_dir().unwrap()).unwrap();
        let rejected = crate::file_workflow::SelectionError::Preparation(
            crate::file_workflow::Error::Entry(Box::new(RejectedEntry {
                error: Error::Document(error()),
                entry: "spec.yaml".into(),
                origin: None,
                document: None,
                environment,
                resources: Resources::for_entry(&base).unwrap(),
            })),
        );
        assert_eq!(
            crate::project_application::selection_failure(&rejected),
            crate::file_application::rejected(
                yamaa_core::application_issue::Step::Prepare,
                yamaa_core::application_issue::Fault::Inventory,
            )
        );
    }
}
