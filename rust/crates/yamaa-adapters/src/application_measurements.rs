//! Measurements captured during the original application. Reading them
//! later never runs a source, function, serializer or publisher again.
use serde::Serialize;
use std::{collections::BTreeMap, sync::Mutex, time::Instant};
use yamaa_core::{
    application_observer::{CallStage, Event, Observer, Outcome, Phase},
    dataset::ExecutionPhase,
};

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
fn resource_name(resource: yamaa_core::dataset::Resource) -> &'static str {
    crate::resource_diagnostics::resource_name(resource)
}

struct State {
    nodes: Vec<Node>,
    records: Vec<Record>,
    calls: BTreeMap<Option<usize>, [Calls; 6]>,
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
                nodes: Vec::new(),
                records: Vec::new(),
                calls: BTreeMap::new(),
                refused: None,
                finished: false,
                accepted: None,
                saves: Vec::new(),
                save_attempts: 0,
                refusals: Vec::new(),
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
            .values()
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
        if let Some(now) = self.now(&mut state) {
            state.saves.push(Save {
                attempt,
                entered_ns: now,
                completed_ns: None,
                outcome: None,
            });
        }
    }
    /// Record a finished save from its result. Every host maps the same
    /// result to the same outcome: a saved build is `succeeded`, a refused
    /// save is `refused`, and any error is `failed`.
    pub fn save_completed_result<E>(&self, result: &Result<bool, E>) {
        self.save_completed(match result {
            Ok(true) => "succeeded",
            Ok(false) => "refused",
            Err(_) => "failed",
        });
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
    pub fn evidence(&self) -> yamaa_core::json::Value {
        let state = self.state();
        let callback_counts=state.calls.iter().filter(|(node, _)| node.is_some()).chain(state.calls.iter().filter(|(node, _)| node.is_none())).flat_map(|(node,stages)|stages.iter().enumerate().filter(|(_,calls)|calls.observed).map(move |(stage,calls)|crate::json!({
            "node":node,"stage":(["activation","execution_admission","execution_derivation","output_keys","verification","execution_finished"][stage]),
            "entered":calls.entered,"succeeded":calls.succeeded,"failed":calls.failed
        }))).collect::<Vec<_>>();
        crate::json!({"protocol":"application-measurements/1", "clock":{"provider":"std::time::Instant","unit":"nanoseconds","origin":"owner-local monotonic origin immediately before original application entry"},
            "types":{"offsets":"uint64 nanoseconds since owner origin","function_counts":"uint64 actual calls","resource_counts":"usize; required null preserves original overflow/absence"},
            "classified_refusals":state.refusals,
            "admitted":state.refused.is_none() && state.finished && state.accepted.is_some() && state.saves.iter().all(|save|save.completed_ns.is_some()),"refusal":state.refused,"original_execution_finished":state.finished,"build_accepted":state.accepted,
            "nodes":state.nodes,"stages":state.records,"function_calls":callback_counts,"save_attempts":state.save_attempts,"saves":state.saves})
    }
    pub fn envelope(&self, report: Option<String>) -> String {
        #[derive(Serialize)]
        struct Envelope<'a> {
            protocol: &'static str,
            report: Option<Box<serde_json::value::RawValue>>,
            measurements: crate::json_wire::WireValue<'a>,
        }
        let report = report.map(|value| {
            serde_json::value::RawValue::from_string(value).expect("admitted report JSON")
        });
        let evidence = self.evidence();
        serde_json::to_string(&Envelope {
            protocol: "held-observations/1",
            report,
            measurements: crate::json_wire::WireValue(&evidence),
        })
        .expect("measurement evidence is serializable")
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
            if let Some(held) = state.nodes.iter().find(|held| held.index == node.index) {
                if held.identity != node.identity {
                    state.refused = Some("node_identity_changed");
                    return;
                }
            } else {
                state.nodes.push(Node {
                    index: node.index,
                    identity: node.identity.to_owned(),
                });
            }
        }
        let index = node.map(|node| node.index);
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
                state.records.push(Record {
                    node: node.map(|node| node.index),
                    phase: phase_name(phase),
                    entered_ns: now,
                    completed_ns: None,
                    outcome: None,
                });
                match phase {
                    Phase::Activation => state.calls.entry(index).or_default()[0].observed = true,
                    Phase::Dataset(phase) => {
                        state.calls.entry(index).or_default()
                            [stage_index(CallStage::Dataset(phase))]
                        .observed = true
                    }
                    _ => {}
                }
            }
            Event::FunctionCall { stage, .. } => {
                let calls = &mut state.calls.entry(index).or_default()[stage_index(stage)];
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
                let calls = &mut state.calls.entry(index).or_default()[stage_index(stage)];
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
    use yamaa_core::application_observer::{Noop, Scope};
    #[test]
    fn actual_entries_returns_and_failed_stage_retain_complete_typed_offsets() {
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
    fn incomplete_callbacks_refuse_measurement_admission_without_losing_the_original_report() {
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
        let wrapped: yamaa_core::json::Value =
            crate::json_wire::decode(&measurements.envelope(Some(report.into()))).unwrap();
        assert_eq!(wrapped["report"], crate::json_wire::decode(report).unwrap());
        let _ = Scope::new(&Noop);
    }
    #[test]
    fn complete_observations_keep_all_nodes_stages_identities_and_save_attempts() {
        let measurements = Measurements::new();
        let scope = Scope::new(&measurements);
        let long_identity = "é".repeat(524_289);
        for index in 0..65 {
            let identity = if index == 64 {
                &long_identity
            } else {
                "ordinary-node"
            };
            let scope = scope.for_node(index, identity);
            scope.phase(Phase::Activation);
            scope.exit(Phase::Activation, Outcome::Succeeded);
        }
        for _ in 0..1025 {
            scope.phase(Phase::Output);
            scope.exit(Phase::Output, Outcome::Succeeded);
        }
        scope.phase(Phase::Finished);
        measurements.complete(true);
        for _ in 0..17 {
            measurements.save_entered();
            measurements.save_completed("succeeded");
        }
        let evidence = measurements.evidence();
        assert_eq!(evidence["admitted"], true);
        assert_eq!(evidence["nodes"].as_array().unwrap().len(), 65);
        assert_eq!(evidence["nodes"][64]["identity"], long_identity);
        assert_eq!(evidence["function_calls"].as_array().unwrap().len(), 65);
        assert_eq!(evidence["function_calls"][64]["node"], 64);
        assert_eq!(evidence["stages"].as_array().unwrap().len(), 1090);
        assert_eq!(evidence["save_attempts"], 17);
        assert_eq!(evidence["saves"].as_array().unwrap().len(), 17);
        assert!(evidence["retention_limits"].is_null());
    }
    #[test]
    fn save_completed_result_maps_the_shared_outcomes() {
        let measurements = Measurements::new();
        let scope = Scope::new(&measurements);
        scope.phase(Phase::Preparation);
        scope.exit(Phase::Preparation, Outcome::Succeeded);
        scope.phase(Phase::Finished);
        measurements.complete(true);
        measurements.save_entered();
        measurements.save_completed_result(&Ok::<bool, ()>(true));
        measurements.save_entered();
        measurements.save_completed_result(&Ok::<bool, ()>(false));
        measurements.save_entered();
        measurements.save_completed_result(&Err::<bool, ()>(()));
        let evidence = measurements.evidence();
        assert_eq!(evidence["saves"][0]["outcome"], "succeeded");
        assert_eq!(evidence["saves"][1]["outcome"], "refused");
        assert_eq!(evidence["saves"][2]["outcome"], "failed");
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
