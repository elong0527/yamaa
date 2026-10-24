//! The oracle of #3805, which goes with the checks pass. While `check()`
//! still reports the pass's findings, each specification a recording thread
//! prepares compares them, complete rows in order, with the findings binding
//! decides over the declared input schemas, and the pass's written filter
//! faults with those binding decides over the written schemas. Nothing is
//! captured: two implementations are compared in one build.
use super::*;
use std::cell::RefCell;

/// What the pass and binding report for one prepared specification.
#[derive(Clone, Debug)]
pub struct Comparison {
    /// The specification's `domain`, which names it in a report.
    pub domain: String,
    /// The pass's declaration findings, as `check()` reports them.
    pub pass: Vec<crate::diagnostic::Diagnostic>,
    /// What binding reports over the declared schemas, withheld findings
    /// left out, or the binding error that stopped it.
    pub binding: Result<Vec<crate::diagnostic::Diagnostic>, String>,
    /// The findings binding withholds over the declared schemas.
    pub withheld: Vec<crate::diagnostic::Diagnostic>,
    /// The pass's intermediate filter faults that written declarations
    /// decide, as a run whose inputs are all missing reports them.
    pub written_pass: Vec<crate::diagnostic::Diagnostic>,
    /// The same faults as binding over the written schemas decides them.
    pub written_binding: Result<Vec<crate::diagnostic::Diagnostic>, String>,
}

impl Comparison {
    /// Whether both implementations report the same rows.
    pub fn agrees(&self) -> bool {
        self.binding.as_ref() == Ok(&self.pass)
            && self.written_binding.as_ref() == Ok(&self.written_pass)
    }
}

thread_local! {
    static RECORDING: RefCell<Option<Vec<Comparison>>> = const { RefCell::new(None) };
}

/// Run `body`, recording a comparison for each specification this thread
/// prepares meanwhile.
pub fn record<T>(body: impl FnOnce() -> T) -> (T, Vec<Comparison>) {
    let outer = RECORDING.with(|recording| recording.replace(Some(Vec::new())));
    let value = body();
    let recorded = RECORDING.with(|recording| recording.replace(outer));
    (value, recorded.unwrap_or_default())
}

/// Whether this thread records comparisons.
pub(super) fn active() -> bool {
    RECORDING.with(|recording| recording.borrow().is_some())
}

impl PreparedSpecification {
    /// Record what binding decides beside the pass's findings.
    pub(super) fn compare(&self) {
        let rows = |schemas: Result<InputSchemas, BindError>| {
            let schemas = schemas.map_err(|error| format!("{error:?}"))?;
            let findings = self
                .bind_findings(&schemas)
                .map_err(|error| format!("{error:?}"))?;
            let mut kept = Vec::new();
            let mut withheld = Vec::new();
            for finding in findings {
                let (into, finding) = match finding {
                    BindFinding::Withheld(finding) => (&mut withheld, *finding),
                    finding => (&mut kept, finding),
                };
                into.extend(
                    finding
                        .diagnostics(self.source())
                        .ok_or_else(|| format!("no diagnostics: {finding:?}"))?,
                );
            }
            Ok((kept, withheld))
        };
        let declared = rows(InputSchemas::declared(self));
        let written_binding = rows(InputSchemas::written(self)).map(|(kept, _)| {
            kept.into_iter()
                .filter(|finding| self.intermediates.owns_filter_type_finding(finding))
                .collect()
        });
        let comparison = Comparison {
            domain: self.output_name().into(),
            pass: self.check_expressions.clone(),
            withheld: declared
                .as_ref()
                .map(|(_, withheld)| withheld.clone())
                .unwrap_or_default(),
            binding: declared.map(|(kept, _)| kept),
            written_pass: self
                .written_filter_types
                .iter()
                .filter(|finding| self.intermediates.owns_filter_type_finding(finding))
                .cloned()
                .collect(),
            written_binding,
        };
        RECORDING.with(|recording| {
            if let Some(recorded) = recording.borrow_mut().as_mut() {
                recorded.push(comparison);
            }
        });
    }
}
