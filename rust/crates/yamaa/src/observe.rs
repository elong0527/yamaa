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

/// The observed graph node, borrowed only for the observer call.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node<'a> {
    pub index: usize,
    pub identity: &'a str,
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
pub enum Event<'a> {
    Phase {
        node: Option<Node<'a>>,
        phase: Phase,
    },
    FunctionCall {
        node: Option<Node<'a>>,
        stage: CallStage,
    },
    Exit {
        node: Option<Node<'a>>,
        phase: Phase,
        outcome: Outcome,
    },
    FunctionReturn {
        node: Option<Node<'a>>,
        stage: CallStage,
        outcome: Outcome,
    },
    Refusal {
        node: Option<Node<'a>>,
        phase: Phase,
        refusal: Refusal,
    },
}

impl<'a> Event<'a> {
    pub(crate) fn from_core(event: yamaa_core::application_observer::Event<'a>) -> Self {
        use yamaa_core::application_observer::Event as Core;
        let map_node = |node: Option<yamaa_core::application_observer::Node<'a>>| {
            node.map(|node| Node {
                index: node.index,
                identity: node.identity,
            })
        };
        match event {
            Core::Phase { node, phase } => Self::Phase {
                node: map_node(node),
                phase: Phase::from_core(phase),
            },
            Core::FunctionCall { node, stage } => Self::FunctionCall {
                node: map_node(node),
                stage: CallStage::from_core(stage),
            },
            Core::Exit {
                node,
                phase,
                outcome,
            } => Self::Exit {
                node: map_node(node),
                phase: Phase::from_core(phase),
                outcome: Outcome::from_core(outcome),
            },
            Core::FunctionReturn {
                node,
                stage,
                outcome,
            } => Self::FunctionReturn {
                node: map_node(node),
                stage: CallStage::from_core(stage),
                outcome: Outcome::from_core(outcome),
            },
            Core::Refusal {
                node,
                phase,
                refusal,
            } => Self::Refusal {
                node: map_node(node),
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
    fn observe(&self, event: Event<'_>);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sink {
        events: std::sync::Mutex<Vec<String>>,
    }

    impl Progress for Sink {
        fn observe(&self, event: Event<'_>) {
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
            node: None,
            phase: CorePhase::Preparation,
        }));
        sink.observe(Event::from_core(CoreEvent::Exit {
            node: None,
            phase: CorePhase::Finished,
            outcome: CoreOutcome::Succeeded,
        }));
        assert_eq!(*sink.events.lock().unwrap(), vec!["phase", "exit"]);
        assert_eq!(Outcome::from_core(CoreOutcome::Failed), Outcome::Failed);
    }
}
