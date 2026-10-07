//! Gate artifact encoding and publication on accepted execution and observations.
use crate::dataset::{Dataset, Execution};
use alloc::vec::Vec;
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
    let Some(execution) = execution else {
        return report.failure().map_err(CompleteError::Report);
    };
    report.begin(execution).map_err(CompleteError::Report)?;
    let findings = prepared.output_findings();
    if !findings.is_empty() {
        return report.rejected(&findings).map_err(CompleteError::Report);
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
    publisher
        .publish(prepared.output_path(), &bytes)
        .map_err(CompleteError::Publish)?;
    Ok(completed)
}
