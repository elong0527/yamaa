//! Bounded check/build use cases over an admitted, normalized specification.
//!
//! Source capture, YAML preparation and schema selection precede this boundary.
//! Environment activation and the public host result contract are not implemented
//! here yet. Unsupported vocabulary is rejected before any study-data authority
//! is invoked. A successful check is not evidence that ingestion or execution
//! will succeed: actual source schemas and values are available only at build.
use crate::specification_run::{self, CapturedAttempt, Limits, SourceDecoder, SourcePort};
use yamaa_core::{
    schema::SpecificationDocument,
    specification::{PrepareError, PreparedSpecification},
};

/// An immutable capability obtained only by the engine's preflight operation.
/// Each build has fresh observations and executes again, including cached reads.
#[derive(Debug)]
pub struct CheckedSpecification {
    compiled: PreparedSpecification,
}

/// A rejected check has no source attempt; every entered build retains one.
pub type BuildResult<C, D, T> = Result<CapturedAttempt<C, D, T>, PrepareError>;

/// Admit the executable vocabulary without study data, callbacks or publication.
/// Formula evaluation and binding to actual source schemas remain build phases.
pub fn check(document: &SpecificationDocument) -> Result<CheckedSpecification, PrepareError> {
    Ok(CheckedSpecification {
        compiled: PreparedSpecification::prepare(document)?,
    })
}

impl CheckedSpecification {
    /// Borrow the core-owned plan for provenance/reporting and explicit probes.
    pub fn compiled(&self) -> &PreparedSpecification {
        &self.compiled
    }

    /// Execute into caller-held observations so a host panic fence can retain
    /// partial evidence. No host is entered during the preceding check operation.
    pub fn build_into<P: SourcePort, D: SourceDecoder>(
        &self,
        port: &mut P,
        decoder: &mut D,
        limits: Limits,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
    ) {
        specification_run::execute_with_port_into(&self.compiled, port, decoder, limits, attempt);
    }
}

/// Check and build in one engine operation. Preparation failures return before
/// capture or decoding; later failures retain their original typed attempt.
pub fn build<P: SourcePort, D: SourceDecoder>(
    document: &SpecificationDocument,
    port: &mut P,
    decoder: &mut D,
    limits: Limits,
) -> BuildResult<P::Error, D::Error, D::Table> {
    let checked = check(document)?;
    let mut attempt = CapturedAttempt::new(checked.compiled().source());
    checked.build_into(port, decoder, limits, &mut attempt);
    Ok(attempt)
}
