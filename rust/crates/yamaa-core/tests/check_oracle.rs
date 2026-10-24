//! The oracle of #3805 over this crate's cases: the seam between the checks
//! pass, which still reports `check()`, and binding over the declared input
//! schemas, which will. Each case of `tests/fixtures/diagnostics/` makes
//! its calls while the engine records, for each specification it prepares,
//! both implementations' complete rows; they must agree but for the
//! differences #3805 records. The oracle goes with the pass.
use crate::support::diagnostic::{operations, replay};
use yamaa_core::specification::check_oracle::{self, Comparison};

/// The rows of `diagnostics`, one line each.
fn rows(diagnostics: &[yamaa_core::diagnostic::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("    {:?}\n", replay::row(diagnostic)))
        .collect()
}

/// One comparison that disagrees, as the report names it.
pub fn report(label: &str, comparison: &Comparison) -> String {
    let side = |rows_or_error: &Result<Vec<_>, String>| match rows_or_error {
        Ok(found) => rows(found),
        Err(error) => format!("    error: {error}\n"),
    };
    format!(
        "{label} ({}):\n  pass:\n{}  binding:\n{}  withheld:\n{}  written pass:\n{}  written binding:\n{}",
        comparison.domain,
        rows(&comparison.pass),
        side(&comparison.binding),
        rows(&comparison.withheld),
        rows(&comparison.written_pass),
        side(&comparison.written_binding),
    )
}

#[test]
fn binding_over_the_declared_inputs_reports_what_the_pass_reports() {
    let mut disagreements = Vec::new();
    let mut compared = 0;
    for case in replay::cases() {
        for operation in &case.operations {
            let (_, comparisons) =
                check_oracle::record(|| operations::run(operation, &case.input));
            for comparison in comparisons {
                compared += 1;
                if !comparison.agrees() {
                    disagreements.push(report(&case.label, &comparison));
                }
            }
        }
    }
    assert!(compared > 0, "no case prepared a specification");
    assert!(
        disagreements.is_empty(),
        "{} of {compared} comparisons disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}
