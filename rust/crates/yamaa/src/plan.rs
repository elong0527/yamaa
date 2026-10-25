//! The compiled request a host's lazy object reuses (#2821), internal to the
//! facade: there is no public compile-once plan handle (#3030 D22, D72).
//!
//! A [`Plans`](crate::Plans) holder keeps one plan per specification and
//! project identity. A run of a kept plan does the rest of a build: it
//! activates project functions, reads the inputs, binds, executes and
//! converts the outputs. Each run is an independent attempt in its own
//! resource scope; nothing a run captures, activates or produces is kept in
//! the plan, so runs neither see nor disturb each other.

use std::sync::Arc;

use crate::cancel::CancellationToken;
use crate::diagnostic::Report;
use crate::engine::Build;
use crate::internal::{Compiled, EngineShared};

/// An owned compiled specification, cheap to clone: every clone shares the
/// same compiled state. A refused compilation is a plan too: [`Plan::report`]
/// holds its diagnostics, and each run returns that same failed build
/// without any resource access.
#[derive(Clone)]
pub(crate) struct Plan {
    shared: Arc<EngineShared>,
    compiled: Arc<CompiledPlan>,
}

struct CompiledPlan {
    specification: String,
    state: Compiled,
    report: Report,
}

impl Plan {
    pub(crate) fn new(shared: Arc<EngineShared>, specification: String, state: Compiled) -> Self {
        let report = match &state {
            Compiled::Ready(..) => Report {
                diagnostics: Vec::new(),
            },
            Compiled::Refused {
                issues, locator, ..
            } => Report::located(issues, locator),
        };
        Self {
            shared,
            compiled: Arc::new(CompiledPlan {
                specification,
                state,
                report,
            }),
        }
    }

    /// Borrow the compilation report: a refused compilation's diagnostics,
    /// empty when the specification compiled.
    pub(crate) fn report(&self) -> &Report {
        &self.compiled.report
    }

    /// The specification's dataset name, once compilation read its document.
    pub(crate) fn name(&self) -> Option<&str> {
        self.compiled.state.name()
    }

    /// This plan without the compiling engine's project functions and
    /// progress callback, as a [`Plans`](crate::Plans) holder keeps it: a kept
    /// plan runs through the calling engine's, so holding the compiling call's
    /// host callbacks would only keep them alive.
    pub(crate) fn detached(&self) -> Plan {
        Plan {
            shared: Arc::new(EngineShared {
                resolver: self.shared.resolver.clone(),
                language: self.shared.language,
                runtime_name: self.shared.runtime_name.clone(),
                runtime_version: self.shared.runtime_version.clone(),
                functions: None,
                partitioning: self.shared.partitioning,
                progress: None,
            }),
            compiled: Arc::clone(&self.compiled),
        }
    }

    /// Whether an engine configured as `shared` may run this plan again: it
    /// compiled, for the same host language and resource authority, and the
    /// run-start check finds every file it was compiled from unchanged.
    pub(crate) fn reusable_by(&self, shared: &EngineShared) -> bool {
        crate::internal::same_authority(&self.shared, shared)
            && crate::internal::unchanged(&self.compiled.state)
    }

    /// One run of this plan through an engine configured as `shared`: that
    /// engine's project functions and runtime identity, so a host call keeps
    /// its own capabilities. The caller has just verified the held files, or
    /// compiled them. A project run offers its `held` artifacts. The run
    /// polls `token`.
    pub(crate) fn run_through(
        &self,
        shared: &EngineShared,
        held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
        observed: &crate::progress::Observed<'_>,
        token: &CancellationToken,
    ) -> Build {
        crate::internal::run(
            shared,
            &self.compiled.specification,
            &self.compiled.state,
            held,
            observed,
            token,
        )
    }

    /// One run of this plan whose output is read as a stream (#3697), as
    /// [`Plan::run_through`] runs it with no held artifacts: the build, and
    /// an accepted output's rows, handed out at most `rows` records at a
    /// time.
    #[cfg(feature = "arrow")]
    pub(crate) fn stream_through(
        &self,
        shared: &EngineShared,
        observed: &crate::progress::Observed<'_>,
        token: &CancellationToken,
        rows: std::num::NonZeroUsize,
    ) -> (Build, Option<crate::internal::PublicBatches>) {
        crate::internal::run_streamed(
            shared,
            &self.compiled.specification,
            &self.compiled.state,
            observed,
            token,
            rows,
        )
    }

    /// The public schema of the output this plan builds, known before it
    /// runs; none for a refused compilation.
    #[cfg(feature = "arrow")]
    pub(crate) fn schema(&self) -> Option<arrow_schema::SchemaRef> {
        match &self.compiled.state {
            Compiled::Ready(prepared, _) => {
                yamaa_adapters::public_table::output_schema(prepared.run().compiled())
            }
            Compiled::Refused { .. } => None,
        }
    }

    /// Whether a run of this plan can call its host: an engine with project
    /// functions runs a specification with a project, whose activation and
    /// derivations call them.
    #[cfg(feature = "arrow")]
    pub(crate) fn calls_host(&self, shared: &EngineShared) -> bool {
        shared.functions.is_some()
            && matches!(&self.compiled.state, Compiled::Ready(prepared, _) if prepared.run().has_project())
    }
}
