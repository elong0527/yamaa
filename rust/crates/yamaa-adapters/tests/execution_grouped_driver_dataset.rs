#![cfg(feature = "yaml")]
//! A driver field read outside the groups of a grouped row template fails as
//! `ungrouped_driver_field` before any data is read: in the template under
//! ROW-5 and at column level under NAME-4. The finding's `dataset` names
//! the driver that the read qualifies, which is the grouped template's own
//! input or named intermediate. It need not be the input behind the first row
//! template's driver. At column level only the grouped templates over the
//! read's own driver decide: a template grouped over another driver leaves
//! the read scalar. A column-level window instead reads only a driver that
//! every row template shares, grouped or not (WINDOW-11).
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("grouped-driver-dataset");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("clinic.csv"), CLINIC).unwrap();
        fs::write(root.join("home.csv"), HOME).unwrap();
        fs::write(root.join("ae.csv"), AE).unwrap();
        Self(root)
    }
    fn domain(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("grouped-driver-dataset", specification),
        )
    }
    /// `domain` reports nothing and saves exactly `expected` to `output`.
    fn saved(&self, output: &str, expected: &str) {
        let mut domain = self.domain().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        assert_eq!(fs::read_to_string(self.0.join(output)).unwrap(), expected);
    }
    /// The issues `domain` reports, whether preparation or the build fails.
    fn issues(&self) -> Vec<Value> {
        let issues = match self.domain() {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        issues
            .iter()
            .map(crate::support::study::issue_row)
            .collect()
    }
}

// One clinic measurement per visit; two home readings on one day.
const CLINIC: &str = "USUBJID,VSDY,SYSBP\nS1,1,132\nS1,15,128\n";
const HOME: &str = "USUBJID,VSDY,SYSBP\nS1,8,130\nS1,8,126\n";
// The intermediate keeps S1's two serious events, which form one group.
const AE: &str = "USUBJID,AESEQ,AESER,AETERM\nS1,1,Y,HEADACHE\nS1,2,Y,DIZZINESS\nS1,3,N,NAUSEA\n";

fn ungrouped_driver_field(rule: &str, path: &str, context: Value) -> Vec<Value> {
    vec![yamaa_adapters::json!({
        "phase": "validation",
        "condition": "ungrouped_driver_field",
        "rule": rule,
        "spec_paths": [path],
        "context": context
    })]
}

/// The clinic template comes first, so the specification's driver is CLINIC;
/// the second template groups the home readings by subject and day.
fn vitals(clinic: &str, home: &str, column: &str) -> String {
    format!(
        "schema_version: \"1.0\"\n\
         domain: VS\n\
         keys: [USUBJID, VSDY, VSCAT]\n\
         output: {{path: vs.csv, columns: [USUBJID, VSDY, VSCAT, VSSTRESN]}}\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{VSDY: int, SYSBP: int}}}}\n\
         \x20 HOME: {{path: home.csv, types: {{VSDY: int, SYSBP: int}}}}\n\
         rows:\n\
         \x20 - id: clinic\n\
         \x20   dataset: CLINIC\n\
         \x20   derivations: {{USUBJID: CLINIC.USUBJID, VSDY: CLINIC.VSDY, VSCAT: {{literal: CLINIC}}{clinic}}}\n\
         \x20 - id: home\n\
         \x20   dataset: HOME\n\
         \x20   group_by: [HOME.USUBJID, HOME.VSDY]\n\
         \x20   derivations: {{USUBJID: HOME.USUBJID, VSDY: HOME.VSDY, VSCAT: {{literal: HOME}}{home}}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: VSDY, type: int}}\n\
         \x20 - {{name: VSCAT, type: str}}\n\
         \x20 - {{name: VSSTRESN, type: float{column}}}\n"
    )
}

#[test]
fn later_grouped_template_names_its_own_input() {
    let context =
        yamaa_adapters::json!({"identifier": "HOME.SYSBP", "row": "home", "dataset": "HOME"});
    for (home, path) in [
        (
            ", VSSTRESN: HOME.SYSBP",
            "rows[1].derivations.VSSTRESN.source",
        ),
        (
            ", VSSTRESN: {row_number: {window: {order_by: [HOME.SYSBP]}}}",
            "rows[1].derivations.VSSTRESN.row_number.window.order_by[0]",
        ),
    ] {
        let fixture = Fixture::new(&vitals(", VSSTRESN: CLINIC.SYSBP", home, ""));
        assert_eq!(
            fixture.issues(),
            ungrouped_driver_field("ROW-5", path, context.clone()),
            "{home}"
        );
    }
}

#[test]
fn column_derivation_completed_by_a_later_template_names_its_input() {
    // The clinic template derives VSSTRESN itself, so only the home template
    // completes it from the column-level derivation.
    let fixture = Fixture::new(&vitals(
        ", VSSTRESN: CLINIC.SYSBP",
        "",
        ", derivation: HOME.SYSBP",
    ));
    assert_eq!(
        fixture.issues(),
        ungrouped_driver_field(
            "ROW-5",
            "columns.VSSTRESN.derivation.source",
            yamaa_adapters::json!({"identifier": "HOME.SYSBP", "row": "home", "dataset": "HOME"}),
        )
    );
}

#[test]
fn column_read_of_an_ungrouped_driver_ignores_another_drivers_groups() {
    // The home template averages its readings itself, so only the clinic
    // template completes VSSTRESN from the column-level derivation. No
    // template groups the clinic measurements, so each clinic row reads its
    // own measurement.
    let fixture = Fixture::new(&vitals(
        "",
        ", VSSTRESN: {aggregate: 'MEAN(HOME.SYSBP)'}",
        ", derivation: CLINIC.SYSBP",
    ));
    fixture.saved(
        "vs.csv",
        "USUBJID,VSDY,VSCAT,VSSTRESN\nS1,1,CLINIC,132\nS1,15,CLINIC,128\nS1,8,HOME,128\n",
    );
}

#[test]
fn column_window_over_a_template_grouped_by_another_driver_names_the_drivers() {
    // A column-level window also orders the rows the home template groups,
    // which no clinic measurement built. Grouping is not the cause: the home
    // rows hold no clinic record at all, so the read fails as a field that
    // names nothing they hold, naming both drivers.
    let specification = vitals(
        "",
        "",
        ", derivation: {row_number: {window: {order_by: [CLINIC.SYSBP]}}}",
    )
    .edit("type: float", "type: int");
    assert_eq!(
        Fixture::new(&specification).issues(),
        vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "rule": "WINDOW-11",
            "spec_paths": ["columns.VSSTRESN.derivation.row_number.window.order_by[0]"],
            "context": {
                "identifier": "CLINIC.SYSBP",
                "drivers": ["CLINIC", "HOME"],
                "operation": "row_number"
            }
        })]
    );
}

/// A template grouped by subject over the serious events, which a named
/// intermediate selects from AE.
fn serious(derivations: &str, column: &str) -> String {
    format!(
        "schema_version: \"1.0\"\n\
         domain: XX\n\
         keys: [USUBJID]\n\
         output: {{path: out.csv, columns: [USUBJID, R]}}\n\
         input:\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int}}}}\n\
         intermediates:\n\
         \x20 - {{id: SER, dataset: AE, filter: \"AE.AESER = 'Y'\"}}\n\
         rows:\n\
         \x20 - id: subjects\n\
         \x20   dataset: SER\n\
         \x20   group_by: [SER.USUBJID]\n\
         \x20   derivations: {{USUBJID: SER.USUBJID{derivations}}}\n\
         columns:\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: R, type: str{column}}}\n"
    )
}

#[test]
fn grouped_intermediate_driver_is_named_rather_than_its_input() {
    assert_eq!(
        Fixture::new(&serious(", R: SER.AETERM", "")).issues(),
        ungrouped_driver_field(
            "ROW-5",
            "rows[0].derivations.R.source",
            yamaa_adapters::json!({"identifier": "SER.AETERM", "row": "subjects", "dataset": "SER"}),
        )
    );
}

#[test]
fn column_read_of_a_grouped_intermediate_driver_names_the_intermediate() {
    assert_eq!(
        Fixture::new(&serious("", ", derivation: SER.AETERM")).issues(),
        ungrouped_driver_field(
            "NAME-4",
            "columns.R.derivation.source",
            yamaa_adapters::json!({"identifier": "SER.AETERM", "dataset": "SER"}),
        )
    );
}
