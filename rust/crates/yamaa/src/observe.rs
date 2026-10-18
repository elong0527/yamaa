//! Facade-owned progress observation, the public form of the stage observer.
//!
//! Observers have no failure result and must not mutate source or callback
//! state, invoke project code or publish artifacts. The scope guard stays
//! internal; embedders implement [`Progress`] and the facade threads one
//! scope per observed operation.

/// Trusted stage observations, separate from semantic reports.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Preparation,
    Build,
    Activation,
    Input,
    Dataset(DatasetPhase),
    Output,
    Finished,
}

impl Phase {
    pub(crate) fn from_core(phase: yamaa_core::application_observer::Phase) -> Self {
        use yamaa_core::application_observer::Phase as Core;
        match phase {
            Core::Preparation => Self::Preparation,
            Core::Build => Self::Build,
            Core::Activation => Self::Activation,
            Core::Input => Self::Input,
            Core::Dataset(stage) => Self::Dataset(DatasetPhase::from_core(stage)),
            Core::Output => Self::Output,
            Core::Finished => Self::Finished,
        }
    }
}

/// Contiguous stages of one dataset attempt.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatasetPhase {
    Admission,
    Derivation,
    OutputKeys,
    Verification,
    Finished,
}

impl DatasetPhase {
    pub(crate) fn from_core(phase: yamaa_core::dataset::ExecutionPhase) -> Self {
        use yamaa_core::dataset::ExecutionPhase as Core;
        match phase {
            Core::Admission => Self::Admission,
            Core::Derivation => Self::Derivation,
            Core::OutputKeys => Self::OutputKeys,
            Core::Verification => Self::Verification,
            Core::Finished => Self::Finished,
        }
    }
}

/// The stage of one function call.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallStage {
    Activation,
    Dataset(DatasetPhase),
}

impl CallStage {
    pub(crate) fn from_core(stage: yamaa_core::application_observer::CallStage) -> Self {
        use yamaa_core::application_observer::CallStage as Core;
        match stage {
            Core::Activation => Self::Activation,
            Core::Dataset(phase) => Self::Dataset(DatasetPhase::from_core(phase)),
        }
    }
}

/// Whether one exited stage succeeded or failed.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
}

impl Outcome {
    pub(crate) fn from_core(outcome: yamaa_core::application_observer::Outcome) -> Self {
        use yamaa_core::application_observer::Outcome as Core;
        match outcome {
            Core::Succeeded => Self::Succeeded,
            Core::Failed => Self::Failed,
        }
    }
}

/// A classified native limit refusal payload.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub resource: &'static str,
    pub limit: usize,
    pub required: Option<usize>,
}

impl Refusal {
    pub(crate) fn from_core(refusal: yamaa_core::application_observer::Refusal) -> Self {
        use yamaa_core::dataset::Resource as Core;
        let resource = match refusal.resource {
            Core::SourceRows => "source_rows",
            Core::SourceCells => "source_cells",
            Core::SourceColumns => "source_columns",
            Core::SourceCount => "source_count",
            Core::OutputRows => "output_rows",
            Core::OutputCells => "output_cells",
            Core::OutputColumns => "output_columns",
        };
        Self {
            resource,
            limit: refusal.limit,
            required: refusal.required,
        }
    }
}

/// One progress event, the public form of the observer event.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Phase { phase: Phase },
    FunctionCall { stage: CallStage },
    Exit { phase: Phase, outcome: Outcome },
    FunctionReturn { stage: CallStage, outcome: Outcome },
    Refusal { phase: Phase, refusal: Refusal },
}

impl Event {
    pub(crate) fn from_core(event: yamaa_core::application_observer::Event) -> Self {
        use yamaa_core::application_observer::Event as Core;
        match event {
            Core::Phase { phase } => Self::Phase {
                phase: Phase::from_core(phase),
            },
            Core::FunctionCall { stage } => Self::FunctionCall {
                stage: CallStage::from_core(stage),
            },
            Core::Exit { phase, outcome } => Self::Exit {
                phase: Phase::from_core(phase),
                outcome: Outcome::from_core(outcome),
            },
            Core::FunctionReturn { stage, outcome } => Self::FunctionReturn {
                stage: CallStage::from_core(stage),
                outcome: Outcome::from_core(outcome),
            },
            Core::Refusal { phase, refusal } => Self::Refusal {
                phase: Phase::from_core(phase),
                refusal: Refusal::from_core(refusal),
            },
        }
    }
}

/// Embedder progress observation. Observers must not mutate source or
/// callback state, invoke project code or publish artifacts.
pub trait Progress: Send + Sync {
    /// Observe one progress event on the calling thread of the operation.
    fn observe(&self, event: Event);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sink {
        events: std::sync::Mutex<Vec<String>>,
    }

    impl Progress for Sink {
        fn observe(&self, event: Event) {
            let name = match event {
                Event::Phase { .. } => "phase",
                Event::FunctionCall { .. } => "call",
                Event::Exit { .. } => "exit",
                Event::FunctionReturn { .. } => "return",
                Event::Refusal { .. } => "refusal",
            };
            self.events.lock().unwrap().push(name.into());
        }
    }

    #[test]
    fn events_convert_from_core_without_payloads() {
        use yamaa_core::application_observer::{
            Event as CoreEvent, Outcome as CoreOutcome, Phase as CorePhase,
        };
        let sink = Sink {
            events: std::sync::Mutex::new(Vec::new()),
        };
        sink.observe(Event::from_core(CoreEvent::Phase {
            phase: CorePhase::Preparation,
        }));
        sink.observe(Event::from_core(CoreEvent::Exit {
            phase: CorePhase::Finished,
            outcome: CoreOutcome::Succeeded,
        }));
        assert_eq!(*sink.events.lock().unwrap(), vec!["phase", "exit"]);
        assert_eq!(Outcome::from_core(CoreOutcome::Failed), Outcome::Failed);
    }
}
