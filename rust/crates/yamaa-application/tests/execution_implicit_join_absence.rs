#![cfg(feature = "yaml")]
//! A read through an implicit join that finds no record (LOOKUP-24): a source
//! binding answers with its `absent` value, missing by default, and an operand
//! with missing, each counted under `absent` at the read's path; a read made
//! while a predicate is evaluated is not counted. `require_match: true` on the
//! input fails every such read as `unmatched_key`, unless the read declares
//! its own `absent` answer, and leaves a named intermediate's policy alone. A
//! named intermediate's absence is counted under `absent` too, at the same
//! read paths.
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::file_application;
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
        crate::support::study::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("implicit-join-absence", specification),
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
        "yamaa: \"1.0\"\n\
         type: spec\n\
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

/// No benchmark's run report counts an absence, so none pins these counts.
#[test]
fn every_value_read_counts_its_missing_record_and_a_predicate_read_does_not() {
    let fixture = Fixture::new(&format!(
        "{}{READS}",
        specification(
            "{path: adsl.csv, key: [STUDYID, USUBJID], types: {AGE: int}}",
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

/// `negative-input-require-match` reads its required subject-level input
/// through a concise read for an event whose subject has no record, and
/// states the whole finding.
#[test]
fn an_input_requiring_a_match_fails_a_concise_read_as_unmatched_key() {
    crate::support::benchmarks::reproduce("negative-input-require-match");
}

/// No benchmark fails an operand's or a predicate's read of a required input:
/// `negative-input-require-match` fails a concise read.
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
                "{path: adsl.csv, key: [STUDYID, USUBJID], types: {AGE: int}, require_match: true}",
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

/// No benchmark declares an `absent` answer on a read of an input that
/// requires a match.
#[test]
fn a_declared_absent_answers_before_the_input_requirement() {
    let fixture = Fixture::new(&format!(
        "{}  - name: TRTSDT\n\
         \x20   type: str\n\
         \x20   derivation:\n\
         \x20     source: {{variable: ADSL.TRTSDT, absent: NONE}}\n",
        specification(
            "{path: adsl.csv, key: [STUDYID, USUBJID], require_match: true}",
            ", TRTSDT",
            "",
        )
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,TRTSDT\nS,S-1,1,2025-01-10\nS,S-2,1,NONE\n"
    );
    assert!(counts.contains(&("columns.TRTSDT.derivation.source.absent".into(), 1)));
}

/// No benchmark requires a match of an input that no read joins.
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

/// No benchmark reads a named intermediate over an input that requires a
/// match.
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
             \x20   key: [STUDYID, USUBJID]\n\
             \x20   filter: \"ADSL.SAFFL = 'Y'\"\n"
        )
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(csv, "STUDYID,USUBJID,AESEQ,ARM\nS,S-1,1,drug a\nS,S-2,1,\n");
    assert_eq!(
        counts,
        [("columns.ARM.derivation.source.absent".to_owned(), 1)]
    );
}

/// TRTSDT reads ADSL through its implicit join, ARM and ARMD read a lookup of
/// ADSL's safety population, by default and with a declared answer, and AGE1
/// adds one to the lookup's AGE. The file declares ADSL's key and the
/// lookups' own.
const NAMED: &str = "yamaa: \"1.0\"\n\
    type: spec\n\
    domain: ADAE\n\
    keys: [STUDYID, USUBJID, AESEQ]\n\
    input:\n\
    \x20 AE: ae.csv\n\
    \x20 ADSL: {path: adsl.csv, key: [STUDYID, USUBJID], types: {AGE: int}}\n\
    base: AE\n\
    output:\n\
    \x20 path: adae.csv\n\
    \x20 columns: [STUDYID, USUBJID, AESEQ, TRTSDT, ARM, ARMD, AGE1]\n\
    intermediates:\n\
    \x20 - {id: SAFETY, dataset: ADSL, key: [STUDYID, USUBJID], filter: \"ADSL.SAFFL = 'Y'\"}\n\
    \x20 - {id: NAMED, dataset: ADSL, key: [STUDYID, USUBJID], filter: \"ADSL.SAFFL = 'Y'\", absent: NONE}\n\
    columns:\n\
    \x20 - {name: STUDYID, type: str, derivation: AE.STUDYID}\n\
    \x20 - {name: USUBJID, type: str, derivation: AE.USUBJID}\n\
    \x20 - {name: AESEQ, type: int, derivation: AE.AESEQ}\n\
    \x20 - {name: TRTSDT, type: str, derivation: ADSL.TRTSDT}\n\
    \x20 - {name: ARM, type: str, derivation: SAFETY.ARM}\n\
    \x20 - {name: ARMD, type: str, derivation: NAMED.ARM}\n\
    \x20 - {name: AGE1, type: int, derivation: \"SAFETY.AGE + 1\"}\n";

/// S-2 has no subject-level record, so each read answers it once. A named
/// intermediate's answer, declared or by default, is counted under `absent`
/// at the read's path, as the implicit join's is (LOOKUP-14, D202). No
/// benchmark's run report counts an absence.
#[test]
fn a_named_intermediate_s_absence_counts_under_the_handler_its_surface_names() {
    let fixture = Fixture::new(NAMED);
    let mut domain = fixture.run().unwrap();
    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
    assert!(domain.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("adae.csv")).unwrap(),
        "STUDYID,USUBJID,AESEQ,TRTSDT,ARM,ARMD,AGE1\n\
         S,S-1,1,2025-01-10,drug a,drug a,41\n\
         S,S-2,1,,,NONE,\n"
    );
    assert_eq!(
        domain.observations()["handler_counts"],
        yamaa_adapters::json!([
            {"spec_path": "columns.TRTSDT.derivation.source.absent", "handler": "absent", "count": 1},
            {"spec_path": "columns.ARM.derivation.source.absent", "handler": "absent", "count": 1},
            {"spec_path": "columns.ARMD.derivation.source.absent", "handler": "absent", "count": 1},
            {"spec_path": "columns.AGE1.derivation.absent", "handler": "absent", "count": 1},
        ])
    );
}

const ROWS: &str = "yamaa: \"1.0\"\n\
    type: spec\n\
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

/// No benchmark's run report counts an absence, and no benchmark requires a
/// match of an input that a row template reads.
#[test]
fn a_row_template_counts_an_operand_read_and_fails_it_when_required() {
    let fixture = Fixture::new(&ROWS.edit(
        "ADSL_DECLARATION",
        "{path: adsl.csv, key: [STUDYID, USUBJID]}",
    ));
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,ARMU\nS,S-1,1,DRUG A\nS,S-2,1,\n"
    );
    for path in [
        "rows.event.derivations.ARMU.str_case.source.absent",
        "rows.event.derivations.ARMU.str_case.missing",
    ] {
        assert!(counts.contains(&(path.into(), 1)), "{path}: {counts:?}");
    }
    let fixture = Fixture::new(&ROWS.edit(
        "ADSL_DECLARATION",
        "{path: adsl.csv, key: [STUDYID, USUBJID], require_match: true}",
    ));
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unmatched_key");
    assert_eq!(failure["rule"], "LOOKUP-24");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["rows.event.derivations.ARMU.str_case.source"])
    );
}
