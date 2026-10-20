//! Column-phase dependency rules over already-bound declaration indices.
use std::vec;

use crate::dependency_analysis;

/// Invalid compiler metadata, separate from language diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("column dependencies graph: {0:?}")]
    Graph(dependency_analysis::Error),
    #[error("column dependencies invalid key: key={key:?}")]
    InvalidKey { key: usize },
    #[error("column dependencies duplicate key: key={key:?}")]
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
        self.diagnostic_code().definition().condition
    }

    /// Retain the language rule that governs each dependency.
    pub fn rule(&self) -> &'static str {
        self.diagnostic_code()
            .definition()
            .rule
            .expect("dependency cause has a normative rule")
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
/// the declaration-order and key-dependency rules. A later key is available
/// before the column phase only when it is derived first: always without rows
/// (COLUMN-7), and with rows only when no row template leaves it to the column
/// phase. This does not waive preflight, type, reference-resolution, or
/// phase-boundary checks performed by the caller.
pub fn analyze(
    dependencies: &[Option<Vec<usize>>],
    keys: &[usize],
    has_rows: bool,
) -> Result<Analysis, Error> {
    dependency_analysis::validate_indices(
        dependencies.len(),
        dependencies
            .iter()
            .map(|edges| edges.as_deref().unwrap_or(&[])),
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
    // Without rows every key is derived before any other column. With rows, a key
    // that some template completes in the column phase follows declaration order
    // like any other column-phase column (COLUMN-5).
    let derived_first =
        |dependency: usize| is_key[dependency] && (!has_rows || dependencies[dependency].is_none());
    for (column, edges) in dependencies.iter().enumerate() {
        if in_cycle[column] {
            continue;
        }
        for &dependency in edges.as_deref().unwrap_or(&[]) {
            if !derived_first(dependency) && dependency >= column {
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
