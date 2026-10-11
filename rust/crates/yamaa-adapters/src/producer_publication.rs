//! Retained native targets for an actual complete producer build.
use crate::{
    file_producer_build::Provenance,
    file_publication::{AnchoredTarget, Publisher},
    file_resources::{Error as ResourceError, Resources},
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_engine::producer_publication::{
    self as engine, Artifact, PreparedPublication, PublicationPort,
};

#[derive(Debug)]
pub enum AdmissionFailure {
    Incomplete,
    Limit(&'static str),
    Resource { node: usize, error: ResourceError },
}
struct Target {
    node: usize,
    selected: String,
    target: AnchoredTarget,
}
pub struct Publication {
    prepared: PreparedPublication,
    targets: Result<Vec<Target>, AdmissionFailure>,
}
#[derive(Debug)]
pub struct TargetFailure {
    pub node: usize,
    pub error: crate::file_publication::Error,
}
#[derive(Default)]
pub struct Attempt {
    pub entered: engine::PublicationAttempt<TargetFailure>,
    pub unwind: Option<Box<dyn Any + Send>>,
}
impl Publication {
    /// Called only beside FileBuild's actual completed build while its original
    /// Resources owner is alive. Original layer declarations select each target.
    pub(crate) fn from_build(
        prepared: PreparedPublication,
        provenance: &Provenance,
        resources: &Resources,
        limits: engine::Limits,
    ) -> Self {
        let targets = Self::targets(&prepared, provenance, resources, limits);
        Self { prepared, targets }
    }
    fn targets(
        prepared: &PreparedPublication,
        provenance: &Provenance,
        resources: &Resources,
        limits: engine::Limits,
    ) -> Result<Vec<Target>, AdmissionFailure> {
        let count = prepared.artifacts().len();
        if count == 0 || count > limits.nodes || count > provenance.metadata().nodes().len() {
            return Err(AdmissionFailure::Limit("producer_publication_nodes"));
        }
        let mut text = limits.path_bytes;
        let mut work = limits.work;
        // Whole authored/canonical path admission precedes target/descriptor
        // ownership, including the temporary text used by native comparisons.
        for artifact in prepared.artifacts() {
            work = work
                .checked_sub(artifact.node().saturating_add(1))
                .ok_or(AdmissionFailure::Limit("producer_publication_work"))?;
            let view = provenance
                .report_views()
                .nth(artifact.node())
                .ok_or(AdmissionFailure::Incomplete)?;
            let origin = view
                .document()
                .written_output_origin()
                .ok_or(AdmissionFailure::Incomplete)?;
            for path in [origin.declaring_source, origin.written, artifact.path()] {
                text = text
                    .checked_sub(path.len())
                    .ok_or(AdmissionFailure::Limit("producer_publication_paths"))?;
            }
            let selected = selected_bound(origin.declaring_source, origin.written)?;
            // Location and AnchoredTarget each retain a canonical copy. Lexical
            // normalization removes components and cannot exceed this bound.
            text = text
                .checked_sub(
                    selected
                        .checked_mul(2)
                        .ok_or(AdmissionFailure::Limit("producer_publication_paths"))?,
                )
                .ok_or(AdmissionFailure::Limit("producer_publication_paths"))?;
            // Each path can participate in every duplicate comparison. This
            // covers path traversal and Windows ordinal UTF-16 comparison text
            // before either Resources call or any comparison allocates it.
            work = work
                .checked_sub(
                    selected
                        .checked_mul(count.saturating_add(1))
                        .and_then(|n| n.checked_mul(4))
                        .ok_or(AdmissionFailure::Limit("producer_publication_work"))?,
                )
                .ok_or(AdmissionFailure::Limit("producer_publication_work"))?;
        }
        let mut targets = Vec::with_capacity(count);
        for artifact in prepared.artifacts() {
            let view = provenance
                .report_views()
                .nth(artifact.node())
                .ok_or(AdmissionFailure::Incomplete)?;
            let origin = view
                .document()
                .written_output_origin()
                .ok_or(AdmissionFailure::Incomplete)?;
            let failure = |error| AdmissionFailure::Resource {
                node: artifact.node(),
                error,
            };
            let selected = resources
                .location_from(origin.declaring_source, origin.written)
                .map_err(failure)?;
            if selected.len() > selected_bound(origin.declaring_source, origin.written)? {
                return Err(AdmissionFailure::Limit("producer_publication_paths"));
            }
            let target = resources
                .publication_target_from(origin.declaring_source, origin.written)
                .map_err(failure)?;
            // Bound the full duplicate-target scan before retaining this target.
            work = work
                .checked_sub(targets.len())
                .ok_or(AdmissionFailure::Limit("producer_publication_work"))?;
            if targets
                .iter()
                .any(|target: &Target| same_location(&target.selected, &selected))
            {
                return Err(AdmissionFailure::Resource {
                    node: artifact.node(),
                    error: ResourceError::InvalidPath,
                });
            }
            targets.push(Target {
                node: artifact.node(),
                selected,
                target,
            });
        }
        Ok(targets)
    }
    pub fn artifacts(&self) -> &[Artifact] {
        self.prepared.artifacts()
    }
    pub fn admission_failure(&self) -> Option<&AdmissionFailure> {
        self.targets.as_ref().err()
    }
    pub fn save_into(&self, limits: engine::Limits, attempt: &mut Attempt) {
        *attempt = Default::default();
        let Ok(targets) = &self.targets else {
            return;
        };
        let mut port = NativePort {
            targets,
            publishers: Vec::new(),
            next: 0,
        };
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            self.prepared
                .save_into(&mut port, limits, &mut attempt.entered)
        })) {
            attempt.unwind = Some(payload);
        }
    }
}
fn selected_bound(declaring: &str, written: &str) -> Result<usize, AdmissionFailure> {
    declaring
        .len()
        .checked_add(written.len())
        .and_then(|n| n.checked_add(2))
        .ok_or(AdmissionFailure::Limit("producer_publication_paths"))
}
fn same_location(left: &str, right: &str) -> bool {
    #[cfg(windows)]
    {
        // Child opens are ordinal case-insensitive, including absent targets.
        crate::windows_file::equal_name(left, right)
    }
    #[cfg(unix)]
    {
        left == right
    }
}
struct NativePort<'a> {
    targets: &'a [Target],
    publishers: Vec<Publisher>,
    next: usize,
}
impl PublicationPort for NativePort<'_> {
    type Error = TargetFailure;
    fn preflight(&mut self, artifacts: &[Artifact]) -> Result<(), Self::Error> {
        if artifacts.len() != self.targets.len() {
            return Err(TargetFailure {
                node: 0,
                error: crate::file_publication::Error::InvalidTarget,
            });
        }
        self.publishers.reserve(artifacts.len());
        for (artifact, target) in artifacts.iter().zip(self.targets) {
            if artifact.node() != target.node {
                return Err(TargetFailure {
                    node: artifact.node(),
                    error: crate::file_publication::Error::PathMismatch,
                });
            }
            self.publishers
                .push(
                    target
                        .target
                        .publisher(artifact.path())
                        .map_err(|error| TargetFailure {
                            node: artifact.node(),
                            error,
                        })?,
                );
        }
        Ok(())
    }
    fn publish(&mut self, artifact: &Artifact) -> Result<(), Self::Error> {
        let publisher = self
            .publishers
            .get_mut(self.next)
            .ok_or_else(|| TargetFailure {
                node: artifact.node(),
                error: crate::file_publication::Error::InvalidTarget,
            })?;
        self.next += 1;
        publisher
            .publish(artifact.path(), artifact.bytes())
            .map_err(|error| TargetFailure {
                node: artifact.node(),
                error,
            })
    }
}
