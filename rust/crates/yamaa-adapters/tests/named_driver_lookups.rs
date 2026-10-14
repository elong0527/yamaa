#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new(spec: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-named-driver-lookups-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("spec.yaml"), spec).unwrap();
        fs::write(path.join("pp.csv"),"USUBJID,PPTESTCD,VISITDY,PPSTRESN\nS1,CMAX,1,100\nS1,CMAX,14,150\nS1,TMAX,14,2\nS2,CMAX,1,80\nS2,CMAX,14,120\n").unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn attempt(&self) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "named-driver-lookups",
                specification: &path,
                base_directory: ".",
            },
        )
    }
    fn run(&self) -> file_application::Domain {
        self.attempt()
            .unwrap_or_else(|issues| panic!("preparation: {issues:?}"))
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn specification(driver: &str, key: &str, grouped: bool, filter: &str) -> String {
    let groups = if grouped {
        format!("    group_by: [{driver}.USUBJID]\n")
    } else {
        String::new()
    };
    format!("schema_version: '1.0'\ndomain: ADPP\nkeys: [USUBJID]\ninput:\n  PP: {{path: pp.csv, types: {{VISITDY: int, PPSTRESN: float}}}}\n  PPD: {{path: pp.csv, types: {{VISITDY: int, PPSTRESN: float}}}}\nintermediates:\n  - {{id: SS, dataset: PP, filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}}\n  - {{id: DAY1, dataset: PP, key: {{USUBJID: {key}}}, filter: \"{filter}\"}}\ncolumns: [{{name: USUBJID, type: str}}, {{name: AVAL, type: float}}]\nrows:\n  - id: subjects\n    dataset: {driver}\n{groups}    derivations: {{USUBJID: {driver}.USUBJID, AVAL: DAY1.PPSTRESN}}\noutput: {{path: output.csv, columns: [USUBJID, AVAL]}}\n")
}
const DAY1: &str = "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 1";
#[test]
fn input_and_output_key_controls_build() {
    for (driver, key, filter) in [
        ("PPD", "PPD.USUBJID", DAY1),
        ("SS", "USUBJID", DAY1),
        (
            "PPD",
            "USUBJID",
            "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 1 AND PP.USUBJID = PPD.USUBJID",
        ),
    ] {
        let study = Study::new(&specification(driver, key, true, filter));
        let mut result = study.run();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read_to_string(study.0.join("output.csv")).unwrap(),
            "USUBJID,AVAL\nS1,100\nS2,80\n"
        );
    }
}
#[test]
fn named_driver_keys_and_filters_read_the_current_driver_record() {
    for (key, grouped, filter) in [
        ("SS.USUBJID", true, DAY1),
        ("SS.USUBJID", false, DAY1),
        (
            "USUBJID",
            true,
            "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 1 AND PP.USUBJID = SS.USUBJID",
        ),
    ] {
        let spec = specification("SS", key, grouped, filter);
        let study = Study::new(&spec);
        let mut result = study.run();
        assert!(result.issues().is_empty(), "{spec}: {:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read_to_string(study.0.join("output.csv")).unwrap(),
            "USUBJID,AVAL\nS1,100\nS2,80\n"
        );
    }
}

fn assert_output(spec: &str, expected: &str) {
    let study = Study::new(spec);
    let mut result = study.run();
    assert!(result.issues().is_empty(), "{spec}: {:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(study.0.join("output.csv")).unwrap(),
        expected
    );
}
fn assert_lookup_finding(spec: &str, requirement: &str, path: &str) {
    let study = Study::new(spec);
    let issues = match study.attempt() {
        Ok(result) => {
            assert!(result.output().is_none());
            result.issues()
        }
        Err(issues) => issues,
    };
    assert!(
        !issues
            .iter()
            .any(|issue| issue.condition == "engine_rejected"),
        "{spec}: {issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.requirement.as_deref() == Some(requirement)
                && issue.spec_paths == [path]),
        "{spec}: {issues:?}"
    );
}
const EXPECTED: &str = "USUBJID,AVAL\nS1,100\nS2,80\n";

#[test]
fn named_driver_filters_and_augmented_group_keys_are_typed_in_either_order() {
    for grouped in [false, true] {
        for reversed in [false, true] {
            for filter in [DAY1, "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = SS.DAY"] {
                let mut spec = specification("SS", "SS.USUBJID", grouped, filter)
                    .replace("key: {USUBJID: SS.USUBJID}", "key: {USUBJID: SS.USUBJID, VISITDY: SS.DAY}")
                    .replace("filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}", "filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\", derivations: {DAY: {literal: 1}}}")
                    .replace("group_by: [SS.USUBJID]", "group_by: [SS.USUBJID, SS.DAY]");
                if reversed {
                    let lines = spec.lines().map(str::to_owned).collect::<Vec<_>>();
                    let mut swapped = lines.clone();
                    let first = lines
                        .iter()
                        .position(|line| line.starts_with("  - {id: SS,"))
                        .unwrap();
                    swapped.swap(first, first + 1);
                    spec = swapped.join("\n") + "\n";
                }
                assert_output(&spec, EXPECTED);
            }
        }
    }
}

#[test]
fn named_driver_column_defaults_use_the_same_current_record_scope() {
    for grouped in [false, true] {
        let spec = specification("SS", "SS.USUBJID", grouped, DAY1)
            .replace(
                "{name: AVAL, type: float}",
                "{name: AVAL, type: float, derivation: DAY1.PPSTRESN}",
            )
            .replace(", AVAL: DAY1.PPSTRESN", "");
        assert_output(&spec, EXPECTED);
    }
}

#[test]
fn group_key_exemptions_do_not_expose_other_fields_or_another_driver() {
    for (key, filter, requirement, path) in [
        ("SS.PPTESTCD", DAY1, "REQ-0117", "intermediates[1].key"),
        ("SS.MISSING", DAY1, "REQ-0117", "intermediates[1].key"),
        (
            "SS.USUBJID",
            "PP.USUBJID = SS.USUBJID AND PP.VISITDY = SS.VISITDY",
            "REQ-0120",
            "intermediates[1].filter",
        ),
        (
            "SS.USUBJID",
            "PP.USUBJID = SS.MISSING",
            "REQ-0120",
            "intermediates[1].filter",
        ),
    ] {
        assert_lookup_finding(&specification("SS", key, true, filter), requirement, path);
    }
    let mut spec = specification("SS", "SS.USUBJID", true, DAY1).replace(
        "  - {id: DAY1,",
        "  - {id: OTHER, dataset: PP, filter: \"PP.VISITDY = 14\"}\n  - {id: DAY1,",
    );
    spec=spec.replace("output: {path", "  - id: other\n    dataset: OTHER\n    group_by: [OTHER.USUBJID]\n    derivations: {USUBJID: OTHER.USUBJID, AVAL: DAY1.PPSTRESN}\noutput: {path");
    assert_lookup_finding(&spec, "REQ-0117", "intermediates[2].key");
}

#[test]
fn a_hidden_named_driver_field_is_not_a_lookup_operand() {
    let spec = specification("SS", "SS.PPTESTCD", false, DAY1).replace(
        "filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}",
        "filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\", columns: [USUBJID, PPSTRESN]}",
    );
    assert_lookup_finding(&spec, "REQ-0117", "intermediates[1].key");
}

#[test]
fn invalid_driver_declarations_keep_their_owned_eligibility_findings() {
    for extra in ["key: [USUBJID]", "order_by: [PP.VISITDY]", "no_match: null"] {
        let spec = specification("SS", "SS.USUBJID", true, DAY1).replace(
            "filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}",
            &format!("filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\", {extra}}}"),
        );
        assert_lookup_finding(&spec, "REQ-1262", "rows[0].dataset");
    }
}

#[test]
fn derived_named_driver_key_types_precede_matching() {
    let spec=specification("SS", "SS.DAY", false, DAY1)
        .replace("key: {USUBJID: SS.DAY}", "key: {VISITDY: SS.DAY}")
        .replace("filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}", "filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\", derivations: {DAY: {literal: wrong-type}}}");
    assert_lookup_finding(&spec, "REQ-0323", "intermediates[1].key");
}

#[test]
fn a_secondary_named_driver_reads_its_own_current_record() {
    let spec=specification("SS", "SS.USUBJID", true, DAY1)
        .replace("id: SS, dataset: PP, filter: \"PP.", "id: SS, dataset: PPD, filter: \"PPD.")
        .replace("AND PP.VISITDY = 14", "AND PPD.VISITDY = 14")
        .replace("rows:\n", "rows:\n  - id: input_subjects\n    dataset: PP\n    group_by: [PP.USUBJID]\n    derivations: {USUBJID: {str_template: 'input/{PP.USUBJID}'}, AVAL: {literal: 0}}\n")
        .replace("USUBJID: SS.USUBJID,", "USUBJID: {str_template: 'ss/{SS.USUBJID}'},");
    assert_output(
        &spec,
        "USUBJID,AVAL\ninput/S1,0\ninput/S2,0\nss/S1,100\nss/S2,80\n",
    );
}

#[test]
fn a_driver_field_copied_from_another_intermediate_keeps_its_numeric_type() {
    let spec = specification("SS", "SS.USUBJID", false, "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = SS.DAY")
        .replace("  - {id: SS,", "  - {id: OTHER, dataset: PPD, key: {USUBJID: USUBJID}, filter: \"PPD.PPTESTCD = 'CMAX' AND PPD.VISITDY = 1\"}\n  - {id: SS,")
        .replace("filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}", "filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\", derivations: {DAY: OTHER.VISITDY}}")
        .replace("key: {USUBJID: SS.USUBJID}", "key: {USUBJID: SS.USUBJID, VISITDY: SS.DAY}");
    assert_output(&spec, EXPECTED);
}

#[test]
fn row_filters_prove_named_driver_ownership_for_lookup_keys_and_filters() {
    for own_filter in [
        DAY1,
        "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 1 AND PP.USUBJID = SS.USUBJID",
    ] {
        let spec = specification("SS", "SS.USUBJID", false, own_filter).replace(
            "    dataset: SS\n",
            "    dataset: SS\n    filter: 'DAY1.PPSTRESN > 90'\n",
        );
        assert_output(&spec, "USUBJID,AVAL\nS1,100\n");
        let spec = spec.replace("  - {id: DAY1,", "  - {id: OTHER, dataset: PP, filter: \"PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14\"}\n  - {id: DAY1,")
            .replace("    dataset: SS\n    filter:", "    dataset: OTHER\n    filter:")
            .replace("derivations: {USUBJID: SS.USUBJID,", "derivations: {USUBJID: OTHER.USUBJID,")
            .replace("output: {path", "  - id: driver_owner\n    dataset: SS\n    derivations: {USUBJID: SS.USUBJID, AVAL: {literal: 0}}\noutput: {path");
        assert_lookup_finding(&spec, "REQ-0117", "intermediates[2].key");
    }
}

#[test]
fn a_nonunique_driver_group_is_not_reselected_as_an_ordinary_lookup() {
    let spec = specification(
        "SS",
        "SS.USUBJID",
        true,
        "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 1 AND PP.USUBJID = SS.USUBJID",
    )
    .replace(
        "PP.PPTESTCD = 'CMAX' AND PP.VISITDY = 14",
        "PP.VISITDY = 14",
    );
    assert_output(&spec, EXPECTED);
}
