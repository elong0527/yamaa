//! Opt-in wall-clock measurements; no clock or timing value enters engine semantics.
use serde::Serialize;
use std::time::{Duration, Instant};
use yamaa_engine::dataset::ExecutionPhase;

/// Non-overlapping native stages; host IO and normalization are measured by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfilePhase {
    RequestAdmission,
    HostBindings,
    SnapshotDecode,
    EngineAdmission,
    Derivation,
    OutputKeys,
    Verification,
    ResponseEncoding,
}

/// A single attempt's monotonic clock, separate from portable execution observations.
pub struct DatasetProfile {
    started: Instant,
    phase_started: Instant,
    phase: ProfilePhase,
    completed: Vec<(ProfilePhase, Duration)>,
}

impl Default for DatasetProfile {
    /// Begin one fresh profile rather than sharing clock state across attempts.
    fn default() -> Self {
        Self::new()
    }
}

impl DatasetProfile {
    /// Start before request admission; ordinary execution never creates this clock.
    pub fn new() -> Self {
        let started = Instant::now();
        Self {
            started,
            phase_started: started,
            phase: ProfilePhase::RequestAdmission,
            completed: Vec::with_capacity(8),
        }
    }

    /// Close the current phase and start the next without evaluating any language value.
    pub fn enter(&mut self, phase: ProfilePhase) {
        let now = Instant::now();
        self.completed
            .push((self.phase, now.duration_since(self.phase_started)));
        self.phase_started = now;
        self.phase = phase;
    }

    /// Map engine boundaries to adapter-owned clocks; finished begins response encoding.
    pub(crate) fn engine_phase(&mut self, phase: ExecutionPhase) {
        self.enter(match phase {
            ExecutionPhase::Admission => ProfilePhase::EngineAdmission,
            ExecutionPhase::Derivation => ProfilePhase::Derivation,
            ExecutionPhase::OutputKeys => ProfilePhase::OutputKeys,
            ExecutionPhase::Verification => ProfilePhase::Verification,
            ExecutionPhase::Finished => ProfilePhase::ResponseEncoding,
        });
    }

    /// Stop before serializing metrics; integer text retains exact elapsed nanoseconds.
    pub fn finish(mut self) -> Result<String, serde_json::Error> {
        let stopped = Instant::now();
        self.completed
            .push((self.phase, stopped.duration_since(self.phase_started)));
        #[derive(Serialize)]
        struct Phase {
            phase: ProfilePhase,
            elapsed_ns: String,
        }
        #[derive(Serialize)]
        struct Profile {
            protocol: &'static str,
            total_ns: String,
            phases: Vec<Phase>,
        }
        serde_json::to_string(&Profile {
            protocol: "dataset-profile/1",
            total_ns: stopped.duration_since(self.started).as_nanos().to_string(),
            phases: self
                .completed
                .into_iter()
                .map(|(phase, elapsed)| Phase {
                    phase,
                    elapsed_ns: elapsed.as_nanos().to_string(),
                })
                .collect(),
        })
    }
}
