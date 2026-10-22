#![cfg(feature = "yaml")]
//! A read through an implicit join that finds no record (LOOKUP-24): a source
//! binding answers with its `absent` value, missing by default, and an operand
//! with missing, each counted under `absent` at the read's path; a read made
//! while a predicate is evaluated is not counted. `require_match: true` on the
//! input fails every such read as `unmatched_key`, unless the read declares
//! its own `absent` answer, and leaves a named intermediate's policy alone.
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("implicit-join-absence");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("ae.csv"), AE).unwrap();
        fs::write(root.join("adsl.csv"), ADSL).unwrap();
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "implicit-join-absence",
                specification,
                base_directory: ".",
            },
        )
    }
    /// The saved artifact and the run's handler counts as `(path, count)`.
    fn saved(&self) -> (String, Vec<(String, i64)>) {
        let mut domain = self.run().unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        (
            fs::read_to_string(self.0.join("adae.csv")).unwrap(),
            counts(&domain.observations()),
        )
    }
    /// The run's first issue.
    fn failure(&self) -> Value {
        let domain = self.run().unwrap();
        let issues = domain.issues();
        assert!(!issues.is_empty(), "the run succeeded");
        yamaa_adapters::report_storage::diagnostic_value(&issues[0])
    }
}

fn counts(observations: &Value) -> Vec<(String, i64)> {
    observations["handler_counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["spec_path"].as_str().unwrap().to_owned(),
                entry["count"].as_i64().unwrap(),
            )
        })
        .collect()
}

// Subject S-2 has an adverse event but no subject-level record.
const AE: &str = "STUDYID,USUBJID,AESEQ,AETERM\nS,S-1,1,HEADACHE\nS,S-2,1,NAUSEA\n";
const ADSL: &str = "STUDYID,USUBJID,TRTSDT,ARM,AGE,SAFFL\nS,S-1,2025-01-10,drug a,40,Y\n";

fn specification(adsl: &str, columns: &str, extra: &str) -> String {
    format!(
        "schema_version: \"1.0\"\n\
         domain: ADAE\n\
         keys: [STUDYID, USUBJID, AESEQ]\n\
         input:\n\
         \x20 AE: ae.csv\n\
         \x20 ADSL: {adsl}\n\
         base: AE\n\
         output:\n\
         \x20 path: adae.csv\n\
         \x20 columns: [STUDYID, USUBJID, AESEQ{columns}]\n\
         {extra}\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: AE.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: AE.USUBJID}}\n\
         \x20 - {{name: AESEQ, type: int, derivation: AE.AESEQ}}\n"
    )
}

const READS: &str = "  - {name: TRTSDT, type: str, derivation: ADSL.TRTSDT}\n\
    \x20 - name: ARMU\n\
    \x20   type: str\n\
    \x20   derivation: {str_case: {source: ADSL.ARM, to: upper}}\n\
    \x20 - name: SAFE\n\
    \x20   type: str\n\
    \x20   derivation: {flag: \"ADSL.SAFFL = 'Y'\"}\n";

#[test]
fn every_value_read_counts_its_missing_record_and_a_predicate_read_does_not() {
    let fixture = Fixture::new(&format!(
        "{}{READS}",
        specification(
            "{path: adsl.csv, types: {AGE: int}}",
            ", TRTSDT, ARMU, SAFE",
            ""
        )
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,TRTSDT,ARMU,SAFE\n\
         S,S-1,1,2025-01-10,DRUG A,Y\n\
         S,S-2,1,,,\n"
    );
    let count = |path: &str| {
        counts
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, count)| *count)
    };
    // The concise source and the operand each count the subject without a
    // record under `absent` at the read's path.
    assert_eq!(count("columns.TRTSDT.derivation.source.absent"), Some(1));
    assert_eq!(
        count("columns.ARMU.derivation.str_case.source.absent"),
        Some(1)
    );
    // The verb then answers its missing source, and the flag its unknown
    // condition, each under its own `missing`.
    assert_eq!(count("columns.ARMU.derivation.str_case.missing"), Some(1));
    assert_eq!(count("columns.SAFE.derivation.flag.missing"), Some(1));
    // The predicate's read is not a handler site.
    assert!(
        counts
            .iter()
            .all(|(path, _)| !path.starts_with("columns.SAFE")
                || path == "columns.SAFE.derivation.flag.missing"),
        "{counts:?}"
    );
}

#[test]
fn an_input_requiring_a_match_fails_a_concise_read_as_unmatched_key() {
    let fixture = Fixture::new(&format!(
        "{}  - {{name: TRTSDT, type: str, derivation: ADSL.TRTSDT}}\n",
        specification("{path: adsl.csv, require_match: true}", ", TRTSDT", "")
    ));
    let failure = fixture.failure();
    assert_eq!(failure["phase"], "join");
    assert_eq!(failure["condition"], "unmatched_key");
    assert_eq!(failure["rule"], "LOOKUP-24");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["columns.TRTSDT.derivation.source"])
    );
    assert_eq!(failure["context"]["dataset"], "ADSL");
    assert_eq!(failure["context"]["intermediate"], "intermediate(ADSL)");
    assert_eq!(
        failure["context"]["intermediate_key"],
        yamaa_adapters::json!({"STUDYID": "S", "USUBJID": "S-2"})
    );
}

#[test]
fn an_input_requiring_a_match_fails_operand_and_predicate_reads_at_their_paths() {
    for (column, derivation, path) in [
        (
            "ARMU",
            "{str_case: {source: ADSL.ARM, to: upper}}",
            "columns.ARMU.derivation.str_case.source",
        ),
        (
            "SAFE",
            "{flag: \"ADSL.SAFFL = 'Y'\"}",
            "columns.SAFE.derivation.flag.condition",
        ),
    ] {
        let fixture = Fixture::new(&format!(
            "{}  - {{name: {column}, type: str, derivation: {derivation}}}\n",
            specification(
                "{path: adsl.csv, types: {AGE: int}, require_match: true}",
                &format!(", {column}"),
                ""
            )
        ));
        let failure = fixture.failure();
        assert_eq!(failure["condition"], "unmatched_key", "{column}");
        assert_eq!(failure["rule"], "LOOKUP-24", "{column}");
        assert_eq!(failure["spec_paths"], yamaa_adapters::json!([path]));
    }
}

#[test]
fn a_declared_absent_answers_before_the_input_requirement() {
    let fixture = Fixture::new(&format!(
        "{}  - name: TRTSDT\n\
         \x20   type: str\n\
         \x20   derivation:\n\
         \x20     source: {{variable: ADSL.TRTSDT, absent: NONE}}\n",
        specification("{path: adsl.csv, require_match: true}", ", TRTSDT", "")
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,TRTSDT\nS,S-1,1,2025-01-10\nS,S-2,1,NONE\n"
    );
    assert!(counts.contains(&("columns.TRTSDT.derivation.source.absent".into(), 1)));
}

#[test]
fn an_input_requirement_registers_no_count_and_allows_an_unread_input() {
    // The required input is never read through a join: the run succeeds and
    // registers nothing for it.
    let fixture = Fixture::new(&specification(
        "{path: adsl.csv, require_match: true}",
        "",
        "",
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(csv, "STUDYID,USUBJID,AESEQ\nS,S-1,1\nS,S-2,1\n");
    assert!(counts.is_empty(), "{counts:?}");
}

#[test]
fn an_input_requirement_leaves_a_named_intermediate_its_own_policy() {
    let fixture = Fixture::new(&format!(
        "{}  - {{name: ARM, type: str, derivation: SUBJECT.ARM}}\n",
        specification(
            "{path: adsl.csv, require_match: true}",
            ", ARM",
            "intermediates:\n\
             \x20 - id: SUBJECT\n\
             \x20   dataset: ADSL\n\
             \x20   filter: \"ADSL.SAFFL = 'Y'\"\n"
        )
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(csv, "STUDYID,USUBJID,AESEQ,ARM\nS,S-1,1,drug a\nS,S-2,1,\n");
    assert_eq!(
        counts,
        [("columns.ARM.derivation.source.no_match".to_owned(), 1)]
    );
}

const ROWS: &str = "schema_version: \"1.0\"\n\
    domain: ADAE\n\
    keys: [STUDYID, USUBJID, AESEQ]\n\
    input:\n\
    \x20 AE: ae.csv\n\
    \x20 ADSL: ADSL_DECLARATION\n\
    output:\n\
    \x20 path: adae.csv\n\
    \x20 columns: [STUDYID, USUBJID, AESEQ, ARMU]\n\
    rows:\n\
    \x20 - id: event\n\
    \x20   dataset: AE\n\
    \x20   derivations:\n\
    \x20     STUDYID: AE.STUDYID\n\
    \x20     USUBJID: AE.USUBJID\n\
    \x20     AESEQ: AE.AESEQ\n\
    \x20     ARMU: {str_case: {source: ADSL.ARM, to: upper}}\n\
    columns:\n\
    \x20 - {name: STUDYID, type: str}\n\
    \x20 - {name: USUBJID, type: str}\n\
    \x20 - {name: AESEQ, type: int}\n\
    \x20 - {name: ARMU, type: str}\n";

#[test]
fn a_row_template_counts_an_operand_read_and_fails_it_when_required() {
    let fixture = Fixture::new(&ROWS.replace("ADSL_DECLARATION", "adsl.csv"));
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,ARMU\nS,S-1,1,DRUG A\nS,S-2,1,\n"
    );
    for path in [
        "rows[0].derivations.ARMU.str_case.source.absent",
        "rows[0].derivations.ARMU.str_case.missing",
    ] {
        assert!(counts.contains(&(path.into(), 1)), "{path}: {counts:?}");
    }
    let fixture =
        Fixture::new(&ROWS.replace("ADSL_DECLARATION", "{path: adsl.csv, require_match: true}"));
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unmatched_key");
    assert_eq!(failure["rule"], "LOOKUP-24");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["rows[0].derivations.ARMU.str_case.source"])
    );
}
