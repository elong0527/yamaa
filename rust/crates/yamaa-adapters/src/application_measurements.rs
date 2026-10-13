//! Bounded measurements captured during the original application. Reading them
//! later never runs a source, function, serializer or publisher again.
use serde::Serialize;
use std::{sync::Mutex, time::Instant};
use yamaa_engine::{
    application_observer::{CallStage, Event, Observer, Outcome, Phase},
    dataset::ExecutionPhase,
};

const NODES: usize = 64;
const RECORDS: usize = 1024;
const IDENTITY_BYTES: usize = 1_048_576;
const SAVES: usize = 16;

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Preparation => "preparation",
        Phase::Activation => "activation",
        Phase::Input => "input",
        Phase::Build => "build_service",
        Phase::Dataset(ExecutionPhase::Admission) => "execution_admission",
        Phase::Dataset(ExecutionPhase::Derivation) => "execution_derivation",
        Phase::Dataset(ExecutionPhase::OutputKeys) => "output_keys",
        Phase::Dataset(ExecutionPhase::Verification) => "verification",
        Phase::Dataset(ExecutionPhase::Finished) => "execution_finished",
        Phase::Output => "output_preparation",
        Phase::Finished => "finished",
    }
}
fn stage_index(stage: CallStage) -> usize {
    match stage {
        CallStage::Activation => 0,
        CallStage::Dataset(ExecutionPhase::Admission) => 1,
        CallStage::Dataset(ExecutionPhase::Derivation) => 2,
        CallStage::Dataset(ExecutionPhase::OutputKeys) => 3,
        CallStage::Dataset(ExecutionPhase::Verification) => 4,
        CallStage::Dataset(ExecutionPhase::Finished) => 5,
    }
}
#[derive(Clone, Copy, Default, Serialize)]
struct Calls {
    entered: u64,
    succeeded: u64,
    failed: u64,
    observed: bool,
}
#[derive(Serialize)]
struct Node {
    index: usize,
    identity: String,
}
#[derive(Serialize)]
struct Record {
    node: Option<usize>,
    phase: &'static str,
    entered_ns: u64,
    completed_ns: Option<u64>,
    outcome: Option<&'static str>,
}
#[derive(Serialize)]
struct Save {
    attempt: u64,
    entered_ns: u64,
    completed_ns: Option<u64>,
    outcome: Option<&'static str>,
}
#[derive(Serialize)]
struct Refusal {
    node: Option<usize>,
    phase: &'static str,
    resource: &'static str,
    limit: usize,
    required: Option<usize>,
}
fn resource_name(resource: yamaa_engine::dataset::Resource) -> &'static str {
    use yamaa_engine::dataset::Resource;
    match resource {
        Resource::WorkCells => "work_cells",
        Resource::ScalarTextBytes => "scalar_text_bytes",
        Resource::OutputTextBytes => "output_text_bytes",
        Resource::IdentityCells => "identity_cells",
        Resource::IdentityTextBytes => "identity_text_bytes",
        Resource::PredicateWork => "predicate_work",
        Resource::PredicateResolutions => "predicate_resolutions",
        Resource::PredicateTextBytes => "predicate_text_bytes",
        Resource::PredicateLikeWork => "predicate_like_work",
        Resource::PredicateRegexSubjectBytes => "predicate_regex_subject_bytes",
        Resource::PredicateRegexWork => "predicate_regex_work",
        Resource::PredicateRegexStateCells => "predicate_regex_state_cells",
    }
}
struct State {
    nodes: Vec<Node>,
    identity_bytes: usize,
    records: Vec<Record>,
    calls: [[Calls; 6]; NODES + 1],
    refused: Option<&'static str>,
    finished: bool,
    accepted: Option<bool>,
    saves: Vec<Save>,
    save_attempts: u64,
    refusals: Vec<Refusal>,
}
pub struct Measurements {
    origin: Instant,
    state: Mutex<State>,
}
impl Default for Measurements {
    fn default() -> Self {
        Self::new()
    }
}
impl Measurements {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            state: Mutex::new(State {
                nodes: Vec::with_capacity(NODES),
                identity_bytes: 0,
                records: Vec::with_capacity(RECORDS),
                calls: [[Calls::default(); 6]; NODES + 1],
                refused: None,
                finished: false,
                accepted: None,
                saves: Vec::with_capacity(SAVES),
                save_attempts: 0,
                refusals: Vec::with_capacity(NODES),
            }),
        }
    }
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn now(&self, state: &mut State) -> Option<u64> {
        let value = Instant::now()
            .checked_duration_since(self.origin)
            .and_then(|d| u64::try_from(d.as_nanos()).ok());
        if value.is_none() {
            state.refused = Some("monotonic_clock_unavailable");
        }
        value
    }
    pub fn complete(&self, accepted: bool) {
        let mut state = self.state();
        state.accepted = Some(accepted);
        if !state.finished
            || state
                .records
                .iter()
                .any(|record| record.completed_ns.is_none())
        {
            state.refused.get_or_insert("original_execution_incomplete");
        }
        if state
            .calls
            .iter()
            .flatten()
            .any(|calls| calls.entered != calls.succeeded.saturating_add(calls.failed))
        {
            state
                .refused
                .get_or_insert("original_function_return_incomplete");
        }
    }
    pub fn save_entered(&self) {
        let mut state = self.state();
        let Some(attempt) = state.save_attempts.checked_add(1) else {
            state.refused = Some("save_count_capacity");
            return;
        };
        state.save_attempts = attempt;
        if state.saves.len() == SAVES {
            state.refused = Some("save_event_capacity");
            return;
        }
        if let Some(now) = self.now(&mut state) {
            state.saves.push(Save {
                attempt,
                entered_ns: now,
                completed_ns: None,
                outcome: None,
            });
        }
    }
    pub fn save_completed(&self, outcome: &'static str) {
        let mut state = self.state();
        if let Some(now) = self.now(&mut state) {
            if let Some(save) = state
                .saves
                .last_mut()
                .filter(|save| save.completed_ns.is_none())
            {
                save.completed_ns = Some(now);
                save.outcome = Some(outcome);
            } else {
                state.refused = Some("save_entry_unavailable");
            }
        }
    }
    pub fn evidence(&self) -> serde_json::Value {
        let state = self.state();
        let callback_counts=state.calls.iter().enumerate().flat_map(|(index,stages)|stages.iter().enumerate().filter(|(_,calls)|calls.observed).map(move |(stage,calls)|serde_json::json!({
            "node":if index==NODES {None} else {Some(index)},"stage":(["activation","execution_admission","execution_derivation","output_keys","verification","execution_finished"][stage]),
            "entered":calls.entered,"succeeded":calls.succeeded,"failed":calls.failed
        }))).collect::<Vec<_>>();
        serde_json::json!({"protocol":"application-measurements/1", "clock":{"provider":"std::time::Instant","unit":"nanoseconds","origin":"owner-local monotonic origin immediately before original application entry"},
            "types":{"offsets":"uint64 nanoseconds since owner origin","function_counts":"uint64 actual calls","resource_counts":"usize; required null preserves original overflow/absence"},
            "classified_refusals":state.refusals,"retention_limits":{"nodes":NODES,"identity_bytes":IDENTITY_BYTES,"stage_records":RECORDS,"save_records":SAVES},
            "admitted":state.refused.is_none() && state.finished && state.accepted.is_some() && state.saves.iter().all(|save|save.completed_ns.is_some()),"refusal":state.refused,"original_execution_finished":state.finished,"build_accepted":state.accepted,
            "nodes":state.nodes,"stages":state.records,"function_calls":callback_counts,"save_attempts":state.save_attempts,"saves":state.saves})
    }
    pub fn envelope(&self, report: Option<String>) -> String {
        #[derive(Serialize)]
        struct Envelope {
            protocol: &'static str,
            report: Option<Box<serde_json::value::RawValue>>,
            measurements: serde_json::Value,
        }
        let report = report.map(|value| {
            serde_json::value::RawValue::from_string(value).expect("admitted report JSON")
        });
        serde_json::to_string(&Envelope {
            protocol: "held-observations/1",
            report,
            measurements: self.evidence(),
        })
        .expect("bounded measurement evidence")
    }
}
impl Observer for Measurements {
    fn observe(&self, event: Event<'_>) {
        let mut state = self.state();
        if state.refused.is_some() {
            return;
        }
        let node = match event {
            Event::Phase { node, .. }
            | Event::FunctionCall { node, .. }
            | Event::Exit { node, .. }
            | Event::FunctionReturn { node, .. }
            | Event::Refusal { node, .. } => node,
        };
        if let Some(node) = node {
            if node.index >= NODES {
                state.refused = Some("node_capacity");
                return;
            }
            if let Some(held) = state.nodes.iter().find(|held| held.index == node.index) {
                if held.identity != node.identity {
                    state.refused = Some("node_identity_changed");
                    return;
                }
            } else {
                let Some(total) = state
                    .identity_bytes
                    .checked_add(node.identity.len())
                    .filter(|n| *n <= IDENTITY_BYTES)
                else {
                    state.refused = Some("identity_capacity");
                    return;
                };
                state.identity_bytes = total;
                state.nodes.push(Node {
                    index: node.index,
                    identity: node.identity.to_owned(),
                });
            }
        }
        let index = node.map_or(NODES, |node| node.index);
        match event {
            Event::Phase { phase, .. } => {
                let Some(now) = self.now(&mut state) else {
                    return;
                };
                if phase == Phase::Finished {
                    if node.is_none() {
                        state.finished = true;
                    }
                    return;
                }
                if phase == Phase::Dataset(ExecutionPhase::Finished) {
                    return;
                }
                if state.records.len() == RECORDS {
                    state.refused = Some("stage_event_capacity");
                    return;
                }
                state.records.push(Record {
                    node: node.map(|node| node.index),
                    phase: phase_name(phase),
                    entered_ns: now,
                    completed_ns: None,
                    outcome: None,
                });
                match phase {
                    Phase::Activation => state.calls[index][0].observed = true,
                    Phase::Dataset(phase) => {
                        state.calls[index][stage_index(CallStage::Dataset(phase))].observed = true
                    }
                    _ => {}
                }
            }
            Event::FunctionCall { stage, .. } => {
                let calls = &mut state.calls[index][stage_index(stage)];
                calls.observed = true;
                if let Some(count) = calls.entered.checked_add(1) {
                    calls.entered = count;
                } else {
                    state.refused = Some("function_count_capacity");
                }
            }
            Event::Exit { phase, outcome, .. } => {
                let Some(now) = self.now(&mut state) else {
                    return;
                };
                if let Some(record) = state.records.iter_mut().rev().find(|record| {
                    record.node == node.map(|node| node.index)
                        && record.phase == phase_name(phase)
                        && record.completed_ns.is_none()
                }) {
                    record.completed_ns = Some(now);
                    record.outcome = Some(match outcome {
                        Outcome::Succeeded => "succeeded",
                        Outcome::Failed => "failed",
                    });
                } else {
                    state.refused = Some("stage_entry_unavailable");
                }
            }
            Event::FunctionReturn { stage, outcome, .. } => {
                let calls = &mut state.calls[index][stage_index(stage)];
                let counter = match outcome {
                    Outcome::Succeeded => &mut calls.succeeded,
                    Outcome::Failed => &mut calls.failed,
                };
                if let Some(count) = counter.checked_add(1) {
                    *counter = count;
                } else {
                    state.refused = Some("function_count_capacity");
                }
            }
            Event::Refusal { phase, refusal, .. } => {
                if state.refusals.len() == NODES {
                    state.refused = Some("refusal_event_capacity");
                    return;
                }
                state.refusals.push(Refusal {
                    node: node.map(|node| node.index),
                    phase: phase_name(phase),
                    resource: resource_name(refusal.resource),
                    limit: refusal.limit,
                    required: refusal.required,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_engine::application_observer::{Noop, Scope};
    #[test]
    fn actual_entries_returns_and_failed_stage_have_bounded_retention_and_typed_offsets() {
        let measurements = Measurements::new();
        let scope = Scope::new(&measurements);
        scope.phase(Phase::Preparation);
        scope.exit(Phase::Preparation, Outcome::Succeeded);
        scope.phase(Phase::Build);
        scope.phase(Phase::Dataset(ExecutionPhase::Derivation));
        for _ in 0..100_000 {
            measurements.observe(Event::FunctionCall {
                node: None,
                stage: CallStage::Dataset(ExecutionPhase::Derivation),
            });
            measurements.observe(Event::FunctionReturn {
                node: None,
                stage: CallStage::Dataset(ExecutionPhase::Derivation),
                outcome: Outcome::Succeeded,
            });
        }
        scope.exit(Phase::Dataset(ExecutionPhase::Derivation), Outcome::Failed);
        scope.exit(Phase::Build, Outcome::Succeeded);
        scope.phase(Phase::Finished);
        measurements.complete(false);
        let evidence = measurements.evidence();
        assert_eq!(evidence["admitted"], true);
        assert_eq!(evidence["build_accepted"], false);
        assert_eq!(evidence["stages"].as_array().unwrap().len(), 3);
        let calls = &evidence["function_calls"][0];
        assert_eq!(calls["entered"], 100_000);
        assert_eq!(calls["succeeded"], 100_000);
        assert_eq!(calls["failed"], 0);
        for stage in evidence["stages"].as_array().unwrap() {
            assert!(
                stage["completed_ns"].as_u64().unwrap() >= stage["entered_ns"].as_u64().unwrap()
            );
        }
        assert_eq!(evidence["stages"][2]["outcome"], "failed");
    }
    #[test]
    fn incomplete_callbacks_and_observer_capacity_refuse_only_measurement_admission() {
        let measurements = Measurements::new();
        let scope = Scope::new(&measurements);
        scope.phase(Phase::Dataset(ExecutionPhase::Derivation));
        measurements.observe(Event::FunctionCall {
            node: None,
            stage: CallStage::Dataset(ExecutionPhase::Derivation),
        });
        scope.exit(Phase::Dataset(ExecutionPhase::Derivation), Outcome::Failed);
        scope.phase(Phase::Finished);
        measurements.complete(false);
        assert_eq!(measurements.evidence()["admitted"], false);
        assert_eq!(measurements.evidence()["function_calls"][0]["entered"], 1);
        assert_eq!(measurements.evidence()["function_calls"][0]["failed"], 0);
        let report = r#"{"tables":[],"artifacts":[],"error":{"stage":"original"}}"#;
        let wrapped: serde_json::Value =
            serde_json::from_str(&measurements.envelope(Some(report.into()))).unwrap();
        assert_eq!(
            wrapped["report"],
            serde_json::from_str::<serde_json::Value>(report).unwrap()
        );
        let bounded = Measurements::new();
        let scope = Scope::new(&bounded);
        for _ in 0..=RECORDS {
            scope.phase(Phase::Output);
            scope.exit(Phase::Output, Outcome::Succeeded);
        }
        assert_eq!(bounded.state().records.len(), RECORDS);
        assert_eq!(bounded.evidence()["admitted"], false);
        assert_eq!(bounded.evidence()["refusal"], "stage_event_capacity");
        let _ = Scope::new(&Noop);
    }
    #[test]
    fn later_observation_cannot_create_events_and_save_retries_keep_actual_attempts() {
        let measurements = Measurements::new();
        assert_eq!(measurements.evidence()["admitted"], false);
        assert!(measurements.evidence()["stages"]
            .as_array()
            .unwrap()
            .is_empty());
        let scope = Scope::new(&measurements);
        scope.phase(Phase::Preparation);
        scope.exit(Phase::Preparation, Outcome::Succeeded);
        scope.phase(Phase::Finished);
        measurements.complete(true);
        for outcome in ["refused", "succeeded", "succeeded"] {
            measurements.save_entered();
            measurements.save_completed(outcome);
        }
        let evidence = measurements.evidence();
        assert_eq!(evidence["admitted"], true);
        assert_eq!(evidence["save_attempts"], 3);
        assert_eq!(evidence["saves"][0]["outcome"], "refused");
        assert_eq!(evidence["saves"][2]["attempt"], 3);
        measurements.save_entered();
        assert_eq!(measurements.evidence()["admitted"], false);
    }
}
