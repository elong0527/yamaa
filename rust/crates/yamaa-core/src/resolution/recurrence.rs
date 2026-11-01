//! Layer ordering distinguishes earlier-row reads before binding types.
//! The bound column policy still admits and schedules feedback components.
use super::{dependency_analysis, ExprKind, Live, Scope, Step};
use crate::ast::{Direction, ExprId, Nulls, Window};
use std::collections::BTreeSet;

pub(super) fn order(
    live: &Live<'_>,
    selected: &[usize],
    mut graph: Vec<Vec<usize>>,
) -> dependency_analysis::Analysis {
    for members in dependency_analysis::components(&graph) {
        if members.len() == 1 && !graph[members[0]].contains(&members[0]) {
            continue;
        }
        let Some(delayed) = delayed(live, selected, &members) else {
            continue;
        };
        let mut current = graph.clone();
        for (&column, names) in members.iter().zip(&delayed) {
            current[column].retain(|dependency| {
                !members.contains(dependency)
                    || !names.contains(&live.spec.columns[selected[*dependency]].name.name)
            });
        }
        // Keep an unsupported current-row cycle in the original graph, so
        // its refusal retains the existing closed loop and authored paths.
        let local = members
            .iter()
            .map(|&column| {
                current[column]
                    .iter()
                    .filter_map(|dependency| members.iter().position(|member| member == dependency))
                    .collect()
            })
            .collect();
        if dependency_analysis::analyze_admitted(local).cycle.is_none() {
            graph = current;
        }
    }
    dependency_analysis::analyze_admitted(graph)
}

/// Only the source operand of a pure, unfiltered, strictly backward read
/// is delayed. Keys, row overrides, joins and other window shapes keep
/// ordinary layer ordering and its pre-data cycle diagnostics.
fn delayed(
    live: &Live<'_>,
    selected: &[usize],
    members: &[usize],
) -> Option<Vec<BTreeSet<String>>> {
    let spec = live.spec;
    let names: BTreeSet<_> = members
        .iter()
        .map(|&column| spec.columns[selected[column]].name.name.as_str())
        .collect();
    if spec
        .keys
        .iter()
        .any(|key| names.contains(key.name.as_str()))
        || spec.rows.templates.iter().any(|template| {
            template
                .overrides
                .iter()
                .any(|(name, _)| names.contains(name.name.as_str()))
        })
    {
        return None;
    }
    let mut shared = None;
    let mut result = Vec::new();
    for &column in members {
        let root = spec.columns[selected[column]].derivation.as_ref()?.expr;
        let mut sources = BTreeSet::new();
        let mut safe = true;
        spec.walk(root, Scope::Row, |at, _| {
            match &spec.expr(at).kind {
                ExprKind::Window(window) => {
                    let backward = window
                        .function
                        .row_value_offset()
                        .is_some_and(|offset| offset < 0);
                    if !backward
                        || window.filter.is_some()
                        || window.order_by.is_empty()
                        || window
                            .operands
                            .iter()
                            .any(|operand| reference(live, operand.expr).is_none())
                        || shared.is_some_and(|other| !same_window(live, other, window))
                        || window
                            .group_by
                            .iter()
                            .copied()
                            .chain(window.order_by.iter().map(|term| term.variable))
                            .any(|field| {
                                reference(live, field).is_none_or(|name| names.contains(name))
                            })
                    {
                        safe = false;
                        return Step::Stop;
                    }
                    shared = Some(window);
                    sources.extend(window.operands.iter().map(|operand| operand.expr));
                }
                ExprKind::Ref(name)
                    if name.owner().is_some_and(|owner| Some(owner) != live.driver) =>
                {
                    safe = false;
                }
                ExprKind::Source(source)
                    if source.filter.is_some()
                        || source.between.is_some()
                        || source.keep.is_some()
                        || !source.order_by.is_empty() =>
                {
                    safe = false;
                }
                ExprKind::Aggregate(_)
                | ExprKind::Odm(_)
                | ExprKind::Call { .. }
                | ExprKind::Unsupported(_) => safe = false,
                _ => (),
            }
            Step::Continue
        });
        if !safe {
            return None;
        }
        let (mut delayed, mut current) = (BTreeSet::new(), BTreeSet::new());
        for read in spec.reads(root, Scope::Row) {
            if read.name.owner().is_none() {
                if sources.contains(&read.at) && read.scope == Scope::Row {
                    delayed.insert(read.name.0);
                } else {
                    current.insert(read.name.0);
                }
            }
        }
        delayed.retain(|name| !current.contains(name));
        result.push(delayed);
    }
    shared.map(|_| result)
}

fn reference<'a>(live: &Live<'a>, id: ExprId) -> Option<&'a str> {
    match &live.spec.expr(id).kind {
        ExprKind::Ref(name) if name.owner().is_none() => Some(&name.0),
        _ => None,
    }
}

fn same_window(live: &Live<'_>, left: &Window, right: &Window) -> bool {
    left.group_by
        .iter()
        .map(|&id| reference(live, id))
        .eq(right.group_by.iter().map(|&id| reference(live, id)))
        && left
            .order_by
            .iter()
            .map(|term| {
                (
                    reference(live, term.variable),
                    term.direction,
                    term.nulls
                        .map_or(term.direction != Direction::Desc, |nulls| {
                            nulls == Nulls::First
                        }),
                )
            })
            .eq(right.order_by.iter().map(|term| {
                (
                    reference(live, term.variable),
                    term.direction,
                    term.nulls
                        .map_or(term.direction != Direction::Desc, |nulls| {
                            nulls == Nulls::First
                        }),
                )
            }))
}
