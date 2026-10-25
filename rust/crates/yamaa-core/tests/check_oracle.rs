//! The oracle of #3805 over this crate's cases: the seam between the checks
//! pass, which still reports `check()`, and binding over the declared input
//! schemas, which will. Each case of `tests/fixtures/diagnostics/` makes
//! its calls while the engine records, for each specification it prepares,
//! both implementations' complete rows; they must agree but for the
//! differences #3805 records, each listed below with its class. The oracle
//! goes with the pass.
use crate::support::diagnostic::{operations, replay};
use yamaa_core::specification::check_oracle::{self, Class, Difference};

/// The differences of this crate's cases, as #3805 records them.
const DIFFERENCES: &[Difference] = &[
    // A template a foreign input drives, whose column defaults read the key
    // that input may not store: the pass reports a cycle through the column
    // (COLUMN-6), which neither binding nor `run()` reports.
    Difference {
        case: "intermediates.json::a_template_a_foreign_input_drives_needs_the_raw_key_its_column_defaults_read",
        domain: "out",
        class: Class::Q2,
    },
    // A row template's intermediate driver whose filter reads another input
    // (ROW-2), and the read of its field that follows, which `run()` alone
    // reported.
    Difference {
        case: "records.json::a_row_template_driver_filters_on_its_own_input_alone",
        domain: "result",
        class: Class::Q3,
    },
];

#[test]
fn binding_over_the_declared_inputs_reports_what_the_pass_reports() {
    let mut recorded = Vec::new();
    for case in replay::cases() {
        for operation in &case.operations {
            let (_, comparisons) = check_oracle::record(|| operations::run(operation, &case.input));
            recorded.extend(
                comparisons
                    .into_iter()
                    .map(|comparison| (case.label.clone(), comparison)),
            );
        }
    }
    if let Err(failure) = check_oracle::judge(&recorded, DIFFERENCES) {
        panic!("{failure}");
    }
}
