//! Deterministic analysis of already-bound dependency graphs, without host effects.
use alloc::{collections::BTreeSet, vec, vec::Vec};

/// Graph work limits, separate from language validation conditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub nodes: usize,
    /// Count written edges before duplicate removal.
    pub edges: usize,
}

impl Default for Limits {
    /// Bound the prototype's graph allocations and work without recursive traversal.
    fn default() -> Self {
        Self {
            nodes: 4096,
            edges: 65_536,
        }
    }
}

/// Rejected graph shape or resource policy; a dependency cycle is an analysis result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    NodeLimit { limit: usize, required: usize },
    EdgeLimit { limit: usize, required: usize },
    SizeOverflow,
    InvalidDependency { node: usize, dependency: usize },
}

/// Declaration indices retain both the portable cycle spelling and execution order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Analysis {
    /// First cycle reached by declaration-ordered DFS, rotated to its earliest member.
    pub cycle: Option<Vec<usize>>,
    /// Repeatedly choose the earliest declared ready node; omit cycle-blocked nodes.
    pub order: Vec<usize>,
}

/// Analyze one graph whose nodes are indexed in declaration order.
///
/// Bindings to values outside this scheduling phase must be omitted by the binder.
/// Every supplied edge refers to another node in this graph. Edge order and
/// repetition do not change the result, but repeated edges still consume budget.
/// This service neither resolves references nor decides whether forward reads are legal.
pub fn analyze(dependencies: &[Vec<usize>], limits: Limits) -> Result<Analysis, Error> {
    admit(
        dependencies.len(),
        dependencies.iter().map(Vec::as_slice),
        limits,
    )?;
    Ok(analyze_admitted(dependencies.to_vec()))
}

/// Admit borrowed edges before either graph service copies or filters them.
pub(crate) fn admit<'a>(
    nodes: usize,
    dependencies: impl Iterator<Item = &'a [usize]> + Clone,
    limits: Limits,
) -> Result<(), Error> {
    if nodes > limits.nodes {
        return Err(Error::NodeLimit {
            limit: limits.nodes,
            required: nodes,
        });
    }
    let edges = dependencies
        .clone()
        .try_fold(0usize, |count, dependencies| {
            count
                .checked_add(dependencies.len())
                .ok_or(Error::SizeOverflow)
        })?;
    if edges > limits.edges {
        return Err(Error::EdgeLimit {
            limit: limits.edges,
            required: edges,
        });
    }
    for (node, dependencies_for_node) in dependencies.enumerate() {
        for &dependency in dependencies_for_node {
            if dependency >= nodes {
                return Err(Error::InvalidDependency { node, dependency });
            }
        }
    }
    Ok(())
}

/// Analyze an owned graph after its caller has admitted indices and work limits.
pub(crate) fn analyze_admitted(mut graph: Vec<Vec<usize>>) -> Analysis {
    for dependencies in &mut graph {
        dependencies.sort_unstable();
        dependencies.dedup();
    }
    Analysis {
        cycle: first_cycle(&graph),
        order: stable_order(&graph),
    }
}

/// Reproduce declaration-ordered DFS with an explicit stack rather than host recursion.
fn first_cycle(graph: &[Vec<usize>]) -> Option<Vec<usize>> {
    let mut states = vec![0u8; graph.len()];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for root in 0..graph.len() {
        if states[root] != 0 {
            continue;
        }
        states[root] = 1;
        stack.push((root, 0));
        while let Some(&(node, next)) = stack.last() {
            if next == graph[node].len() {
                states[node] = 2;
                stack.pop();
                continue;
            }
            let dependency = graph[node][next];
            stack.last_mut().expect("current traversal node").1 += 1;
            match states[dependency] {
                0 => {
                    states[dependency] = 1;
                    stack.push((dependency, 0));
                }
                1 => {
                    let start = stack
                        .iter()
                        .position(|&(active, _)| active == dependency)
                        .expect("active nodes are on the traversal stack");
                    let mut cycle: Vec<_> = stack[start..].iter().map(|&(n, _)| n).collect();
                    let earliest = cycle
                        .iter()
                        .enumerate()
                        .min_by_key(|&(_, node)| node)
                        .map(|(index, _)| index)
                        .expect("cycle has at least one member");
                    cycle.rotate_left(earliest);
                    cycle.push(cycle[0]);
                    return Some(cycle);
                }
                _ => (),
            }
        }
    }
    None
}

/// Select one earliest ready node at a time, including newly ready earlier declarations.
fn stable_order(graph: &[Vec<usize>]) -> Vec<usize> {
    let mut order = Vec::with_capacity(graph.len());
    match visit_stable_order(graph, |next| {
        if let Some(node) = next {
            order.push(node);
        }
        Ok::<_, core::convert::Infallible>(())
    }) {
        Ok(()) => order,
        Err(never) => match never {},
    }
}

/// Bound all scheduler vector/tree slots before allocating its admitted state.
/// Counts cover remaining dependencies, reader lists and the maximal ready set;
/// the caller owns and separately accounts for its emitted-order representation.
pub(crate) fn stable_order_slots(graph: &[Vec<usize>]) -> usize {
    graph
        .iter()
        .fold(graph.len().saturating_mul(3), |slots, edges| {
            slots.saturating_add(edges.len())
        })
}

/// Visit the same earliest-ready schedule for every already-admitted graph.
/// Indices and graph storage must be admitted under the caller's own policies;
/// this service applies no generic graph limits. Before emitting a node, or
/// completing a stalled/exhausted schedule, it gives the caller an admission
/// point. An error stops visitation immediately; no later node is emitted.
pub(crate) fn visit_stable_order<E>(
    graph: &[Vec<usize>],
    mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<(), E> {
    let mut remaining: Vec<_> = graph.iter().map(Vec::len).collect();
    let mut readers = vec![Vec::new(); graph.len()];
    for (node, dependencies) in graph.iter().enumerate() {
        for &dependency in dependencies {
            readers[dependency].push(node);
        }
    }
    let mut ready: BTreeSet<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(node, &count)| (count == 0).then_some(node))
        .collect();
    loop {
        let next = ready.pop_first();
        admit(next)?;
        let Some(node) = next else { break };
        for &reader in &readers[node] {
            remaining[reader] -= 1;
            if remaining[reader] == 0 {
                ready.insert(reader);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_scheduler_preserves_admission_points_and_stops_on_refusal() {
        let graph = vec![vec![2], vec![2], vec![], vec![]];
        let mut attempts = Vec::new();
        assert_eq!(
            visit_stable_order(&graph, |next| {
                attempts.push(next);
                if next == Some(0) {
                    Err("work exhausted")
                } else {
                    Ok(())
                }
            }),
            Err("work exhausted")
        );
        assert_eq!(attempts, vec![Some(2), Some(0)]);
        let mut attempts = Vec::new();
        visit_stable_order(&graph, |next| {
            attempts.push(next);
            Ok::<_, core::convert::Infallible>(())
        })
        .unwrap();
        assert_eq!(attempts, vec![Some(2), Some(0), Some(1), Some(3), None]);
    }

    #[test]
    fn admitted_scheduler_uses_caller_limits_instead_of_the_probe_node_cap() {
        let graph = vec![vec![]; Limits::default().nodes + 1];
        let mut order = Vec::new();
        visit_stable_order(&graph, |next| {
            if let Some(node) = next {
                order.push(node);
            }
            Ok::<_, core::convert::Infallible>(())
        })
        .unwrap();
        assert_eq!(order, (0..4097).collect::<Vec<_>>());
        assert!(matches!(
            analyze(&graph, Limits::default()),
            Err(Error::NodeLimit { .. })
        ));
    }
}
