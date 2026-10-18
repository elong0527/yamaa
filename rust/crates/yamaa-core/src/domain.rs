//! Check/build use cases over an admitted, normalized specification.
//!
//! Source capture, YAML preparation and schema selection precede this boundary.
//! Project activation is the project boundary's authority. A successful check
//! admits declarations; actual source schemas and values are observed at build.
use crate::specification_run::{self, CapturedAttempt, SourceDecoder, SpecificationSourcePort};
use crate::{
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

/// Compile calls against the one admitted environment without activating code.
pub fn check_with_project(
    document: &SpecificationDocument,
    functions: &[crate::project_function::Function],
) -> Result<CheckedSpecification, PrepareError> {
    Ok(CheckedSpecification {
        compiled: PreparedSpecification::prepare_with_project(document, functions)?,
    })
}

/// Compile with the producing specifications that `schema` links name, with or
/// without admitted environment metadata. Links supply output contracts only;
/// no producer is compiled, activated or executed.
pub fn check_linked(
    document: &SpecificationDocument,
    environment: Option<&crate::project_environment::ExecutionEnvironment>,
    links: crate::producer_contract::Links<'_>,
) -> Result<CheckedSpecification, PrepareError> {
    Ok(CheckedSpecification {
        compiled: PreparedSpecification::prepare_linked(document, environment, links)?,
    })
}

/// Compile against complete admitted environment metadata without host or study ports.
pub fn check_with_environment(
    document: &SpecificationDocument,
    environment: &crate::project_environment::ExecutionEnvironment,
) -> Result<CheckedSpecification, PrepareError> {
    Ok(CheckedSpecification {
        compiled: PreparedSpecification::prepare_with_environment(document, environment)?,
    })
}

impl CheckedSpecification {
    /// Execute with explicit activated handles and caller-held observations.
    /// Metadata admission precedes every source inspection, capture and decode.
    pub fn build_into<P: SpecificationSourcePort, D: SourceDecoder>(
        &self,
        port: &mut P,
        decoder: &mut D,
        functions: &mut dyn crate::function_invocation::FunctionPort<
            Error = <D::Table as crate::table::TableAccess>::Error,
        >,
        attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
        scope: crate::application_observer::Scope<'_>,
    ) {
        specification_run::execute_into(&self.compiled, port, decoder, functions, attempt, scope);
    }

    /// Borrow the core-owned plan for provenance/reporting and explicit probes.
    pub fn compiled(&self) -> &PreparedSpecification {
        &self.compiled
    }
    /// Report all known declaration findings without entering study authority.
    /// Build still defers these findings to their ordered execution checkpoints.
    pub fn check_diagnostics(&self) -> std::vec::Vec<crate::diagnostic::Diagnostic> {
        self.compiled.check_declaration_diagnostics()
    }

    /// Inspect verification declarations alone for callers with that narrow scope.
    pub fn verification_declaration_diagnostics(
        &self,
    ) -> std::vec::Vec<crate::diagnostic::Diagnostic> {
        self.compiled.verification_declaration_diagnostics()
    }
}

/// Check and build in one engine operation. Preparation failures return before
/// capture or decoding; later failures retain their original typed attempt.
pub fn build<P: SpecificationSourcePort, D: SourceDecoder>(
    document: &SpecificationDocument,
    port: &mut P,
    decoder: &mut D,
) -> BuildResult<P::Error, D::Error, D::Table> {
    let checked = check(document)?;
    let mut attempt = CapturedAttempt::new(checked.compiled().source());
    checked.build_into(
        port,
        decoder,
        &mut crate::function_invocation::NoFunctions::new(),
        &mut attempt,
        crate::application_observer::Scope::default(),
    );
    Ok(attempt)
}
