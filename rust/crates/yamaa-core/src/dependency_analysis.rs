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
    let mut order = Vec::with_capacity(graph.len());
    while let Some(node) = ready.pop_first() {
        order.push(node);
        for &reader in &readers[node] {
            remaining[reader] -= 1;
            if remaining[reader] == 0 {
                ready.insert(reader);
            }
        }
    }
    order
}
