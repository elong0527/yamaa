//! Metadata admission for the producer compiler prerequisite. No build or
//! activation ports are accepted here and no executable capability is granted.
use yamaa_core::{
    producer_admission::{self, Candidate, Error, Limits, Prepared},
    project_environment::ExecutionEnvironment,
    schema::SpecificationDocument,
    specification::PrepareError,
};

#[derive(Debug)]
pub struct CheckedProducerMetadata {
    prepared: Prepared,
}
pub fn check(
    consumer_identity: &str,
    consumer: &SpecificationDocument,
    candidates: &[Candidate<'_>],
    environment: Option<&ExecutionEnvironment>,
    limits: Limits,
) -> Result<CheckedProducerMetadata, Error> {
    Ok(CheckedProducerMetadata {
        prepared: producer_admission::prepare(
            consumer_identity,
            consumer,
            candidates,
            environment,
            limits,
        )?,
    })
}
impl CheckedProducerMetadata {
    pub fn metadata(&self) -> &Prepared {
        &self.prepared
    }
    /// Until #1741 qualifies orchestration, callers cannot obtain the ordinary
    /// check/build capability. Refusal precedes every source or runtime effect.
    pub fn execution_capability(
        &self,
    ) -> Result<&crate::domain::CheckedSpecification, PrepareError> {
        Err(self.prepared.execution_refusal())
    }
}
