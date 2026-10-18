#![cfg(feature = "yaml")]
//! One lookup model (LOOKUP-25): an implicit join, a named intermediate and a
//! qualified aggregate match their keys and range (LOOKUP-9), narrow the
//! matched records by their filter (LOOKUP-8), and select or reduce the
//! candidates in stored record order (LOOKUP-10).
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str, inputs: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-lookup-model-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, content) in inputs {
            fs::write(root.join(name), content).unwrap();
        }
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "lookup-model",
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
        let counts = domain.observations()["handler_counts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    entry["spec_path"].as_str().unwrap().to_owned(),
                    entry["count"].as_i64().unwrap(),
                )
            })
            .collect();
        (fs::read_to_string(self.0.join("out.csv")).unwrap(), counts)
    }
    /// The first issue, from validation or from the run.
    fn failure(&self) -> Value {
        match self.run() {
            Ok(domain) => {
                let issues = domain.issues();
                assert!(!issues.is_empty(), "the run succeeded");
                issues[0].as_value()
            }
            Err(issues) => issues[0].as_value(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

/// Adverse events and the treatment periods of each subject. S-1 has two
/// periods, an event in each, one on the second period's first day and one
/// before any period; S-2 has an event without a start date and a period
/// without an end date.
const AE: &str = "STUDYID,USUBJID,AESEQ,ASTDT\n\
    S,S-1,1,2025-01-05\n\
    S,S-1,2,2025-02-01\n\
    S,S-1,3,2025-02-20\n\
    S,S-1,4,2024-12-01\n\
    S,S-2,1,\n\
    S,S-2,2,2025-03-10\n";
const PERIODS: &str = "STUDYID,USUBJID,APERIOD,APERSDT,APEREDT\n\
    S,S-1,1,2025-01-01,2025-01-31\n\
    S,S-1,2,2025-02-01,2025-02-28\n\
    S,S-2,1,2025-03-01,\n";

fn specification(header: &str, columns: &str, output: &str) -> String {
    format!(
        "schema_version: \"1.0\"\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, AESEQ]\n\
         input:\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int, ASTDT: date}}}}\n\
         \x20 PERIODS: {{path: periods.csv, types: {{APERIOD: int, APERSDT: date, APEREDT: date}}}}\n\
         {header}\
         output:\n\
         \x20 path: out.csv\n\
         \x20 columns: [STUDYID, USUBJID, AESEQ{output}]\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: AE.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: AE.USUBJID}}\n\
         \x20 - {{name: AESEQ, type: int, derivation: AE.AESEQ}}\n\
         \x20 - {{name: ASTDT, type: date, derivation: AE.ASTDT}}\n\
         {columns}"
    )
}

const PERIOD: &str = "  - name: APERIOD\n\
    \x20   type: int\n\
    \x20   derivation:\n\
    \x20     source:\n\
    \x20       variable: PERIODS.APERIOD\n\
    \x20       between: {value: ASTDT, lower: APERSDT, upper: APEREDT}\n";

#[test]
fn an_implicit_join_matches_its_keys_and_an_inclusive_range() {
    let fixture = Fixture::new(
        &specification("base: AE\n", PERIOD, ", APERIOD"),
        &[("ae.csv", AE), ("periods.csv", PERIODS)],
    );
    let (csv, counts) = fixture.saved();
    let expected = "STUDYID,USUBJID,AESEQ,APERIOD\n\
        S,S-1,1,1\n\
        S,S-1,2,2\n\
        S,S-1,3,2\n\
        S,S-1,4,\n\
        S,S-2,1,\n\
        S,S-2,2,\n";
    assert_eq!(csv, expected);
    // The event before any period, the missing start date and the period
    // without an end date are each answered with missing, counted.
    assert!(counts.contains(&("columns.APERIOD.derivation.source.absent".into(), 3)));
}

#[test]
fn a_row_template_range_reads_its_driver_record() {
    let rows = "rows:\n\
        \x20 - id: events\n\
        \x20   dataset: AE\n\
        \x20   derivations:\n\
        \x20     APERIOD:\n\
        \x20       source:\n\
        \x20         variable: PERIODS.APERIOD\n\
        \x20         between: {value: AE.ASTDT, lower: APERSDT, upper: APEREDT}\n";
    let columns = "  - {name: APERIOD, type: int}\n";
    let fixture = Fixture::new(
        &specification(rows, columns, ", APERIOD"),
        &[("ae.csv", AE), ("periods.csv", PERIODS)],
    );
    let (csv, _) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,APERIOD\n\
         S,S-1,1,1\n\
         S,S-1,2,2\n\
         S,S-1,3,2\n\
         S,S-1,4,\n\
         S,S-2,1,\n\
         S,S-2,2,\n"
    );
}

#[test]
fn a_range_with_a_filter_and_a_selection_keeps_the_four_steps() {
    // Overlapping windows: the filter narrows the matched windows, and keep
    // chooses the latest-starting one, the later stored record on a tie.
    let windows = "STUDYID,USUBJID,APERIOD,APERSDT,APEREDT\n\
        S,S-1,1,2025-01-01,2025-12-31\n\
        S,S-1,2,2025-02-01,2025-02-28\n\
        S,S-1,3,2025-02-01,2025-03-31\n\
        S,S-1,9,2025-01-01,2025-12-31\n";
    let column = "  - name: APERIOD\n\
        \x20   type: int\n\
        \x20   derivation:\n\
        \x20     source:\n\
        \x20       variable: PERIODS.APERIOD\n\
        \x20       between: {value: ASTDT, lower: APERSDT, upper: APEREDT}\n\
        \x20       filter: \"PERIODS.APERIOD < 9\"\n\
        \x20       order_by: [PERIODS.APERSDT]\n\
        \x20       keep: last\n";
    let fixture = Fixture::new(
        &specification("base: AE\n", column, ", APERIOD"),
        &[("ae.csv", AE), ("periods.csv", windows)],
    );
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,APERIOD\n\
         S,S-1,1,1\n\
         S,S-1,2,3\n\
         S,S-1,3,3\n\
         S,S-1,4,\n\
         S,S-2,1,\n\
         S,S-2,2,\n"
    );
    assert!(counts.contains(&(
        "columns.APERIOD.derivation.source.multiple_matches".into(),
        2
    )));
}

#[test]
fn a_required_range_match_fails_the_row_without_a_record() {
    let column = PERIOD.replace(
        "       between:",
        "       require_match: true\n\x20       between:",
    );
    let fixture = Fixture::new(
        &specification("base: AE\n", &column, ", APERIOD"),
        &[("ae.csv", AE), ("periods.csv", PERIODS)],
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unmatched_key");
    assert_eq!(failure["requirement"], "LOOKUP-24");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["columns.APERIOD.derivation.source"])
    );
}

#[test]
fn a_range_value_names_a_value_of_the_current_row() {
    let other = "STUDYID,USUBJID,REFDT\nS,S-1,2025-01-05\nS,S-2,2025-03-10\n";
    for (value, condition, requirement, path) in [
        // Another input's record is not a value of an event row (LOOKUP-9).
        (
            "OTHER.REFDT",
            "unknown_field",
            "REQ-0121",
            "columns.APERIOD.derivation.source.between.value",
        ),
        (
            "ABSENT",
            "unknown_field",
            "REQ-0121",
            "columns.APERIOD.derivation.source.between.value",
        ),
        (
            "PERIODS.APERSDT",
            "unknown_field",
            "REQ-0121",
            "columns.APERIOD.derivation.source.between.value",
        ),
        // A number cannot fall between two dates.
        (
            "AESEQ",
            "incomparable_range_types",
            "REQ-0121",
            "columns.APERIOD.derivation.source.between",
        ),
    ] {
        let column = PERIOD.replace("value: ASTDT", &format!("value: {value}"));
        let fixture = Fixture::new(
            &specification("base: AE\n", &column, ", APERIOD").replace(
                "input:\n",
                "input:\n  OTHER: {path: other.csv, types: {REFDT: date}}\n",
            ),
            &[
                ("ae.csv", AE),
                ("periods.csv", PERIODS),
                ("other.csv", other),
            ],
        );
        let failure = fixture.failure();
        assert_eq!(
            (
                failure["phase"].as_str(),
                failure["condition"].as_str(),
                failure["requirement"].as_str()
            ),
            (Some("validation"), Some(condition), Some(requirement)),
            "{value}: {failure:?}"
        );
        assert_eq!(failure["spec_paths"], yamaa_adapters::json!([path]));
    }
}

#[test]
fn a_range_on_a_read_without_an_implicit_join_is_refused() {
    let column = "  - name: DRIVER\n\
        \x20   type: date\n\
        \x20   derivation:\n\
        \x20     source:\n\
        \x20       variable: AE.ASTDT\n\
        \x20       between: {value: ASTDT, lower: ASTDT}\n";
    let fixture = Fixture::new(
        &specification("base: AE\n", column, ", DRIVER"),
        &[("ae.csv", AE), ("periods.csv", PERIODS)],
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "prohibited_construct");
    assert_eq!(failure["requirement"], "LOOKUP-9");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["columns.DRIVER.derivation.source.between"])
    );
}

/// Narrowing reads only matched records (LOOKUP-8): a filter that would fail
/// on a record no row matches is never asked about it, for every kind.
#[test]
fn every_kind_narrows_only_the_records_that_match() {
    // PERIOD 0 of an unknown subject divides by zero in each filter.
    let periods = "STUDYID,USUBJID,APERIOD,APERSDT,APEREDT\n\
        S,S-1,1,2025-01-01,2025-01-31\n\
        S,S-9,0,2025-01-01,2025-01-31\n";
    let filter = "\"10 / PERIODS.APERIOD > 1\"";
    let columns = format!(
        "  - name: JOINED\n\
         \x20   type: int\n\
         \x20   derivation:\n\
         \x20     source: {{variable: PERIODS.APERIOD, filter: {filter}}}\n\
         \x20 - {{name: NAMED, type: int, derivation: P.APERIOD}}\n\
         \x20 - name: COUNTED\n\
         \x20   type: int\n\
         \x20   derivation:\n\
         \x20     aggregate: {{expr: \"COUNT(PERIODS.*)\", filter: {filter}}}\n"
    );
    let header = format!(
        "base: AE\n\
         intermediates:\n\
         \x20 - id: P\n\
         \x20   dataset: PERIODS\n\
         \x20   key: [STUDYID, USUBJID]\n\
         \x20   filter: {filter}\n"
    );
    let events = "STUDYID,USUBJID,AESEQ,ASTDT\nS,S-1,1,2025-01-05\n";
    let fixture = Fixture::new(
        &specification(&header, &columns, ", JOINED, NAMED, COUNTED"),
        &[("ae.csv", events), ("periods.csv", periods)],
    );
    let (csv, _) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,JOINED,NAMED,COUNTED\nS,S-1,1,1,1,1\n"
    );
}

/// Clinic and home measurements, each driving its own row template; a window
/// table gives each subject two day windows.
const CLINIC: &str = "STUDYID,USUBJID,SEQ,DAY\nS,S-1,1,3\nS,S-1,2,40\n";
const HOME: &str = "STUDYID,USUBJID,SEQ,DAY\nS,S-1,3,12\nS,S-1,4,99\n";
const WINDOWS: &str = "STUDYID,NAME,LO,HI\nS,EARLY,1,10\nS,LATE,11,50\n";

fn measurements(clinic: &str, home: &str, header: &str) -> String {
    format!(
        "schema_version: \"1.0\"\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, SEQ]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 HOME: {{path: home.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 WINDOWS: {{path: windows.csv, types: {{LO: int, HI: int}}}}\n\
         {header}\
         output:\n\
         \x20 path: out.csv\n\
         \x20 columns: [STUDYID, USUBJID, SEQ, WINDOW]\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: SEQ, type: int}}\n\
         \x20 - {{name: WINDOW, type: str}}\n\
         rows:\n\
         \x20 - id: clinic\n\
         \x20   dataset: CLINIC\n\
         \x20   derivations:\n\
         \x20     STUDYID: CLINIC.STUDYID\n\
         \x20     USUBJID: CLINIC.USUBJID\n\
         \x20     SEQ: CLINIC.SEQ\n\
         \x20     WINDOW: {clinic}\n\
         \x20 - id: home\n\
         \x20   dataset: HOME\n\
         \x20   derivations:\n\
         \x20     STUDYID: HOME.STUDYID\n\
         \x20     USUBJID: HOME.USUBJID\n\
         \x20     SEQ: HOME.SEQ\n\
         \x20     WINDOW: {home}\n"
    )
}

fn window(value: &str) -> String {
    format!(
        "{{source: {{variable: WINDOWS.NAME, between: {{value: {value}, lower: LO, upper: HI}}}}}}"
    )
}

#[test]
fn a_range_value_reads_the_record_of_the_driver_of_its_own_row() {
    let inputs = [
        ("clinic.csv", CLINIC),
        ("home.csv", HOME),
        ("windows.csv", WINDOWS),
    ];
    // Each template's range reads its own driver's record, on its own port.
    let fixture = Fixture::new(
        &measurements(&window("CLINIC.DAY"), &window("HOME.DAY"), ""),
        &inputs,
    );
    let (csv, _) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,SEQ,WINDOW\nS,S-1,1,EARLY\nS,S-1,2,LATE\nS,S-1,3,LATE\nS,S-1,4,\n"
    );
    // A home row has no clinic record (LOOKUP-9).
    let fixture = Fixture::new(
        &measurements(&window("CLINIC.DAY"), &window("CLINIC.DAY"), ""),
        &inputs,
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unknown_field");
    assert_eq!(failure["requirement"], "REQ-0121");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["rows[1].derivations.WINDOW.source.between.value"])
    );
    // The same rule holds for a named intermediate read from both templates,
    // and for an aggregate a column derives for every row.
    let named = "intermediates:\n\
        \x20 - id: W\n\
        \x20   dataset: WINDOWS\n\
        \x20   key: [STUDYID]\n\
        \x20   between: {value: CLINIC.DAY, lower: LO, upper: HI}\n";
    let fixture = Fixture::new(&measurements("W.NAME", "W.NAME", named), &inputs);
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unknown_field");
    assert_eq!(failure["requirement"], "REQ-0121");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["intermediates[0].between.value"])
    );
    let fixture = Fixture::new(&measurements("W.NAME", "{literal: x}", named), &inputs);
    let (csv, _) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,SEQ,WINDOW\nS,S-1,1,EARLY\nS,S-1,2,LATE\nS,S-1,3,x\nS,S-1,4,x\n"
    );
}

#[test]
fn a_grouped_row_range_reads_a_group_key_of_its_driver() {
    let header = "rows:\n\
        \x20 - id: days\n\
        \x20   dataset: CLINIC\n\
        \x20   group_by: [CLINIC.STUDYID, CLINIC.USUBJID]\n\
        \x20   derivations:\n\
        \x20     STUDYID: CLINIC.STUDYID\n\
        \x20     USUBJID: CLINIC.USUBJID\n\
        \x20     WINDOW:\n\
        \x20       source:\n\
        \x20         variable: WINDOWS.NAME\n\
        \x20         between: {value: CLINIC.DAY, lower: LO, upper: HI}\n";
    let specification = format!(
        "schema_version: \"1.0\"\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 WINDOWS: {{path: windows.csv, types: {{LO: int, HI: int}}}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, WINDOW]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: WINDOW, type: str}}\n\
         {header}"
    );
    let fixture = Fixture::new(
        &specification,
        &[("clinic.csv", CLINIC), ("windows.csv", WINDOWS)],
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "ungrouped_driver_field");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["rows[0].derivations.WINDOW.source.between.value"])
    );
}

/// A key value whose type is decided per value is compared when it is
/// matched, for every kind (LOOKUP-9): a text value against an integer key
/// column fails the run instead of matching nothing.
#[test]
fn a_per_value_key_type_that_cannot_compare_fails_for_every_kind() {
    let events = "STUDYID,USUBJID,AESEQ,ASTDT\nS,S-1,1,2025-01-05\n";
    let periods = "STUDYID,USUBJID,APERIOD,APERSDT,APEREDT\nS,S-1,1,2025-01-01,2025-01-31\n";
    // The match value is the text "none" or the number 1: a str or an int.
    let choice = "{mapping: {source: USUBJID, dict: {S-1: none, S-2: 1}}}";
    for (column, path) in [
        (
            "  - name: COUNTED\n\
             \x20   type: int\n\
             \x20   derivation:\n\
             \x20     aggregate:\n\
             \x20       key: {APERIOD: CHOICE}\n\
             \x20       expr: \"COUNT(PERIODS.*)\"\n",
            "columns.COUNTED.derivation.aggregate.key.APERIOD",
        ),
        (
            "  - {name: COUNTED, type: int, derivation: P.APERIOD}\n",
            "intermediates[0].key.APERIOD",
        ),
    ] {
        let header = "base: AE\n\
            intermediates:\n\
            \x20 - id: P\n\
            \x20   dataset: PERIODS\n\
            \x20   key: {APERIOD: CHOICE}\n";
        let fixture = Fixture::new(
            &specification(
                &header.replace("CHOICE", choice),
                &column.replace("CHOICE", choice),
                ", COUNTED",
            ),
            &[("ae.csv", events), ("periods.csv", periods)],
        );
        let failure = fixture.failure();
        assert_eq!(
            (
                failure["phase"].as_str(),
                failure["condition"].as_str(),
                failure["requirement"].as_str()
            ),
            (
                Some("validation"),
                Some("incompatible_input_type"),
                Some("LOOKUP-9")
            ),
            "{failure:?}"
        );
        assert_eq!(failure["spec_paths"], yamaa_adapters::json!([path]));
        assert_eq!(failure["context"]["expected"], "str");
        assert_eq!(failure["context"]["actual"], "int");
    }
}
