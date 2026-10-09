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
use yamaa_core::{project_environment::LockKind, project_function::Language};
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
    /// Candidate schemas are explicit retained capabilities. Public entry points
    /// continue using their shipped closure until the format migration qualifies.
    pub fn prepare(
        mut resources: Resources,
        specification: &str,
        environment: &str,
        host: Language,
        specification_schema: Arc<CapturedSchema>,
        environment_schema: Arc<CapturedSchema>,
    ) -> Result<Self, Error> {
        let identity = resources.resolve(environment).map_err(Error::Resource)?;
        let (bytes, _) = resources
            .capture(environment, project_source::Limits::default().bytes)
            .map_err(Error::Resource)?;
        let environment = project_source::prepare(
            environment_schema,
            Source {
                identity,
                bytes: bytes.to_vec(),
            },
            host,
            &mut Metadata {
                resources: &mut resources,
                host,
            },
            Default::default(),
        )
        .map_err(Error::Environment)?
        .into_owned();
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

struct Metadata<'a> {
    resources: &'a mut Resources,
    host: Language,
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
            bytes: bytes.to_vec(),
        };
        Ok(match request.kind {
            // The installed host supplies its lock codec; decoding/version
            // verification still occurs only at the engine's activation gate.
            Kind::Lock => Reply::Lock {
                source,
                kind: match self.host {
                    Language::Python => LockKind::Uv,
                    Language::R => LockKind::Renv,
                },
            },
            Kind::Function | Kind::Codelist => Reply::Document(source),
        })
    }
}
