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
    // A lookup key's str_pad width is the authored literal 0 (TEXT-2).
    // Binding decides it independently of the source's type or presence;
    // the old declaration pass leaves that refusal to run().
    Difference {
        case: "intermediates.json::a_padded_key_reports_its_width_then_its_unknown_source_then_the_read_that_depends_on_it",
        domain: "out",
        class: Class::Q3,
    },
    Difference {
        case: "intermediates.json::a_padded_key_of_an_int_donor_column_reports_its_authored_type_first",
        domain: "out",
        class: Class::Q3,
    },
    Difference {
        case: "intermediates.json::a_padded_key_of_a_column_its_donor_lacks_reports_the_donor_column_first",
        domain: "out",
        class: Class::Q3,
    },
    Difference {
        case: "intermediates.json::a_padded_key_of_a_known_source_reports_only_its_width",
        domain: "out",
        class: Class::Q3,
    },
    // A project function's integer parameter reads an undeclared CSV
    // field, which INPUT-5 types as `str`. Binding decides FUNC-5 while
    // the old pass leaves it to run(). The second case also reads a
    // relation the specification never declares (NAME-3), which binding
    // decides beside the type fault. Recorded on #3805 from #3920.
    Difference {
        case: "projects.json::a_stored_field_of_another_type_than_its_parameter_is_refused_at_binding",
        domain: "output",
        class: Class::Q3,
    },
    Difference {
        case: "projects.json::missing_stored_fields_and_unknown_relations_keep_authored_argument_diagnostics",
        domain: "output",
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
