//! Owned original-document project runs over the shared activation/build service.
//! Preparation has no runtime or study authority. Every build retains its own
//! observations and original host failures, including an unwound host boundary.
use crate::{
    project_source::{CapturedEnvironment, OwnedEnvironment},
    project_source_decoder::{Decoder, Snapshot},
    specification_source::PreparedDocument,
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::{project_environment::ExecutionEnvironment, specification::PreparedSpecification};
use yamaa_engine::{
    project_activation::{ActivationPort, Failure, Observations},
    project_domain::{self, CheckedOwnedProject, RejectedOwnedProject},
    specification_run::{CapturedAttempt, SourcePort},
};

#[derive(Debug)]
pub struct PreparedRun {
    document: PreparedDocument,
    captured: CapturedEnvironment,
    checked: CheckedOwnedProject,
}

/// Failed static checking still owns the original documents, lock and metadata.
/// A diagnostic renderer must use these held origins instead of recapturing them.
#[derive(Debug)]
pub struct RejectedRun {
    pub document: PreparedDocument,
    pub captured: CapturedEnvironment,
    pub rejection: Box<RejectedOwnedProject>,
}

pub enum BoundaryFailure<E> {
    Activation(Failure<E>),
    Unwind(Box<dyn Any + Send>),
}
impl<E: std::fmt::Debug> std::fmt::Debug for BoundaryFailure<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Activation(error) => formatter.debug_tuple("Activation").field(error).finish(),
            Self::Unwind(_) => formatter.write_str("Unwind(..)"),
        }
    }
}

/// Boundary success alone does not imply dataset success. The engine's dataset
/// attempt independently owns capture, ingestion, execution and check outcomes.
pub struct Attempt<C, E> {
    pub boundary: Result<(), BoundaryFailure<E>>,
    pub activation: Observations,
    pub dataset: CapturedAttempt<C, crate::specification_run::Error, Snapshot<E>>,
}

impl PreparedRun {
    pub fn prepare(
        document: PreparedDocument,
        environment: OwnedEnvironment,
    ) -> Result<Self, Box<RejectedRun>> {
        let (environment, captured) = environment.into_parts();
        match project_domain::check_owned(document.model(), environment) {
            Ok(checked) => Ok(Self {
                document,
                captured,
                checked,
            }),
            Err(rejection) => Err(Box::new(RejectedRun {
                document,
                captured,
                rejection,
            })),
        }
    }
    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }
    pub fn captured_environment(&self) -> &CapturedEnvironment {
        &self.captured
    }
    pub fn environment(&self) -> &ExecutionEnvironment {
        self.checked.environment()
    }
    pub fn compiled(&self) -> &PreparedSpecification {
        self.checked.compiled()
    }
    pub fn check_diagnostics(&self) -> Vec<yamaa_core::diagnostic::Diagnostic> {
        self.checked.check_diagnostics()
    }

    /// Own evidence outside the host boundary before granting activation or study
    /// authority. No prior attempt, binding or successful test is reused here.
    pub fn execute_with_ports<A: ActivationPort, P: SourcePort>(
        &self,
        activation: &mut A,
        source: &mut P,
    ) -> Attempt<P::Error, A::Error> {
        let mut attempt = Attempt {
            boundary: Ok(()),
            activation: Observations::default(),
            dataset: CapturedAttempt::new(self.compiled().source()),
        };
        attempt.boundary = match catch_unwind(AssertUnwindSafe(|| {
            self.checked.build_observed_into(
                activation,
                source,
                &mut Decoder::<A::Error>::default(),
                crate::specification_run::limits(),
                &mut attempt.dataset,
                &mut attempt.activation,
            )
        })) {
            Ok(result) => result.map_err(BoundaryFailure::Activation),
            Err(payload) => Err(BoundaryFailure::Unwind(payload)),
        };
        attempt
    }
}
