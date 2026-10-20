//! The reusable compile-once plan handle.
//!
//! [`Engine::compile`](crate::Engine::compile) prepares a specification once:
//! it captures and admits the metadata, compiles, and reads no study data.
//! [`Plan::run`] then does the rest as often as needed: it activates project
//! functions, reads the inputs, binds, executes and converts the outputs
//! through the same public tables as [`Engine::build`](crate::Engine::build),
//! which is exactly one compile followed by one run.
//!
//! Each run is an independent attempt in its own resource scope. Nothing a
//! run captures, activates or produces is kept in the plan, so runs neither
//! see nor disturb each other, and a failed run leaves the plan unchanged.

use std::sync::Arc;

use crate::cancel::CancellationToken;
use crate::diagnostic::Report;
use crate::engine::Build;
use crate::error::Error;
use crate::internal::{outcome_accepted, Built, Compiled, EngineShared, ObserverBridge};
use crate::observe::Progress;
use crate::resolver::ResourceResolver;

/// An owned, reusable compiled specification. It is cheap to clone, every
/// clone shares the same compiled state, and it lives until its last clone
/// drops, independently of the [`Engine`](crate::Engine) that compiled it.
///
/// A refused compilation still returns a plan: [`Plan::report`] holds its
/// diagnostics, and each run returns that same failed build, with its
/// retained preparation sources, without any resource access.
#[derive(Clone)]
pub struct Plan {
    shared: Arc<EngineShared>,
    compiled: Arc<CompiledPlan>,
}

struct CompiledPlan {
    specification: String,
    state: Compiled,
    report: Report,
}

/// One run's request. Without a resolver, the run reads its inputs and
/// targets Save where compilation did: local files or the engine's resolver.
/// A run resolver serves the inputs at the same declaring identities and
/// receives this run's Save targets. Without an observer, the engine's
/// observer, if any, sees the run's events.
///
/// Progress events carry no run identity, so concurrent runs pass their own
/// observer.
#[non_exhaustive]
#[derive(Clone, Default)]
pub struct RunRequest {
    pub resolver: Option<Arc<dyn ResourceResolver>>,
    pub observer: Option<Arc<dyn Progress>>,
}

impl RunRequest {
    /// Run with the compile authority and the engine's observer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serve this run's inputs and Save targets through a caller resolver.
    pub fn with_resolver(mut self, resolver: Arc<dyn ResourceResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Observe this run's events through a caller progress port.
    pub fn with_observer(mut self, observer: Arc<dyn Progress>) -> Self {
        self.observer = Some(observer);
        self
    }
}

impl Plan {
    pub(crate) fn new(shared: Arc<EngineShared>, specification: String, state: Compiled) -> Self {
        let report = match &state {
            Compiled::Ready(_) => Report {
                diagnostics: Vec::new(),
            },
            Compiled::Refused { issues, .. } => Report::from_issues(issues),
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

    /// Run the compiled specification once. Every adjudicated outcome
    /// returns an owned build carrying its output, run report, observations
    /// and diagnostics; only cancellation fails the call itself.
    ///
    /// # Example
    ///
    /// One plan runs two studies, each served by its own resolver. The
    /// compile resolver holds only the specification. Lanes without the
    /// `yaml` feature compile a refused plan whose runs carry its report.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use arrow_array::RecordBatchIterator;
    /// use arrow_schema::Schema;
    /// use yamaa::{
    ///     CancellationToken, DataType, Engine, EngineOptions, FFI_ArrowArrayStream, Field,
    ///     HostLanguage, InputFrame, Int64Array, MemoryResolver, RecordBatch, Request,
    ///     RunRequest,
    /// };
    ///
    /// let specification = MemoryResolver::new();
    /// specification.insert("/", "spec.yaml", "spec:1", Arc::from(
    ///     &b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n"[..],
    /// ));
    /// let study = |ids: Vec<i64>| {
    ///     let schema = Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, true)]));
    ///     let batch =
    ///         RecordBatch::try_new(Arc::clone(&schema), vec![Arc::new(Int64Array::from(ids))])
    ///             .unwrap();
    ///     let frame = InputFrame::import(FFI_ArrowArrayStream::new(Box::new(
    ///         RecordBatchIterator::new(vec![Ok(batch)], schema),
    ///     )))
    ///     .unwrap();
    ///     let resolver = Arc::new(MemoryResolver::new());
    ///     resolver.insert_frame("/spec.yaml", "input.csv", "data:1", &frame);
    ///     resolver
    /// };
    /// let engine = Engine::new(
    ///     EngineOptions::new(HostLanguage::Python, "doctest".into(), "0".into())
    ///         .with_resolver(Arc::new(specification)),
    /// );
    /// let token = CancellationToken::new();
    /// let plan = engine.compile(&Request::new("spec.yaml"), &token).unwrap();
    /// for ids in [vec![1, 2], vec![7]] {
    ///     let rows = ids.len();
    ///     let build = plan
    ///         .run(&RunRequest::new().with_resolver(study(ids)), &token)
    ///         .unwrap();
    ///     if cfg!(feature = "yaml") {
    ///         assert!(plan.report().is_empty());
    ///         assert_eq!(build.output().unwrap().rows(), rows);
    ///     } else {
    ///         assert!(build.output().is_none());
    ///         assert_eq!(build.report(), plan.report());
    ///     }
    /// }
    /// ```
    pub fn run(&self, request: &RunRequest, token: &CancellationToken) -> Result<Build, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let measurements = Arc::new(yamaa_adapters::application_measurements::Measurements::new());
        let progress = request
            .observer
            .as_deref()
            .or(self.shared.observer.as_deref());
        let resolver = request.resolver.clone().map(crate::internal::bridge);
        measurements.plan(if self.report().is_empty() {
            "reused"
        } else {
            "refused"
        });
        let outcome = self.run_observed(resolver, true, &measurements, progress);
        measurements.complete(outcome_accepted(&outcome));
        let subject = self.name().unwrap_or(&self.compiled.specification);
        Ok(Build::from_outcome(outcome, measurements, &self.shared).with_subject(subject))
    }

    /// Borrow the compilation report: a refused compilation's diagnostics,
    /// empty when the specification compiled.
    pub fn report(&self) -> &Report {
        &self.compiled.report
    }

    /// One run; with `verify` it first verifies the compiled metadata.
    /// The specification's dataset name, once compilation read its document.
    pub(crate) fn name(&self) -> Option<&str> {
        self.compiled.state.name()
    }

    pub(crate) fn run_observed(
        &self,
        resolver: Option<Arc<dyn yamaa_adapters::host_resources::Resolver>>,
        verify: bool,
        measurements: &yamaa_adapters::application_measurements::Measurements,
        progress: Option<&dyn Progress>,
    ) -> Built {
        let bridge = ObserverBridge::new(measurements, progress);
        crate::internal::run(
            &self.shared,
            &self.compiled.specification,
            &self.compiled.state,
            resolver,
            None,
            verify,
            yamaa_core::application_observer::Scope::new(&bridge),
        )
    }

    /// This plan without the compiling engine's project functions and
    /// observer, as a [`Plans`](crate::Plans) holder keeps it: a kept plan
    /// runs through the calling engine's, so holding the compiling call's
    /// host callbacks would only keep them alive.
    pub(crate) fn detached(&self) -> Plan {
        Plan {
            shared: Arc::new(EngineShared {
                resolver: self.shared.resolver.clone(),
                language: self.shared.language,
                runtime_name: self.shared.runtime_name.clone(),
                runtime_version: self.shared.runtime_version.clone(),
                functions: None,
                observer: None,
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
    /// engine's project functions, observer and runtime identity, so a host
    /// call keeps its own capabilities. The caller has just verified the held
    /// files, or compiled them. A project run offers its `held` artifacts.
    pub(crate) fn run_through(
        &self,
        shared: &EngineShared,
        held: Option<Arc<yamaa_adapters::file_resources::HeldArtifacts>>,
        measurements: &yamaa_adapters::application_measurements::Measurements,
        progress: Option<&dyn Progress>,
    ) -> Built {
        let bridge = ObserverBridge::new(measurements, progress);
        crate::internal::run(
            shared,
            &self.compiled.specification,
            &self.compiled.state,
            None,
            held,
            false,
            yamaa_core::application_observer::Scope::new(&bridge),
        )
    }
}
