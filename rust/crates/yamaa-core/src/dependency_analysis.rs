//! Deterministic analysis of already-bound dependency graphs, without host effects.
use std::{collections::BTreeSet, vec};

/// Rejected graph shape; a dependency cycle is an analysis result.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("dependency analysis invalid dependency: node={node:?}, dependency={dependency:?}")]
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
/// repetition do not change the result.
/// This service neither resolves references nor decides whether forward reads are legal.
pub fn analyze(dependencies: &[Vec<usize>]) -> Result<Analysis, Error> {
    validate_indices(dependencies.len(), dependencies.iter().map(Vec::as_slice))?;
    Ok(analyze_admitted(dependencies.to_vec()))
}

/// Validate borrowed edges before either graph service copies or filters them.
pub(crate) fn validate_indices<'a>(
    nodes: usize,
    dependencies: impl Iterator<Item = &'a [usize]>,
) -> Result<(), Error> {
    for (node, dependencies_for_node) in dependencies.enumerate() {
        for &dependency in dependencies_for_node {
            if dependency >= nodes {
                return Err(Error::InvalidDependency { node, dependency });
            }
        }
    }
    Ok(())
}

/// Analyze an owned graph after its caller has validated indices.
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

/// Strongly connected components, with members and components in declaration
/// order. The caller has already admitted the graph's indices.
pub(crate) fn components(graph: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut seen = vec![false; graph.len()];
    let mut finish = Vec::new();
    for root in 0..graph.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if *next == graph[*node].len() {
                finish.push(*node);
                stack.pop();
            } else {
                let child = graph[*node][*next];
                *next += 1;
                if !std::mem::replace(&mut seen[child], true) {
                    stack.push((child, 0));
                }
            }
        }
    }
    let mut reverse = vec![Vec::new(); graph.len()];
    for (node, edges) in graph.iter().enumerate() {
        for &edge in edges {
            reverse[edge].push(node);
        }
    }
    seen.fill(false);
    let mut result = Vec::new();
    for root in finish.into_iter().rev() {
        if std::mem::replace(&mut seen[root], true) {
            continue;
        }
        let mut members = Vec::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            members.push(node);
            for &child in &reverse[node] {
                if !std::mem::replace(&mut seen[child], true) {
                    pending.push(child);
                }
            }
        }
        members.sort_unstable();
        result.push(members);
    }
    result.sort_by_key(|members| members[0]);
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_selects_newly_ready_earlier_declarations_first() {
        let graph = vec![vec![2], vec![2], vec![], vec![]];
        assert_eq!(
            analyze(&graph),
            Ok(Analysis {
                cycle: None,
                order: vec![2, 0, 1, 3],
            })
        );
    }

    #[test]
    fn iterative_scheduler_handles_thousands_of_independent_declarations() {
        let graph = vec![vec![]; 4097];
        let analysis = analyze(&graph).unwrap();
        assert_eq!(analysis.order, (0..4097).collect::<Vec<_>>());
        assert_eq!(analysis.cycle, None);
    }
}
