//! Retained native targets for an actual complete producer build.
use crate::{
    file_producer_build::Provenance,
    resource_publication::{AnchoredTarget, Publisher},
    file_resources::{Error as ResourceError, Resources},
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::producer_publication::{
    self as engine, Artifact, PreparedPublication, PublicationPort,
};

#[derive(Debug)]
pub enum AdmissionFailure {
    Incomplete,
    Rejected(&'static str),
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
    ) -> Self {
        let targets = Self::targets(&prepared, provenance, resources);
        Self { prepared, targets }
    }
    fn targets(
        prepared: &PreparedPublication,
        provenance: &Provenance,
        resources: &Resources,
    ) -> Result<Vec<Target>, AdmissionFailure> {
        let count = prepared.artifacts().len();
        if count == 0 || count > provenance.metadata().nodes().len() {
            return Err(AdmissionFailure::Incomplete);
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
                .location_from(origin.base_source, origin.written)
                .map_err(failure)?;
            let target = resources
                .publication_target_from(origin.base_source, origin.written)
                .map_err(failure)?;
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
    pub fn save_into(&self, attempt: &mut Attempt) {
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
            self.prepared.save_into(&mut port, &mut attempt.entered)
        })) {
            attempt.unwind = Some(payload);
        }
    }
}
pub(crate) fn same_location(left: &str, right: &str) -> bool {
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
            let failure = |error| TargetFailure {
                node: artifact.node(),
                error,
            };
            let publisher = target.target.publisher(artifact.path()).map_err(failure)?;
            // Lexical admission cannot identify distinct selected root names
            // denoting the same held physical parent. Check those aliases only
            // at Save, when every parent must exist, before the first write.
            for previous in &self.publishers {
                if publisher.same_target(previous).map_err(failure)? {
                    return Err(failure(crate::file_publication::Error::InvalidTarget));
                }
            }
            self.publishers.push(publisher);
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
