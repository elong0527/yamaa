//! Versioned public boundary records for diagnostics, shared by both hosts.
//!
//! The core registry (`yamaa_core::diagnostic`) owns cause identity: every
//! `ConditionCode` resolves to one canonical phase, condition and requirement.
//! This module owns the boundary built on that registry: the version both
//! hosts read, the severity vocabulary, and the named record types hosts will
//! consume. It only represents findings; it does not select a phase, bind a
//! name or evaluate anything. The [`crate::issue_rows::Issue`] projection
//! migrated onto these records in #2675: it carries the same structured
//! context and geometry, encoded with geometry keys present only when
//! recorded, so diagnostics without geometry keep their previous bytes.
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
//! # Severity vocabulary
//!
//! The vocabulary is closed: `error` and `warning`. Every condition in the
//! current core registry is an `error` — it blocks the result it belongs to.
//! `warning` is reserved for future non-blocking findings; no current
//! condition maps to it, and assigning one is a registry decision recorded
//! with the condition, not a host choice.
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

use yamaa_core::diagnostic::{ConditionCode, Context, Definition, Diagnostic};
use yamaa_core::numeric_parser::SourceSpan;

/// The boundary version both hosts read, covering the diagnostic, result and
/// plan records. See the module documentation for the extension rules that
/// govern when this moves.
pub const BOUNDARY_VERSION: u32 = 1;

/// Boundary severity vocabulary; see the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The finding blocks the result it belongs to.
    Error,
    /// The finding is reported without blocking its result. Reserved: no
    /// condition in the current core registry carries it.
    Warning,
}

impl Severity {
    /// Canonical spelling of this severity at the boundary.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }

    /// The boundary severity of one registered cause. Every condition in the
    /// current core registry blocks its result, so every code maps to
    /// `Error`; `Warning` assignments arrive with future non-blocking
    /// conditions.
    pub fn of(_code: ConditionCode) -> Self {
        Self::Error
    }
}

/// One diagnostic at the host boundary: the registered cause, its boundary
/// severity, and the owned facts recorded at the failure site.
///
/// Phase, condition and requirement are not stored fields: they are pure
/// functions of `code` in the core registry, exposed through
/// [`Self::definition`] and its accessors so the registry stays the single
/// source for them.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticRecord {
    /// Stable registered cause identity.
    pub code: ConditionCode,
    /// Boundary severity assigned to `code` (see [`Severity::of`]).
    pub severity: Severity,
    /// Specification paths the finding belongs to, in authored order.
    pub spec_paths: Vec<String>,
    /// Owned structured context from the failure site.
    pub context: Context,
    /// Half-open byte span in the original expression, when geometry exists.
    pub source_span: Option<SourceSpan>,
    /// Operand route in recorded order, when one was recorded.
    pub operand_route: Option<Vec<String>>,
}

impl DiagnosticRecord {
    /// Carry one core diagnostic across the boundary unchanged: span and
    /// route are retained (the legacy issue projection discards them), the
    /// context moves as the owned tree, and severity is assigned from the
    /// registered cause.
    pub fn from_core(diagnostic: Diagnostic) -> Self {
        let Diagnostic {
            code,
            spec_paths,
            context,
            source_span,
            operand_route,
        } = diagnostic;
        Self {
            severity: Severity::of(code),
            code,
            spec_paths,
            context,
            source_span,
            operand_route,
        }
    }

    /// The canonical public mapping of this record's cause.
    pub fn definition(&self) -> Definition {
        self.code.definition()
    }

    /// Lifecycle phase of the finding, from the core registry.
    pub fn phase(&self) -> &'static str {
        self.definition().phase
    }

    /// Public condition spelling, from the core registry.
    pub fn condition(&self) -> &'static str {
        self.definition().condition
    }

    /// Owning normative requirement, when the cause cites one.
    pub fn requirement(&self) -> Option<&'static str> {
        self.definition().requirement
    }
}

/// The named result boundary record: the diagnostics one operation produced,
/// in authored order, under the boundary version they were recorded with.
/// Result payloads themselves (tables, artifacts) stay with their existing
/// owners; this record is the diagnostic half of a boundary result.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultRecord {
    /// The boundary version these diagnostics were recorded under.
    pub boundary_version: u32,
    /// The operation's diagnostics, in authored order.
    pub diagnostics: Vec<DiagnosticRecord>,
}

impl ResultRecord {
    /// Record diagnostics under the current boundary version.
    pub fn new(diagnostics: Vec<DiagnosticRecord>) -> Self {
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
    use std::collections::BTreeSet;
    use yamaa_core::diagnostic::CONDITIONS;
    use yamaa_core::json::Value;

    fn context() -> Context {
        let mut context = Context::new();
        context.insert(
            "values".into(),
            Value::from(vec![
                Value::from("text"),
                Value::from(i64::MIN),
                Value::from(true),
                Value::Null,
            ]),
        );
        context.insert("nested".into(), Value::from(Context::new()));
        context
    }

    fn diagnostic(code: ConditionCode) -> Diagnostic {
        Diagnostic {
            code,
            spec_paths: vec!["columns.ZLAST".into(), "columns.AFIRST".into()],
            context: context(),
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
    fn severity_vocabulary_is_error_and_warning() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        for code in CONDITIONS {
            assert_eq!(Severity::of(*code), Severity::Error, "{code:?}");
        }
    }

    #[test]
    fn every_registered_condition_is_representable() {
        let records: Vec<DiagnosticRecord> = CONDITIONS
            .iter()
            .map(|code| DiagnosticRecord::from_core(diagnostic(*code)))
            .collect();
        assert_eq!(records.len(), CONDITIONS.len());
        let codes: BTreeSet<ConditionCode> = records.iter().map(|record| record.code).collect();
        assert_eq!(codes.len(), CONDITIONS.len());
        for (record, code) in records.iter().zip(CONDITIONS.iter()) {
            assert_eq!(record.code, *code);
            assert_eq!(record.severity, Severity::Error);
            assert_eq!(record.definition(), code.definition());
            assert_eq!(record.phase(), code.definition().phase);
            assert_eq!(record.condition(), code.definition().condition);
            assert_eq!(record.requirement(), code.definition().requirement);
            // Authored order and owned facts cross the boundary unchanged.
            assert_eq!(record.spec_paths, ["columns.ZLAST", "columns.AFIRST"]);
            assert_eq!(record.context, context());
            assert_eq!(record.source_span, Some(SourceSpan { start: 3, end: 9 }));
            assert_eq!(
                record.operand_route.as_deref(),
                Some(&["operand".to_owned(), "root".to_owned()][..])
            );
        }
    }

    #[test]
    fn absent_geometry_differs_from_zero_geometry() {
        let code = CONDITIONS[0];
        let absent = DiagnosticRecord::from_core(Diagnostic {
            source_span: None,
            operand_route: None,
            ..diagnostic(code)
        });
        let zero = DiagnosticRecord::from_core(Diagnostic {
            source_span: Some(SourceSpan { start: 0, end: 0 }),
            operand_route: Some(Vec::new()),
            ..diagnostic(code)
        });
        assert_eq!(absent.source_span, None);
        assert_eq!(absent.operand_route, None);
        assert_eq!(zero.source_span, Some(SourceSpan { start: 0, end: 0 }));
        assert_eq!(zero.operand_route, Some(Vec::new()));
        assert_ne!(absent, zero);
    }

    #[test]
    fn result_record_preserves_authored_order() {
        let records: Vec<DiagnosticRecord> = CONDITIONS[..2]
            .iter()
            .map(|code| DiagnosticRecord::from_core(diagnostic(*code)))
            .collect();
        let result = ResultRecord::new(records.clone());
        assert_eq!(result.diagnostics, records);
        assert_eq!(result.diagnostics[0].code, CONDITIONS[0]);
        assert_eq!(result.diagnostics[1].code, CONDITIONS[1]);
    }
}
