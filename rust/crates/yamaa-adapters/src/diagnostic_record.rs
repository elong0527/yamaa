//! Versioned public boundary records, shared by both hosts.
//!
//! The diagnostic record is core's [`Diagnostic`]: one type carries a finding
//! from its failure site to the edge that renders it, with its registered
//! cause, specification paths, structured context and optional geometry.
//! This module owns the boundary built on it: the version both hosts read
//! and the named result and plan records. It only represents findings; it
//! does not select a phase, bind a name or evaluate anything.
//!
//! # Boundary version
//!
//! [`BOUNDARY_VERSION`] is the single source both hosts read; it starts at 1.
//! One version covers the diagnostic record, the result record and the plan
//! record together: an incompatible change to any of them moves the shared
//! version (see the extension rules below).
//!
//! # Ordering
//!
//! Diagnostic order is authored order. A producer emits findings in stage
//! order and every boundary collection preserves that order; records are
//! never sorted, grouped or deduplicated at the boundary. Within one record,
//! `spec_paths` keep their authored order and `operand_route` keeps the order
//! the core diagnostic recorded.
//!
//! # Severity
//!
//! The vocabulary is closed: `error` and `warning`. Every condition in the
//! current core registry is an `error` — it blocks the result it belongs to.
//! `warning` is reserved for future non-blocking findings; assigning one is a
//! registry decision recorded with the condition, not a host choice.
//!
//! # Geometry: absent is not zero
//!
//! `source_span` and `operand_route` are optional because geometry is not
//! always available. Absent (`None`) means unavailable, never a default.
//! A present span with `start == end == 0` is a real zero-width span at the
//! first byte of the original expression, and a present empty route is a
//! recorded route with no steps; both differ from absent, and hosts must not
//! collapse them.
//!
//! # Context ownership
//!
//! A record owns its context outright. The context is the core's owned JSON
//! tree, transferred with the record: producers do not retain or mutate it
//! after hand-over, and hosts receive values, not JSON text. Numbers keep
//! their arbitrary width end to end; nothing narrows them to binary64 at
//! this boundary.
//!
//! # Extension rules
//!
//! Compatible within one boundary version: adding a condition to the core
//! registry, assigning a severity to such a new condition, and adding a new
//! optional field to a record. Each is additive for a reader that ignores
//! what it does not know.
//!
//! Incompatible — these bump [`BOUNDARY_VERSION`]: renaming or removing a
//! record field, changing a field's type or meaning, reinterpreting absent
//! geometry as zero, reassigning an existing condition's severity, changing
//! the severity vocabulary, or weakening the ordering guarantees above.

use yamaa_core::diagnostic::Diagnostic;

/// The boundary version both hosts read, covering the diagnostic, result and
/// plan records. See the module documentation for the extension rules that
/// govern when this moves.
pub const BOUNDARY_VERSION: u32 = 1;

/// The named result boundary record: the diagnostics one operation produced,
/// in authored order, under the boundary version they were recorded with.
/// Result payloads themselves (tables, artifacts) stay with their existing
/// owners; this record is the diagnostic half of a boundary result.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultRecord {
    /// The boundary version these diagnostics were recorded under.
    pub boundary_version: u32,
    /// The operation's diagnostics, in authored order.
    pub diagnostics: Vec<Diagnostic>,
}

impl ResultRecord {
    /// Record diagnostics under the current boundary version.
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            boundary_version: BOUNDARY_VERSION,
            diagnostics,
        }
    }
}

/// The named plan boundary record. This fixes the plan record's name and its
/// boundary version only: the reusable owned plan handle itself is the scope
/// of #2592/#2654, and no handle semantics are defined here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanRecord {
    /// The boundary version this plan record is defined under.
    pub boundary_version: u32,
}

impl PlanRecord {
    /// The plan record under the current boundary version.
    pub const fn current() -> Self {
        Self {
            boundary_version: BOUNDARY_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::diagnostic::CONDITIONS;
    use yamaa_core::expression::SourceSpan;

    fn diagnostic(index: usize) -> Diagnostic {
        Diagnostic {
            code: CONDITIONS[index],
            spec_paths: vec!["columns.ZLAST".into(), "columns.AFIRST".into()],
            context: Default::default(),
            source_span: Some(SourceSpan { start: 3, end: 9 }),
            operand_route: Some(vec!["operand".into(), "root".into()]),
        }
    }

    #[test]
    fn boundary_version_starts_at_one_and_versions_every_record() {
        assert_eq!(BOUNDARY_VERSION, 1);
        assert_eq!(PlanRecord::current().boundary_version, BOUNDARY_VERSION);
        assert_eq!(
            ResultRecord::new(Vec::new()).boundary_version,
            BOUNDARY_VERSION
        );
    }

    #[test]
    fn result_record_preserves_authored_order() {
        let records = vec![diagnostic(1), diagnostic(0)];
        let result = ResultRecord::new(records.clone());
        assert_eq!(result.diagnostics, records);
        assert_eq!(result.diagnostics[0].code, CONDITIONS[1]);
        assert_eq!(result.diagnostics[1].code, CONDITIONS[0]);
    }
}
