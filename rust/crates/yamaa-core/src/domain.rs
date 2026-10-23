//! Check/build use cases over an admitted, normalized specification.
//!
//! Source capture, YAML preparation and schema selection precede this boundary.
//! Project activation is the project boundary's authority. A successful check
//! admits declarations; actual source schemas and values are observed at build.
use crate::resolver::Resolver;
use crate::specification::{PrepareError, PreparedSpecification};
use crate::specification_run::{self, CapturedAttempt, SourceDecoder};

/// An immutable capability obtained only by the engine's preflight operation.
/// Each build has fresh observations and executes again, including cached reads.
#[derive(Debug)]
pub struct CheckedSpecification {
    compiled: PreparedSpecification,
}

/// A rejected check has no source attempt; every entered build retains one.
pub type BuildResult<C, D, T> = Result<CapturedAttempt<C, D, T>, PrepareError>;

/// Compile a 1.x specification its front end lowered into `tree`, beside its
/// resolved `document`, which declares its linked inputs, with the producing
/// specifications that `schema` links name, with or without admitted
/// environment metadata. Links supply output contracts only; no producer is
/// compiled, activated or executed. Formula evaluation and binding to actual
/// source schemas remain build phases.
pub fn check_tree_linked(
    tree: crate::ast::Spec,
    document: &crate::schema::Document,
    environment: Option<&crate::project_environment::ExecutionEnvironment>,
    links: crate::producer_contract::Links<'_>,
) -> Result<CheckedSpecification, PrepareError> {
    Ok(CheckedSpecification {
        compiled: PreparedSpecification::prepare_tree_linked(tree, document, environment, links)?,
    })
}

impl CheckedSpecification {
    /// Execute with explicit activated handles and caller-held observations.
    /// Metadata admission precedes every source inspection, capture and decode.
    pub fn build_into<P: Resolver, D: SourceDecoder>(
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
