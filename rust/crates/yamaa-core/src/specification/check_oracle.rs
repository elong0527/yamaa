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

/// The complete row a host reports of `diagnostic`: its phase, condition,
/// rule, specification paths, context, source span and operand route. Its
/// message is its condition's template over the context, and its location
/// follows its first path, so two causes that render one row are one row.
type Row<'d> = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'d [String],
    &'d crate::diagnostic::Context,
    &'d Option<crate::expression::SourceSpan>,
    &'d Option<Vec<String>>,
);

fn row(diagnostic: &crate::diagnostic::Diagnostic) -> Row<'_> {
    (
        diagnostic.phase(),
        diagnostic.condition(),
        diagnostic.rule(),
        &diagnostic.spec_paths,
        &diagnostic.context,
        &diagnostic.source_span,
        &diagnostic.operand_route,
    )
}

/// Whether binding's rows, or the error that stopped it, are the pass's.
fn same(
    binding: &Result<Vec<crate::diagnostic::Diagnostic>, String>,
    pass: &[crate::diagnostic::Diagnostic],
) -> bool {
    binding
        .as_ref()
        .is_ok_and(|binding| binding.iter().map(row).eq(pass.iter().map(row)))
}

impl Comparison {
    /// Whether both implementations report the same rows.
    pub fn agrees(&self) -> bool {
        same(&self.binding, &self.pass) && same(&self.written_binding, &self.written_pass)
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

/// The classes of #3805's answers into which each difference falls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Class {
    /// A schema-linked input is closed, so binding reports a field it does
    /// not store, which the pass left open.
    Q1,
    /// The pass reports a finding binding withholds or never makes.
    Q2,
    /// Binding reports a finding it decides that the pass left to `run()`.
    Q3,
}

/// One difference #3805 records: the case that makes it, the domain of the
/// specification that differs, and its class.
#[derive(Clone, Copy, Debug)]
pub struct Difference {
    pub case: &'static str,
    pub domain: &'static str,
    pub class: Class,
}

/// The complete rows of `diagnostics`, one line each.
fn rows(diagnostics: &[crate::diagnostic::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "    {} {} {} {:?} {:?} span={:?} route={:?} ({:?})\n",
                diagnostic.phase(),
                diagnostic.condition(),
                diagnostic.rule().unwrap_or("-"),
                diagnostic.spec_paths,
                diagnostic.context,
                diagnostic.source_span,
                diagnostic.operand_route,
                diagnostic.code,
            )
        })
        .collect()
}

impl Comparison {
    /// Both sides of the comparison, as a failure prints them.
    pub fn report(&self, case: &str) -> String {
        let side = |found: &Result<Vec<_>, String>| match found {
            Ok(found) => rows(found),
            Err(error) => format!("    error: {error}\n"),
        };
        format!(
            "{case} ({}):\n  pass:\n{}  binding:\n{}  withheld:\n{}  written pass:\n{}  \
             written binding:\n{}",
            self.domain,
            rows(&self.pass),
            side(&self.binding),
            rows(&self.withheld),
            rows(&self.written_pass),
            side(&self.written_binding),
        )
    }
}

/// Judge the comparisons each case recorded against the differences #3805
/// records: every comparison agrees but those listed, and each listed one
/// differs. The error reports what does not hold; `Ok` counts the
/// comparisons.
pub fn judge(
    recorded: &[(String, Comparison)],
    differences: &[Difference],
) -> Result<usize, String> {
    let listed = |case: &str, domain: &str| {
        differences
            .iter()
            .any(|difference| difference.case == case && difference.domain == domain)
    };
    let mut failures = Vec::new();
    for (case, comparison) in recorded {
        if !comparison.agrees() && !listed(case, &comparison.domain) {
            failures.push(comparison.report(case));
        }
    }
    for difference in differences {
        let differs = recorded.iter().any(|(case, comparison)| {
            case == difference.case
                && comparison.domain == difference.domain
                && !comparison.agrees()
        });
        if !differs {
            failures.push(format!(
                "{} ({}): listed as a {:?} difference, but it agrees or was not compared\n",
                difference.case, difference.domain, difference.class
            ));
        }
    }
    match (recorded.len(), failures.is_empty()) {
        (0, _) => Err("no case prepared a specification".into()),
        (compared, true) => Ok(compared),
        (compared, false) => Err(format!(
            "{} of {compared} comparisons do not hold:\n{}",
            failures.len(),
            failures.join("\n")
        )),
    }
}
