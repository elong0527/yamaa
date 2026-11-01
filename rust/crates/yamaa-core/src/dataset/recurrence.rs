//! Admit causal column feedback once, before the immutable plan is lowered.
use super::*;
use crate::{column_dependencies, dependency_analysis};
use std::collections::BTreeSet;

/// A feedback component in same-row dependency order. `window_path` names
/// one of its backward reads; every participating window has that order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Recurrence {
    pub columns: Vec<usize>,
    pub window_path: String,
}

#[derive(Default)]
struct Reads<'a> {
    current: BTreeSet<usize>,
    delayed: Vec<(usize, &'a Window, &'a str)>,
    unsafe_read: bool,
}

impl<'a> Reads<'a> {
    fn read(&mut self, read: &Read) {
        match read {
            Read::Column(column) => {
                self.current.insert(*column);
            }
            Read::Source(_) => (),
            _ => self.unsafe_read = true,
        }
    }

    fn assignment(&mut self, assignment: &'a Assignment) {
        match &assignment.expression {
            Expression::Column(column) => {
                self.current.insert(*column);
            }
            Expression::Source(_) | Expression::Literal(_) => (),
            Expression::Collect {
                filter: None,
                selection: None,
                ..
            } => (),
            Expression::Compute(plan) => {
                for binding in plan.bindings() {
                    self.read(&binding.read);
                }
            }
            Expression::Flag(flag) => {
                for binding in flag.bindings() {
                    if let FlagRead::Read(read) = &binding.read {
                        self.read(read);
                    }
                }
            }
            Expression::Window(window) => {
                if let WindowKind::RowValue { column, offset } = window.kind {
                    if offset < 0 && window.filter.is_none() {
                        self.delayed.push((column, window, &assignment.path));
                    } else {
                        self.unsafe_read = true;
                    }
                } else {
                    self.unsafe_read = true;
                }
                self.current.extend(&window.group_by);
                self.current
                    .extend(window.order_by.iter().map(|term| term.column));
            }
            Expression::Case(branches) => {
                for branch in branches {
                    if let Some(predicate) = &branch.when {
                        for binding in predicate.bindings() {
                            self.read(&binding.read);
                        }
                    }
                    self.assignment(&branch.assignment);
                }
            }
            Expression::Extreme { operands, .. }
            | Expression::StrTemplate { operands, .. }
            | Expression::Imputation { operands, .. }
            | Expression::Temporal { operands, .. } => {
                for operand in operands {
                    self.assignment(operand);
                }
            }
            Expression::MappingCut { source, .. }
            | Expression::Round { source, .. }
            | Expression::StrExtract { source, .. }
            | Expression::StrReplace { source, .. }
            | Expression::StrCase { source, .. }
            | Expression::StrPad { source, .. }
            | Expression::StrSubstr { source, .. }
            | Expression::StrLength { source, .. }
            | Expression::StrDecimal { source, .. }
            | Expression::SourceBinding { source, .. } => self.assignment(source),
            Expression::FirstAvailable(selection) => {
                for operand in selection.sources() {
                    match &operand.read {
                        SelectionRead::Column(column) => {
                            self.current.insert(*column);
                        }
                        SelectionRead::Source(_) => (),
                        SelectionRead::Expression(source) => self.assignment(source),
                        _ => self.unsafe_read = true,
                    }
                }
            }
            _ => self.unsafe_read = true,
        }
    }
}

/// Preserve the existing column policy except for admitted feedback components
/// and forward references that read a strictly earlier position. `None`
/// programs are incomplete or mixed-scope declarations, never feedback state.
pub(crate) fn analyze(
    dependencies: &[Option<Vec<usize>>],
    programs: &[Option<&Assignment>],
    keys: &[usize],
    has_rows: bool,
) -> Result<(column_dependencies::Analysis, Vec<Recurrence>), column_dependencies::Error> {
    // Keep validation and its existing diagnostic identities owned by the
    // column policy. Components below only change which reads are delayed.
    let ordinary = column_dependencies::analyze(dependencies, keys, has_rows)?;
    let reads: Vec<_> = programs
        .iter()
        .map(|program| {
            let mut reads = Reads::default();
            if let Some(program) = program {
                reads.assignment(program);
            } else {
                reads.unsafe_read = true;
            }
            reads
        })
        .collect();
    let graph: Vec<Vec<usize>> = dependencies
        .iter()
        .map(|edges| {
            edges
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .copied()
                .filter(|&column| dependencies[column].is_some())
                .collect()
        })
        .collect();
    let mut groups = Vec::new();
    for members in dependency_analysis::components(&graph) {
        let cyclic = members.len() > 1 || graph[members[0]].contains(&members[0]);
        if !cyclic
            || members
                .iter()
                .any(|column| keys.contains(column) || reads[*column].unsafe_read)
        {
            continue;
        }
        if let Some(group) = admit(&members, &reads, &graph) {
            groups.push(group);
        }
    }
    if groups.is_empty() {
        return Ok((without_delayed_forwards(ordinary, &reads), groups));
    }
    let mut owner: Vec<usize> = (0..dependencies.len()).collect();
    for group in &groups {
        let leader = *group.columns.iter().min().expect("nonempty component");
        for &column in &group.columns {
            owner[column] = leader;
        }
    }
    let mut contracted = dependencies.to_vec();
    for column in 0..dependencies.len() {
        if owner[column] != column {
            contracted[column] = None;
        } else if let Some(edges) = &mut contracted[column] {
            let grouped = groups.iter().any(|group| owner[group.columns[0]] == column);
            *edges = (0..dependencies.len())
                .filter(|&member| owner[member] == column)
                .flat_map(|member| dependencies[member].iter().flatten().copied())
                .filter(|&dependency| !grouped || owner[dependency] != column)
                .map(|dependency| owner[dependency])
                .collect();
        }
    }
    let mut analysis = column_dependencies::analyze(&contracted, keys, has_rows)?;
    // Contraction owns scheduling, never a forward read's declaration
    // identity. A feedback group owns its internal order; retain the cycle
    // suppression of the contracted policy for the first refused cycle.
    let refused = analysis
        .diagnostics
        .iter()
        .find_map(|diagnostic| match diagnostic {
            column_dependencies::Diagnostic::Cycle { columns } => Some(columns.clone()),
            _ => None,
        })
        .unwrap_or_default();
    analysis.diagnostics.retain(|diagnostic| {
        !matches!(
            diagnostic,
            column_dependencies::Diagnostic::ForwardReference { .. }
        )
    });
    analysis.diagnostics.extend(
        without_delayed_forwards(ordinary, &reads)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| match diagnostic {
                column_dependencies::Diagnostic::ForwardReference { column, .. } => {
                    !refused.contains(column)
                        && !groups.iter().any(|group| group.columns.contains(column))
                }
                _ => false,
            }),
    );
    analysis.order = analysis
        .order
        .into_iter()
        .flat_map(|column| {
            groups
                .iter()
                .find(|group| owner[group.columns[0]] == column)
                .map_or_else(|| vec![column], |group| group.columns.clone())
        })
        .collect();
    Ok((analysis, groups))
}

fn without_delayed_forwards(
    mut analysis: column_dependencies::Analysis,
    reads: &[Reads<'_>],
) -> column_dependencies::Analysis {
    analysis.diagnostics.retain(|diagnostic| {
        let column_dependencies::Diagnostic::ForwardReference { column, dependency } = diagnostic
        else {
            return true;
        };
        reads[*column].current.contains(dependency)
            || !reads[*column]
                .delayed
                .iter()
                .any(|(source, _, _)| source == dependency)
    });
    analysis
}

fn admit(members: &[usize], reads: &[Reads<'_>], graph: &[Vec<usize>]) -> Option<Recurrence> {
    let (_, window, path) = members
        .iter()
        .flat_map(|&column| &reads[column].delayed)
        .next()?;
    if window.order_by.is_empty() {
        return None;
    }
    let same_window =
        |other: &Window| other.group_by == window.group_by && other.order_by == window.order_by;
    let mut current = vec![Vec::new(); graph.len()];
    for &column in members {
        for &(_, other, _) in &reads[column].delayed {
            if !same_window(other)
                || other
                    .group_by
                    .iter()
                    .chain(other.order_by.iter().map(|term| &term.column))
                    .any(|field| members.contains(field))
            {
                return None;
            }
        }
        current[column] = graph[column]
            .iter()
            .copied()
            .filter(|dependency| {
                members.contains(dependency)
                    && (reads[column].current.contains(dependency)
                        || !reads[column]
                            .delayed
                            .iter()
                            .any(|(source, _, _)| source == dependency))
            })
            .collect();
    }
    let analysis = dependency_analysis::analyze_admitted(current);
    if analysis.cycle.is_some() {
        return None;
    }
    Some(Recurrence {
        columns: analysis
            .order
            .into_iter()
            .filter(|column| members.contains(column))
            .collect(),
        window_path: (*path).into(),
    })
}
