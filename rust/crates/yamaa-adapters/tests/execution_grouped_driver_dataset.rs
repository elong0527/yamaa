#![cfg(feature = "yaml")]
//! A driver field read outside the groups of a grouped row template fails as
//! `ungrouped_driver_field` before any data is read: in the template under
//! REQ-0067 and at column level under REQ-0107. The finding's `dataset` names
//! the driver that the read qualifies, which is the grouped template's own
//! input or named intermediate. It need not be the input behind the first row
//! template's driver.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-grouped-driver-dataset-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("clinic.csv"), CLINIC).unwrap();
        fs::write(root.join("home.csv"), HOME).unwrap();
        fs::write(root.join("ae.csv"), AE).unwrap();
        Self(root)
    }
    /// The issues `domain` reports, whether preparation or the build fails.
    fn issues(&self) -> Vec<Value> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let issues = match file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "grouped-driver-dataset",
                specification,
                base_directory: ".",
            },
        ) {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        issues
            .iter()
            .map(|issue| {
                yamaa_adapters::json!({
                    "phase": issue.phase,
                    "condition": issue.condition,
                    "requirement": issue.requirement,
                    "spec_paths": issue.spec_paths,
                    "context": issue.context.clone()
                })
            })
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

// One clinic measurement per visit; two home readings on one day.
const CLINIC: &str = "USUBJID,VSDY,SYSBP\nS1,1,132\nS1,15,128\n";
const HOME: &str = "USUBJID,VSDY,SYSBP\nS1,8,130\nS1,8,126\n";
// The intermediate keeps S1's two serious events, which form one group.
const AE: &str = "USUBJID,AESEQ,AESER,AETERM\nS1,1,Y,HEADACHE\nS1,2,Y,DIZZINESS\nS1,3,N,NAUSEA\n";

fn ungrouped_driver_field(requirement: &str, path: &str, context: Value) -> Vec<Value> {
    vec![yamaa_adapters::json!({
        "phase": "validation",
        "condition": "ungrouped_driver_field",
        "requirement": requirement,
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
            ungrouped_driver_field("REQ-0067", path, context.clone()),
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
            "REQ-0067",
            "columns.VSSTRESN.derivation.source",
            yamaa_adapters::json!({"identifier": "HOME.SYSBP", "row": "home", "dataset": "HOME"}),
        )
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
            "REQ-0067",
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
            "REQ-0107",
            "columns.R.derivation.source",
            yamaa_adapters::json!({"identifier": "SER.AETERM", "dataset": "SER"}),
        )
    );
}
