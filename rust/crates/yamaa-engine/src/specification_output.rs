//! Gate artifact encoding and publication on accepted execution and observations.
use crate::dataset::{Dataset, Execution};
use alloc::{string::String, sync::Arc, vec::Vec};
use yamaa_core::{
    specification::{OutputFinding, PreparedSpecification},
    table::TableAccess,
};

/// Explicit host authority for atomic output replacement. Success means every
/// supplied byte has been published at the authorized path; no retry is implied.
pub trait ArtifactPort {
    type Error;
    fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error>;
}

/// A codec must bound allocation while encoding. The application also checks the
/// returned length before observations or publication can accept the artifact.
pub trait ArtifactEncoder {
    type Error;
    fn encode(
        &mut self,
        dataset: &Dataset,
        projection: &[usize],
        byte_limit: usize,
    ) -> Result<Vec<u8>, Self::Error>;
}

/// Portable observation formatting stays outside the application. Methods that
/// return a complete report must enforce its budget; no method may publish.
pub trait OutputReport {
    type Error;
    type Report;
    fn failure(&mut self) -> Result<Self::Report, Self::Error>;
    fn begin(&mut self, execution: &Execution) -> Result<(), Self::Error>;
    fn rejected(&mut self, findings: &[OutputFinding]) -> Result<Self::Report, Self::Error>;
    fn success(
        &mut self,
        execution: &Execution,
        projection: &[usize],
        bytes: &[u8],
    ) -> Result<Self::Report, Self::Error>;
}

#[derive(Debug)]
pub enum CompleteError<R, C, P> {
    Report(R),
    Encode(C),
    Projection,
    OutputLimit,
    Publish(P),
}

/// A failed build cannot be saved; publisher failures retain their original value.
#[derive(Debug)]
pub enum SaveError<E> {
    FailedBuild,
    Publish(E),
}

/// Prepared bytes and projection belong to this successful build, not its caller.
#[derive(Debug)]
pub struct Artifact {
    path: String,
    bytes: Arc<[u8]>,
    projection: Vec<usize>,
}
impl Artifact {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(crate) fn retained_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }
    pub fn projection(&self) -> &[usize] {
        &self.projection
    }
}

/// A completed build retains either its failure report or an immutable artifact.
/// The prepared report may describe prospective publication; only save proves it.
#[derive(Debug)]
pub struct PreparedOutput<R> {
    report: R,
    artifact: Option<Artifact>,
}
impl<R> PreparedOutput<R> {
    pub fn prepared_report(&self) -> &R {
        &self.report
    }
    pub fn artifact(&self) -> Option<&Artifact> {
        self.artifact.as_ref()
    }
    /// Repeat only the explicit publication request. Never capture, decode,
    /// evaluate or encode again, including after a failed publication attempt.
    pub fn save<P: ArtifactPort>(&self, publisher: &mut P) -> Result<&R, SaveError<P::Error>> {
        let artifact = self.artifact.as_ref().ok_or(SaveError::FailedBuild)?;
        publisher
            .publish(&artifact.path, &artifact.bytes)
            .map_err(SaveError::Publish)?;
        Ok(&self.report)
    }
}

pub type Preparation<R, C> = Result<
    PreparedOutput<<R as OutputReport>::Report>,
    CompleteError<
        <R as OutputReport>::Error,
        <C as ArtifactEncoder>::Error,
        core::convert::Infallible,
    >,
>;

pub type Completion<R, C, P> = Result<
    <R as OutputReport>::Report,
    CompleteError<
        <R as OutputReport>::Error,
        <C as ArtifactEncoder>::Error,
        <P as ArtifactPort>::Error,
    >,
>;

/// Only a successful native execution can reach the encoder. Failed capture,
/// decoding, binding, execution or response preparation supplies no execution.
/// Output declaration checks precede encoding, and the complete bounded report
/// precedes the one publication request. Opaque failures are returned unchanged.
pub fn complete<R: OutputReport, C: ArtifactEncoder, P: ArtifactPort>(
    prepared: &PreparedSpecification,
    execution: Option<&Execution>,
    byte_limit: usize,
    report: &mut R,
    codec: &mut C,
    publisher: &mut P,
) -> Completion<R, C, P> {
    let result =
        prepare(prepared, execution, byte_limit, report, codec).map_err(|error| match error {
            CompleteError::Report(e) => CompleteError::Report(e),
            CompleteError::Encode(e) => CompleteError::Encode(e),
            CompleteError::Projection => CompleteError::Projection,
            CompleteError::OutputLimit => CompleteError::OutputLimit,
            CompleteError::Publish(never) => match never {},
        })?;
    if let Some(artifact) = &result.artifact {
        publisher
            .publish(&artifact.path, &artifact.bytes)
            .map_err(CompleteError::Publish)?;
    }
    Ok(result.report)
}

/// Complete output checks and bounded encoding/reporting without publication.
/// No publisher is accepted, so result construction cannot write an artifact.
pub fn prepare<R: OutputReport, C: ArtifactEncoder>(
    prepared: &PreparedSpecification,
    execution: Option<&Execution>,
    byte_limit: usize,
    report: &mut R,
    codec: &mut C,
) -> Preparation<R, C> {
    let Some(execution) = execution else {
        return Ok(PreparedOutput {
            report: report.failure().map_err(CompleteError::Report)?,
            artifact: None,
        });
    };
    report.begin(execution).map_err(CompleteError::Report)?;
    let findings = prepared.output_findings();
    if !findings.is_empty() {
        return Ok(PreparedOutput {
            report: report.rejected(&findings).map_err(CompleteError::Report)?,
            artifact: None,
        });
    }
    let projection = prepared
        .projection()
        .iter()
        .map(|name| {
            execution
                .dataset
                .schema()
                .columns()
                .iter()
                .position(|column| &column.name == name)
                .ok_or(CompleteError::Projection)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bytes = codec
        .encode(&execution.dataset, &projection, byte_limit)
        .map_err(CompleteError::Encode)?;
    if bytes.len() > byte_limit {
        return Err(CompleteError::OutputLimit);
    }
    let completed = report
        .success(execution, &projection, &bytes)
        .map_err(CompleteError::Report)?;
    Ok(PreparedOutput {
        report: completed,
        artifact: Some(Artifact {
            path: prepared.output_path().into(),
            bytes: bytes.into(),
            projection,
        }),
    })
}
