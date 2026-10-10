//! Owned project preparation using the same approved native roots and snapshots
//! as ordinary file builds. Project metadata capture grants no runtime authority.
use crate::{
    file_preparation,
    file_resources::{Error as ResourceError, Resources},
    project_run::{Attempt, PreparedRun, RejectedRun},
    project_source::{self, CapturePort, Kind, Reply, Request},
    specification_source::{CapturedSchema, Source},
};
use std::sync::Arc;
use yamaa_core::project_function::Language;
use yamaa_engine::project_activation::ActivationPort;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceError),
    Environment(project_source::Failure<ResourceError>),
    Document(file_preparation::Error),
    Compile(Box<RejectedRun>),
}

pub struct FileProject {
    run: Arc<PreparedRun>,
    resources: Resources,
}
impl FileProject {
    /// Schemas are retained capabilities; public entry points use the shipped
    /// domain and environment closures.
    pub fn prepare(
        mut resources: Resources,
        specification: &str,
        environment: &str,
        host: Language,
        specification_schema: Arc<CapturedSchema>,
        environment_schema: Arc<CapturedSchema>,
    ) -> Result<Self, Error> {
        let environment =
            capture_environment(&mut resources, environment, host, environment_schema)?;
        let document = file_preparation::prepare_document(
            &mut resources,
            specification,
            Some(specification_schema),
        )
        .map_err(Error::Document)?;
        let run = PreparedRun::prepare(document, environment).map_err(Error::Compile)?;
        Ok(Self {
            run: Arc::new(run),
            resources,
        })
    }
    pub fn run(&self) -> &PreparedRun {
        &self.run
    }
    /// A retained attempt keeps original documents and metadata independently
    /// of this file capability, without acquiring its study-resource authority.
    pub fn retained_run(&self) -> Arc<PreparedRun> {
        Arc::clone(&self.run)
    }
    pub fn capture_reads(&self) -> usize {
        self.resources.capture_reads()
    }
    pub(crate) fn publication_target(
        &self,
    ) -> Result<crate::file_publication::AnchoredTarget, ResourceError> {
        self.resources
            .publication_target(self.run.compiled().output_path())
    }
    pub fn build<A: ActivationPort>(
        &mut self,
        activation: &mut A,
    ) -> Attempt<ResourceError, A::Error> {
        self.run.execute_with_ports(activation, &mut self.resources)
    }
    /// Let an installed port borrow the exact retained lock without copying it
    /// or exposing study authority. Port construction performs no activation.
    pub fn build_with<'run, A: ActivationPort>(
        &'run mut self,
        factory: impl FnOnce(&'run PreparedRun) -> A,
    ) -> Attempt<ResourceError, A::Error> {
        let mut activation = factory(&self.run);
        self.run
            .execute_with_ports(&mut activation, &mut self.resources)
    }
}

/// Capture the original environment and declared metadata through the same
/// approved reader. Static workflow preparation reuses this transfer without
/// introducing a second capture or any activation or study-source port.
pub(crate) fn capture_environment(
    resources: &mut Resources,
    written: &str,
    host: Language,
    schema: Arc<CapturedSchema>,
) -> Result<project_source::OwnedEnvironment, Error> {
    let identity = resources.resolve(written).map_err(Error::Resource)?;
    let (bytes, _) = resources
        .capture(written, project_source::Limits::default().bytes)
        .map_err(Error::Resource)?;
    project_source::prepare(
        schema,
        Source {
            identity,
            bytes: resources
                .preparation_copy(&bytes)
                .map_err(Error::Resource)?,
        },
        host,
        &mut Metadata { resources },
        Default::default(),
    )
    .map_err(Error::Environment)
    .map(project_source::PreparedEnvironment::into_owned)
}

struct Metadata<'a> {
    resources: &'a mut Resources,
}
impl CapturePort for Metadata<'_> {
    type Error = ResourceError;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        let identity = self
            .resources
            .resolve_from(request.declaring_source, request.written)?;
        if identity.len() > request.remaining_identity_bytes {
            return Err(ResourceError::Limit);
        }
        let (bytes, _) = self.resources.capture_from(
            request.declaring_source,
            request.written,
            request.remaining_bytes,
        )?;
        let source = Source {
            identity,
            bytes: self.resources.preparation_copy(&bytes)?,
        };
        Ok(match request.kind {
            Kind::Lock => Reply::Lock {
                kind: crate::project_lock::kind(&source.bytes)
                    .map_err(|_| ResourceError::InvalidLock)?,
                source,
            },
            Kind::Function | Kind::Codelist => Reply::Document(source),
        })
    }
}
