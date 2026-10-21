//! Project operations over the specifications a project file lists: section
//! keys, the producer-first run order of a run scope and the project-level
//! findings. Metadata only; adapters read the project and its specifications.
use crate::diagnostic::{ConditionCode, Diagnostic};
use crate::json::Value;
use crate::project_environment::Submission;

/// Every section in run order: a project run without a selected section
/// builds the declared ones in this order.
pub const SECTIONS: [Submission; 3] = [Submission::Sdtm, Submission::Adam, Submission::Send];

/// The project key that declares a section.
pub fn key(section: Submission) -> &'static str {
    match section {
        Submission::Sdtm => "sdtm",
        Submission::Adam => "adam",
        Submission::Send => "send",
    }
}

/// The section a project key names; any other text names none.
pub fn section(key: &str) -> Option<Submission> {
    SECTIONS
        .into_iter()
        .find(|section| self::key(*section) == key)
}

fn text(value: &str) -> Value {
    Value::String(value.into())
}

fn texts<'a>(values: impl IntoIterator<Item = &'a str>) -> Value {
    Value::Array(values.into_iter().map(text).collect())
}

fn finding(
    code: ConditionCode,
    spec_paths: Vec<String>,
    context: Vec<(&str, Value)>,
) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths,
        context: context
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// Two or more listings share one name. `listings` pairs each listing's
/// project path, such as `sdtm.specs[0]`, with the specification it names.
pub fn duplicate_name(name: &str, listings: &[(String, String)]) -> Diagnostic {
    finding(
        ConditionCode::ProjectDuplicateName,
        listings.iter().map(|(path, _)| path.clone()).collect(),
        vec![
            ("name", text(name)),
            (
                "specifications",
                texts(
                    listings
                        .iter()
                        .map(|(_, specification)| specification.as_str()),
                ),
            ),
        ],
    )
}

/// No listed specification has the requested name. The context names every
/// name the project defines, sorted.
pub fn unknown_name(name: &str, available: &[&str]) -> Diagnostic {
    let mut available = available.to_vec();
    available.sort_unstable();
    available.dedup();
    finding(
        ConditionCode::ProjectUnknownName,
        Vec::new(),
        vec![("name", text(name)), ("available", texts(available))],
    )
}

/// The requested section is not one the project declares: a name outside
/// `sdtm`, `adam` and `send`, or one of them the project does not declare.
/// The context names every section the project declares, sorted.
pub fn unknown_section(section: &str, declared: &[&str]) -> Diagnostic {
    let mut declared = declared.to_vec();
    declared.sort_unstable();
    declared.dedup();
    finding(
        ConditionCode::ProjectUnknownSection,
        Vec::new(),
        vec![("section", text(section)), ("sections", texts(declared))],
    )
}

/// An input without `schema` reads the file a specification in the run scope
/// produces. `written` is the input's path as declared and `producer` the
/// specification whose `output.path` names that file.
pub fn unlinked_producer(dataset: &str, written: &str, producer: &str) -> Diagnostic {
    finding(
        ConditionCode::ProjectUnlinkedProducer,
        vec![format!("input.{dataset}.path")],
        vec![
            ("dataset", text(dataset)),
            ("path", text(written)),
            ("producer", text(producer)),
        ],
    )
}

/// A consumer's producer in the run scope failed or never ran, so the
/// consumer does not run.
pub fn producer_not_completed(dataset: &str, producer: &str) -> Diagnostic {
    finding(
        ConditionCode::ProjectProducerNotCompleted,
        vec![format!("input.{dataset}.schema")],
        vec![("dataset", text(dataset)), ("producer", text(producer))],
    )
}

/// A project run that built no specification has nothing to save, so saving
/// it fails.
pub fn run_nothing_to_save() -> Diagnostic {
    finding(
        ConditionCode::ProjectRunNothingToSave,
        Vec::new(),
        Vec::new(),
    )
}

/// A project submission that composed no section has nothing to save, so
/// saving it fails.
pub fn submission_nothing_to_save() -> Diagnostic {
    finding(
        ConditionCode::ProjectSubmissionNothingToSave,
        Vec::new(),
        Vec::new(),
    )
}

/// A link on a dependency cycle among run-scope specifications. `cycle` names
/// the specifications from the first one entered back to itself, as a
/// producer graph reports a cycle.
pub fn workflow_cycle(dataset: &str, cycle: &[&str]) -> Diagnostic {
    finding(
        ConditionCode::ProducerWorkflowCycle,
        vec![format!("input.{dataset}.schema")],
        vec![("cycle", texts(cycle.iter().copied()))],
    )
}

/// One dependency cycle: the specifications on it, each followed by one of
/// its producers, ending with the specification whose link closes the cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cycle {
    pub members: Vec<usize>,
}

/// The run order of a scope and the cycles that leave part of it unordered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    /// Every specification exactly once, each after the producers it reaches
    /// without a cycle.
    pub order: Vec<usize>,
    pub cycles: Vec<Cycle>,
}

/// Order a run scope producer first. `dependencies[index]` lists the
/// in-scope producers of specification `index` in authored link order.
/// Independent specifications keep scope order, and a producer shared by
/// several consumers appears once, before the first of them.
pub fn order(dependencies: &[Vec<usize>]) -> Order {
    let mut state = vec![0u8; dependencies.len()];
    let mut order = Vec::with_capacity(dependencies.len());
    let mut cycles = Vec::new();
    for start in 0..dependencies.len() {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut stack = vec![(start, 0usize)];
        while let Some(&(node, next)) = stack.last() {
            if next == dependencies[node].len() {
                state[node] = 2;
                order.push(node);
                stack.pop();
                continue;
            }
            stack.last_mut().expect("nonempty traversal").1 += 1;
            let producer = dependencies[node][next];
            match state[producer] {
                0 => {
                    state[producer] = 1;
                    stack.push((producer, 0));
                }
                1 => {
                    let begin = stack
                        .iter()
                        .position(|&(entered, _)| entered == producer)
                        .expect("active producer");
                    cycles.push(Cycle {
                        members: stack[begin..].iter().map(|&(entered, _)| entered).collect(),
                    });
                }
                _ => {}
            }
        }
    }
    // A back edge finds only the producers still being traversed, so a
    // specification whose cycle closes through a finished producer is listed
    // by the shortest cycle through it.
    let mut listed = vec![false; dependencies.len()];
    for cycle in &cycles {
        for &member in &cycle.members {
            listed[member] = true;
        }
    }
    for start in 0..dependencies.len() {
        if listed[start] {
            continue;
        }
        if let Some(members) = shortest_cycle(dependencies, start) {
            for &member in &members {
                listed[member] = true;
            }
            cycles.push(Cycle { members });
        }
    }
    Order { order, cycles }
}

/// The shortest dependency path from `start` back to itself, `start` first,
/// taking producers in authored link order.
fn shortest_cycle(dependencies: &[Vec<usize>], start: usize) -> Option<Vec<usize>> {
    let mut previous = vec![None; dependencies.len()];
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for &producer in &dependencies[node] {
            if producer == start {
                let mut members = vec![node];
                while let Some(before) = previous[*members.last().expect("nonempty path")] {
                    members.push(before);
                }
                members.reverse();
                return Some(members);
            }
            if previous[producer].is_none() {
                previous[producer] = Some(node);
                queue.push_back(producer);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_round_trip_through_their_keys_in_run_order() {
        let keys = SECTIONS.map(key);
        assert_eq!(keys, ["sdtm", "adam", "send"]);
        for (section, key) in SECTIONS.into_iter().zip(keys) {
            assert_eq!(super::section(key), Some(section));
        }
        assert_eq!(section("SDTM"), None);
        assert_eq!(section(""), None);
    }

    #[test]
    fn producers_run_first_and_independent_specifications_keep_scope_order() {
        // 0 reads 2; 1 is independent; 2 reads nothing; 3 reads 2 and 0.
        let order = order(&[vec![2], vec![], vec![], vec![2, 0]]);
        assert_eq!(order.order, [2, 0, 1, 3]);
        assert!(order.cycles.is_empty());
    }

    #[test]
    fn a_cycle_is_reported_once_and_every_specification_still_appears_once() {
        // 0 reads 1, 1 reads 2, 2 reads 0; 3 reads 3; 4 reads 1.
        let order = order(&[vec![1], vec![2], vec![0], vec![3], vec![1]]);
        let mut seen = order.order.clone();
        seen.sort_unstable();
        assert_eq!(seen, [0, 1, 2, 3, 4]);
        assert_eq!(
            order.cycles,
            [
                Cycle {
                    members: vec![0, 1, 2]
                },
                Cycle { members: vec![3] }
            ]
        );
    }

    #[test]
    fn a_cycle_closing_through_a_finished_producer_lists_every_member() {
        // 0 reads 1 and 2, 1 reads 0, 2 reads 1: 2 is on 0 -> 2 -> 1 -> 0,
        // whose link from 2 reaches 1 after 1 has finished.
        let order = order(&[vec![1, 2], vec![0], vec![1]]);
        assert_eq!(order.order, [1, 2, 0]);
        assert_eq!(
            order.cycles,
            [
                Cycle {
                    members: vec![0, 1]
                },
                Cycle {
                    members: vec![2, 1, 0]
                }
            ]
        );
    }

    #[test]
    fn unknown_names_list_the_defined_names_sorted_once() {
        let finding = unknown_name("XX", &["DM", "AE", "DM"]);
        assert!(finding.spec_paths.is_empty());
        assert_eq!(finding.context["available"], texts(["AE", "DM"]));
        assert_eq!(finding.context["name"], text("XX"));
    }
}
