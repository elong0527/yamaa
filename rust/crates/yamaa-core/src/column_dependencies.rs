//! Column-phase dependency rules over already-bound declaration indices.
use alloc::{vec, vec::Vec};

use crate::dependency_analysis::{self, Limits};

/// Invalid compiler metadata or resource policy, separate from language diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Graph(dependency_analysis::Error),
    InvalidKey { key: usize },
    DuplicateKey { key: usize },
}

/// Portable conditions in their existing validation order; hosts attach authored paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    Cycle { columns: Vec<usize> },
    ForwardReference { column: usize, dependency: usize },
    MissingKeyDerivation { column: usize },
    KeyDependency { column: usize, dependency: usize },
}

impl Diagnostic {
    /// Name the language condition independently of host diagnostic representations.
    pub fn condition(&self) -> &'static str {
        match self {
            Self::Cycle { .. } => "dependency_cycle",
            Self::ForwardReference { .. } => "forward_reference",
            Self::MissingKeyDerivation { .. } | Self::KeyDependency { .. } => "key_dependency",
        }
    }

    /// Retain the governing language requirement for each dependency rule.
    pub fn requirement(&self) -> &'static str {
        match self {
            Self::Cycle { .. } => "REQ-0072",
            Self::ForwardReference { .. } => "REQ-0071",
            Self::MissingKeyDerivation { .. } | Self::KeyDependency { .. } => "REQ-0074",
        }
    }

    /// Select the authored path kind without constructing host-specific path strings.
    pub fn location(&self) -> &'static str {
        match self {
            Self::Cycle { .. } | Self::ForwardReference { .. } => "operation",
            Self::MissingKeyDerivation { .. } => "declaration",
            Self::KeyDependency { .. } => "expression",
        }
    }

    /// Ordered declaration indices for naming the condition and selecting its paths.
    pub fn columns(&self) -> Vec<usize> {
        match self {
            Self::Cycle { columns } => columns.clone(),
            Self::ForwardReference { column, dependency }
            | Self::KeyDependency { column, dependency } => vec![*column, *dependency],
            Self::MissingKeyDerivation { column } => vec![*column],
        }
    }
}

/// A maximal column schedule plus ordered language failures; failures prevent execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Analysis {
    pub order: Vec<usize>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Validate one column phase after name binding and phase selection.
///
/// Entries follow the full output declaration order. `None` means the column has
/// no column-phase derivation; `Some([])` is a derivation with no output-column
/// dependencies. Source references are omitted by the binder. Output references
/// retain written first-occurrence order. Keys retain their authored key order.
/// Completed row-phase columns do not block scheduling, but still participate in
/// the declaration-order and key-dependency rules. This does not waive preflight,
/// type, reference-resolution, or phase-boundary checks performed by the caller.
pub fn analyze(
    dependencies: &[Option<Vec<usize>>],
    keys: &[usize],
    has_rows: bool,
    limits: Limits,
) -> Result<Analysis, Error> {
    dependency_analysis::admit(
        dependencies.len(),
        dependencies
            .iter()
            .map(|edges| edges.as_deref().unwrap_or(&[])),
        limits,
    )
    .map_err(Error::Graph)?;
    let mut is_key = vec![false; dependencies.len()];
    for &key in keys {
        let Some(marked) = is_key.get_mut(key) else {
            return Err(Error::InvalidKey { key });
        };
        if *marked {
            return Err(Error::DuplicateKey { key });
        }
        *marked = true;
    }
    // A completed phase is already available, not another ready node that could
    // delay a reader behind an unrelated column. Remove those edges first.
    let graph = dependencies
        .iter()
        .map(|edges| {
            edges
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .copied()
                .filter(|&dependency| dependencies[dependency].is_some())
                .collect()
        })
        .collect();
    let analyzed = dependency_analysis::analyze_admitted(graph);
    let mut diagnostics = Vec::new();
    let mut in_cycle = vec![false; dependencies.len()];
    if let Some(columns) = analyzed.cycle {
        for &column in &columns {
            in_cycle[column] = true;
        }
        diagnostics.push(Diagnostic::Cycle { columns });
    }
    for (column, edges) in dependencies.iter().enumerate() {
        if in_cycle[column] {
            continue;
        }
        for &dependency in edges.as_deref().unwrap_or(&[]) {
            if !is_key[dependency] && dependency >= column {
                diagnostics.push(Diagnostic::ForwardReference { column, dependency });
            }
        }
    }
    if !has_rows {
        for &column in keys {
            if let Some(edges) = &dependencies[column] {
                for &dependency in edges {
                    if !is_key[dependency] {
                        diagnostics.push(Diagnostic::KeyDependency { column, dependency });
                    }
                }
            } else {
                diagnostics.push(Diagnostic::MissingKeyDerivation { column });
            }
        }
    }
    Ok(Analysis {
        order: analyzed
            .order
            .into_iter()
            .filter(|&n| dependencies[n].is_some())
            .collect(),
        diagnostics,
    })
}
