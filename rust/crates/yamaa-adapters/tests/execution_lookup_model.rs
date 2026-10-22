#![cfg(feature = "yaml")]
//! One lookup model (LOOKUP-25): an implicit join, a named intermediate and a
//! qualified aggregate match their keys and range (LOOKUP-9), narrow the
//! matched records by their filter (LOOKUP-8), and select or reduce the
//! candidates in stored record order (LOOKUP-10).
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, inputs: &[(&str, &str)]) -> Self {
        let root = crate::support::Scratch::new("lookup-model");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, content) in inputs {
            fs::write(root.join(name), content).unwrap();
        }
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
            crate::support::study::identity("lookup-model", specification),
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
                yamaa_adapters::report_storage::diagnostic_value(&issues[0])
            }
            Err(issues) => yamaa_adapters::report_storage::diagnostic_value(&issues[0]),
        }
    }
    /// Every issue of the run, from validation or from the run itself.
    fn issues(&self) -> Vec<Value> {
        match self.run() {
            Ok(domain) => domain
                .issues()
                .iter()
                .map(yamaa_adapters::report_storage::diagnostic_value)
                .collect(),
            Err(issues) => issues
                .iter()
                .map(yamaa_adapters::report_storage::diagnostic_value)
                .collect(),
        }
    }
    /// Every issue `check` reports from the declarations alone.
    fn checked(&self) -> Vec<Value> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("lookup-model", specification),
        )
        .iter()
        .map(yamaa_adapters::report_storage::diagnostic_value)
        .collect()
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

/// PERIODS declares the key its implicit joins and keyless aggregates match on.
fn specification(header: &str, columns: &str, output: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, AESEQ]\n\
         input:\n\
         \x20 AE: {{path: ae.csv, types: {{AESEQ: int, ASTDT: date}}}}\n\
         \x20 PERIODS: {{path: periods.csv, key: [STUDYID, USUBJID], types: {{APERIOD: int, APERSDT: date, APEREDT: date}}}}\n\
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
    let column = PERIOD.edit(
        "       between:",
        "       require_match: true\n\x20       between:",
    );
    let fixture = Fixture::new(
        &specification("base: AE\n", &column, ", APERIOD"),
        &[("ae.csv", AE), ("periods.csv", PERIODS)],
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unmatched_key");
    assert_eq!(failure["rule"], "LOOKUP-24");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["columns.APERIOD.derivation.source"])
    );
}

#[test]
fn a_range_value_names_a_value_of_the_current_row() {
    let other = "STUDYID,USUBJID,REFDT\nS,S-1,2025-01-05\nS,S-2,2025-03-10\n";
    for (value, condition, rule, path) in [
        // Another input's record is not a value of an event row (LOOKUP-9).
        (
            "OTHER.REFDT",
            "unknown_field",
            "LOOKUP-9",
            "columns.APERIOD.derivation.source.between.value",
        ),
        (
            "ABSENT",
            "unknown_field",
            "LOOKUP-9",
            "columns.APERIOD.derivation.source.between.value",
        ),
        (
            "PERIODS.APERSDT",
            "unknown_field",
            "LOOKUP-9",
            "columns.APERIOD.derivation.source.between.value",
        ),
        // A number cannot fall between two dates.
        (
            "AESEQ",
            "incomparable_range_types",
            "LOOKUP-9",
            "columns.APERIOD.derivation.source.between",
        ),
    ] {
        let column = PERIOD.edit("value: ASTDT", &format!("value: {value}"));
        let fixture = Fixture::new(
            &specification("base: AE\n", &column, ", APERIOD").edit(
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
                failure["rule"].as_str()
            ),
            (Some("validation"), Some(condition), Some(rule)),
            "{value}: {failure:?}"
        );
        assert_eq!(failure["spec_paths"], yamaa_adapters::json!([path]));
        // A value that names nothing and one that names another input's
        // record report the binding's variable alike.
        if condition == "unknown_field" {
            assert_eq!(failure["context"]["identifier"], value);
            assert_eq!(failure["context"]["variable"], "PERIODS.APERIOD");
        }
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
    assert_eq!(failure["rule"], "LOOKUP-9");
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

/// WINDOWS declares the key its implicit joins match on.
fn measurements(clinic: &str, home: &str, header: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, SEQ]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 HOME: {{path: home.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 WINDOWS: {{path: windows.csv, key: [STUDYID], types: {{LO: int, HI: int}}}}\n\
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
    assert_eq!(failure["rule"], "LOOKUP-9");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["rows.home.derivations.WINDOW.source.between.value"])
    );
    assert_eq!(
        failure["context"],
        yamaa_adapters::json!({"identifier": "CLINIC.DAY", "variable": "WINDOWS.NAME"})
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
    assert_eq!(failure["rule"], "LOOKUP-9");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["intermediates.W.between.value"])
    );
    let fixture = Fixture::new(&measurements("W.NAME", "\"'x'\"", named), &inputs);
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
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 WINDOWS: {{path: windows.csv, key: [STUDYID], types: {{LO: int, HI: int}}}}\n\
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
        yamaa_adapters::json!(["rows.days.derivations.WINDOW.source.between.value"])
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
            "intermediates.P.key.APERIOD",
        ),
    ] {
        let header = "base: AE\n\
            intermediates:\n\
            \x20 - id: P\n\
            \x20   dataset: PERIODS\n\
            \x20   key: {APERIOD: CHOICE}\n";
        let fixture = Fixture::new(
            &specification(
                &header.edit("CHOICE", choice),
                // The second case reads the choice through the header alone.
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
                failure["rule"].as_str()
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

/// An aggregate a column derives evaluates for every row of every template,
/// so its range value may read the record of the driver of all of them
/// (LOOKUP-9): here the one template, driven by HOME, builds every row. A
/// clinic record is not a value of those rows.
#[test]
fn an_aggregate_range_value_reads_the_driver_of_every_template_row() {
    let specification = "yamaa: \"1.0\"\n\
        type: spec\n\
        domain: OUT\n\
        keys: [STUDYID, USUBJID, SEQ]\n\
        input:\n\
        \x20 CLINIC: {path: clinic.csv, types: {SEQ: int, DAY: int}}\n\
        \x20 HOME: {path: home.csv, types: {SEQ: int, DAY: int}}\n\
        \x20 WINDOWS: {path: windows.csv, key: [STUDYID], types: {LO: int, HI: int}}\n\
        output: {path: out.csv, columns: [STUDYID, USUBJID, SEQ, HOLDING]}\n\
        columns:\n\
        \x20 - {name: STUDYID, type: str}\n\
        \x20 - {name: USUBJID, type: str}\n\
        \x20 - {name: SEQ, type: int}\n\
        \x20 - name: HOLDING\n\
        \x20   type: int\n\
        \x20   derivation:\n\
        \x20     aggregate:\n\
        \x20       between: {value: HOME.DAY, lower: LO, upper: HI}\n\
        \x20       expr: \"COUNT(WINDOWS.*)\"\n\
        rows:\n\
        \x20 - id: home\n\
        \x20   dataset: HOME\n\
        \x20   derivations:\n\
        \x20     STUDYID: HOME.STUDYID\n\
        \x20     USUBJID: HOME.USUBJID\n\
        \x20     SEQ: HOME.SEQ\n";
    let inputs = [
        ("clinic.csv", CLINIC),
        ("home.csv", HOME),
        ("windows.csv", WINDOWS),
    ];
    let fixture = Fixture::new(specification, &inputs);
    let (csv, _) = fixture.saved();
    // Day 12 falls in LATE only; day 99 in no window, an empty count.
    assert_eq!(csv, "STUDYID,USUBJID,SEQ,HOLDING\nS,S-1,3,1\nS,S-1,4,\n");
    let fixture = Fixture::new(
        &specification.edit("value: HOME.DAY", "value: CLINIC.DAY"),
        &inputs,
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "unknown_field");
    assert_eq!(failure["rule"], "LOOKUP-9");
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["columns.HOLDING.derivation.aggregate.between.value"])
    );
    assert_eq!(failure["context"]["identifier"], "CLINIC.DAY");
}

/// A row template's range value has a match value's availability (LOOKUP-2)
/// and is a read of the template's derivation (SPEC-6): a row-local default
/// it names completes in the template's row phase, but a column only the
/// column phase derives, such as an aggregate, is not yet a value of the row.
/// `check` and the run agree on both.
#[test]
fn a_row_template_range_value_cannot_read_a_column_phase_column() {
    let rows = "rows:\n\
        \x20 - id: events\n\
        \x20   dataset: AE\n\
        \x20   derivations:\n\
        \x20     STUDYID: AE.STUDYID\n\
        \x20     USUBJID: AE.USUBJID\n\
        \x20     AESEQ: AE.AESEQ\n\
        \x20     APERIOD:\n\
        \x20       source:\n\
        \x20         variable: PERIODS.APERIOD\n\
        \x20         between: {value: ASTDT, lower: APERSDT, upper: APEREDT}\n";
    let specification = |start: &str| {
        format!(
            "yamaa: \"1.0\"\n\
             type: spec\n\
             domain: OUT\n\
             keys: [STUDYID, USUBJID, AESEQ]\n\
             input:\n\
             \x20 AE: {{path: ae.csv, key: [STUDYID, USUBJID, AESEQ], types: {{AESEQ: int, ASTDT: date}}}}\n\
             \x20 PERIODS: {{path: periods.csv, key: [STUDYID, USUBJID], types: {{APERIOD: int, APERSDT: date, APEREDT: date}}}}\n\
             output: {{path: out.csv, columns: [STUDYID, USUBJID, AESEQ, APERIOD]}}\n\
             columns:\n\
             \x20 - {{name: STUDYID, type: str}}\n\
             \x20 - {{name: USUBJID, type: str}}\n\
             \x20 - {{name: AESEQ, type: int}}\n\
             \x20 - {{name: ASTDT, type: date, derivation: {start}}}\n\
             \x20 - {{name: APERIOD, type: int}}\n\
             {rows}"
        )
    };
    let inputs = [("ae.csv", AE), ("periods.csv", PERIODS)];
    // ASTDT's row-local default completes in the row phase of `events`, so
    // each event takes the period holding its start date. The first day of
    // S-1's second period is in that period; no period holds an event before
    // S-1's first period, one without a start date, or one whose period has
    // no last day.
    let fixture = Fixture::new(&specification("AE.ASTDT"), &inputs);
    assert_eq!(agreeing(&fixture), Vec::<Value>::new());
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
    // An aggregate is not row local, so it completes only in the column phase.
    let fixture = Fixture::new(&specification("{aggregate: \"MIN(AE.ASTDT)\"}"), &inputs);
    let failure = fixture.failure();
    assert_eq!(
        (
            failure["phase"].as_str(),
            failure["condition"].as_str(),
            failure["rule"].as_str()
        ),
        (Some("validation"), Some("phase_boundary"), Some("COLUMN-4")),
        "{failure:?}"
    );
    assert_eq!(
        agreeing(&fixture),
        [yamaa_adapters::json!({
            "phase": "validation",
            "condition": "phase_boundary",
            "spec_paths": ["rows.events.derivations.APERIOD.source.between.value"],
            "context": {
                "identifier": "ASTDT",
                "row": "events",
                "available_phase": "column_derivation",
                "required_phase": "row_construction",
            },
        })]
    );
}

/// A range value may name a column a named intermediate selects for the row
/// (LOOKUP-9): each event takes the period holding its subject's reference
/// date, which the intermediate reads from another input.
#[test]
fn a_range_value_may_read_a_named_intermediate_selection() {
    let other = "STUDYID,USUBJID,REFDT\nS,S-1,2025-01-05\nS,S-2,2025-03-10\n";
    let header = "base: AE\n\
        intermediates:\n\
        \x20 - id: REF\n\
        \x20   dataset: OTHER\n\
        \x20   key: [STUDYID, USUBJID]\n";
    let column = PERIOD.edit("value: ASTDT", "value: REF.REFDT");
    let fixture = Fixture::new(
        &specification(header, &column, ", APERIOD").edit(
            "input:\n",
            "input:\n  OTHER: {path: other.csv, types: {REFDT: date}}\n",
        ),
        &[
            ("ae.csv", AE),
            ("periods.csv", PERIODS),
            ("other.csv", other),
        ],
    );
    let (csv, counts) = fixture.saved();
    // S-1's reference date falls in its first period; S-2's only period has
    // no last day, so no period holds its reference date.
    assert_eq!(
        csv,
        "STUDYID,USUBJID,AESEQ,APERIOD\n\
         S,S-1,1,1\n\
         S,S-1,2,1\n\
         S,S-1,3,1\n\
         S,S-1,4,1\n\
         S,S-2,1,\n\
         S,S-2,2,\n"
    );
    assert!(counts.contains(&("columns.APERIOD.derivation.source.absent".into(), 2)));
}

/// Exposure records, one of them without a dose. S-1 has two doses, one in
/// each period of `PERIODS`; S-2's one dose falls on the first day of a period
/// without a last day.
const DOSES: &str = "STUDYID,USUBJID,EXSEQ,EXDOSE,EXSTDT\n\
    S,S-1,1,0,2025-01-03\n\
    S,S-1,2,50,2025-01-10\n\
    S,S-1,3,50,2025-02-01\n\
    S,S-2,1,50,2025-03-01\n";

/// Rows built from the dosed exposure records, each taking the period that
/// holds a start date. `INPUTS` declares the inputs, `DOSED` the driving
/// intermediate, and `TEMPLATES` the row templates.
fn doses(inputs: &str, dosed: &str, templates: &str, period: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, ASEQ]\n\
         input:\n\
         {inputs}\
         \x20 EX: {{path: ex.csv, types: {{EXSEQ: int, EXDOSE: int, EXSTDT: date}}}}\n\
         \x20 PERIODS: {{path: periods.csv, key: [STUDYID, USUBJID], types: {{APERIOD: int, APERSDT: date, APEREDT: date}}}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, ASEQ, APERIOD]}}\n\
         intermediates:\n\
         \x20 - id: DOSED\n\
         \x20   dataset: EX\n\
         \x20   filter: \"EX.EXDOSE > 0\"\n\
         {dosed}\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: ASEQ, type: int}}\n\
         \x20 - name: APERIOD\n\
         \x20   type: int\n\
         {period}\
         rows:\n\
         {templates}"
    )
}

const DOSED_ROWS: &str = "  - id: dosed\n\
    \x20   dataset: DOSED\n\
    \x20   derivations:\n\
    \x20     STUDYID: DOSED.STUDYID\n\
    \x20     USUBJID: DOSED.USUBJID\n\
    \x20     ASEQ: DOSED.EXSEQ\n";

fn dosed_period(value: &str) -> String {
    format!(
        "{{source: {{variable: PERIODS.APERIOD, between: {{value: {value}, lower: APERSDT, upper: APEREDT}}}}}}"
    )
}

/// The qualifier of a named intermediate that drives the row reads the record
/// that built the row (LOOKUP-1, LOOKUP-2), so a range value qualified by it
/// is that record's value, never a keyed lookup of the intermediate, which
/// would find both of S-1's doses. Here the intermediate reads an input that
/// is not the first template's: the value is read on that input's own port,
/// and the subject input's same-named field is not read.
#[test]
fn a_range_value_qualified_by_its_driver_intermediate_reads_the_row_record() {
    let subjects = "STUDYID,USUBJID,EXSTDT\nS,S-1,2025-02-15\nS,S-2,2025-03-05\n";
    let templates = format!(
        "  - id: subject\n\
         \x20   dataset: SUBJ\n\
         \x20   derivations:\n\
         \x20     STUDYID: SUBJ.STUDYID\n\
         \x20     USUBJID: SUBJ.USUBJID\n\
         \x20     ASEQ: 0\n\
         \x20     APERIOD: null\n\
         {DOSED_ROWS}\
         \x20     APERIOD: {}\n",
        dosed_period("DOSED.EXSTDT")
    );
    let fixture = Fixture::new(
        &doses(
            "  SUBJ: {path: subjects.csv, types: {EXSTDT: date}}\n",
            "",
            &templates,
            "",
        ),
        &[
            ("subjects.csv", subjects),
            ("ex.csv", DOSES),
            ("periods.csv", PERIODS),
        ],
    );
    let (csv, counts) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,ASEQ,APERIOD\n\
         S,S-1,0,\n\
         S,S-2,0,\n\
         S,S-1,2,1\n\
         S,S-1,3,2\n\
         S,S-2,1,\n"
    );
    // S-2's dose finds no period with a last day, answered and counted.
    assert!(counts.contains(&("rows.dosed.derivations.APERIOD.source.absent".into(), 1)));
    // A grouped template reads the group key of the records that built the
    // row, and a column derived for the intermediate's rows reads their record.
    for (templates, period) in [
        (
            format!(
                "{}\
                 \x20     APERIOD: {}\n",
                DOSED_ROWS.edit(
                    "    derivations:\n",
                    "    group_by: [DOSED.STUDYID, DOSED.USUBJID, DOSED.EXSEQ, DOSED.EXSTDT]\n\
                     \x20   derivations:\n",
                ),
                dosed_period("DOSED.EXSTDT")
            ),
            String::new(),
        ),
        (
            DOSED_ROWS.to_owned(),
            format!("    derivation: {}\n", dosed_period("DOSED.EXSTDT")),
        ),
    ] {
        let fixture = Fixture::new(
            &doses("", "", &templates, &period),
            &[("ex.csv", DOSES), ("periods.csv", PERIODS)],
        );
        let (csv, _) = fixture.saved();
        assert_eq!(
            csv, "STUDYID,USUBJID,ASEQ,APERIOD\nS,S-1,2,1\nS,S-1,3,2\nS,S-2,1,\n",
            "{templates}{period}"
        );
    }
}

/// A field the driving intermediate derives is a value of the row (ROW-2), but
/// a range value reads only the stored record that built the row, so the read
/// is refused before any record is matched rather than looked up by key.
#[test]
fn a_range_value_naming_a_field_its_driver_intermediate_derives_is_unsupported() {
    let fixture = Fixture::new(
        &doses(
            "",
            "    derivations:\n\
             \x20     ADT: {type: date, derivation: EXSTDT}\n",
            &format!(
                "{DOSED_ROWS}\
                 \x20     APERIOD: {}\n",
                dosed_period("DOSED.ADT")
            ),
            "",
        ),
        &[("ex.csv", DOSES), ("periods.csv", PERIODS)],
    );
    let failure = fixture.failure();
    assert_eq!(
        (
            failure["phase"].as_str(),
            failure["condition"].as_str(),
            failure["rule"].is_null()
        ),
        (Some("planning"), Some("unsupported_operation"), true),
        "{failure:?}"
    );
    assert_eq!(
        failure["spec_paths"],
        yamaa_adapters::json!(["rows.dosed.derivations.APERIOD.source.between.value"])
    );
    assert_eq!(
        failure["context"]["operation"],
        "intermediate_driver_range_value"
    );
}

/// A column derivation evaluates only for the rows of the templates that
/// leave the column to the column phase, so a range value, a correlated
/// filter or a key match value it reads must name the driver of those
/// templates alone (LOOKUP-5, LOOKUP-8, LOOKUP-9). Here the clinic template
/// derives WINDOW itself and the home template leaves it to the column, which
/// reads HOME.DAY for home rows only.
#[test]
fn a_column_lookup_reads_the_driver_of_the_templates_that_leave_the_column() {
    let specification = |header: &str, derivation: &str, clinic: &str| {
        format!(
            "yamaa: \"1.0\"\n\
             type: spec\n\
             domain: OUT\n\
             keys: [STUDYID, USUBJID, SEQ]\n\
             input:\n\
             \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
             \x20 HOME: {{path: home.csv, types: {{SEQ: int, DAY: int}}}}\n\
             \x20 WINDOWS: {{path: windows.csv, key: [STUDYID], types: {{LO: int, HI: int}}}}\n\
             {header}\
             output: {{path: out.csv, columns: [STUDYID, USUBJID, SEQ, WINDOW]}}\n\
             columns:\n\
             \x20 - {{name: STUDYID, type: str}}\n\
             \x20 - {{name: USUBJID, type: str}}\n\
             \x20 - {{name: SEQ, type: int}}\n\
             \x20 - {{name: WINDOW, type: str, derivation: {derivation}}}\n\
             rows:\n\
             \x20 - id: clinic\n\
             \x20   dataset: CLINIC\n\
             \x20   derivations:\n\
             \x20     STUDYID: CLINIC.STUDYID\n\
             \x20     USUBJID: CLINIC.USUBJID\n\
             \x20     SEQ: CLINIC.SEQ\n\
             {clinic}\
             \x20 - id: home\n\
             \x20   dataset: HOME\n\
             \x20   derivations:\n\
             \x20     STUDYID: HOME.STUDYID\n\
             \x20     USUBJID: HOME.USUBJID\n\
             \x20     SEQ: HOME.SEQ\n"
        )
    };
    // A window that starts on the home day 12, for a key match on LO.
    let starts = "STUDYID,NAME,LO,HI\nS,EARLY,1,10\nS,LATE,12,50\n";
    let own = "      WINDOW: \"'CLINIC'\"\n";
    // `check` admits each range value against the same rows (LOOKUP-9); the
    // filter and the key are admitted by binding alone.
    for (header, derivation, path, windows, owner) in [
        (
            String::new(),
            window("HOME.DAY"),
            "columns.WINDOW.derivation.source.between.value",
            WINDOWS,
            Some(("variable", "WINDOWS.NAME")),
        ),
        (
            "intermediates:\n\
             \x20 - id: W\n\
             \x20   dataset: WINDOWS\n\
             \x20   key: [STUDYID]\n\
             \x20   between: {value: HOME.DAY, lower: LO, upper: HI}\n"
                .to_owned(),
            "W.NAME".to_owned(),
            "intermediates.W.between.value",
            WINDOWS,
            Some(("intermediate", "W")),
        ),
        (
            "intermediates:\n\
             \x20 - id: W\n\
             \x20   dataset: WINDOWS\n\
             \x20   key: [STUDYID]\n\
             \x20   filter: \"WINDOWS.LO <= HOME.DAY AND HOME.DAY <= WINDOWS.HI\"\n"
                .to_owned(),
            "W.NAME".to_owned(),
            "intermediates.W.filter",
            WINDOWS,
            None,
        ),
        (
            "intermediates:\n\
             \x20 - id: W\n\
             \x20   dataset: WINDOWS\n\
             \x20   key: {STUDYID: STUDYID, LO: HOME.DAY}\n"
                .to_owned(),
            "W.NAME".to_owned(),
            "intermediates.W.key",
            starts,
            None,
        ),
    ] {
        let inputs = [
            ("clinic.csv", CLINIC),
            ("home.csv", HOME),
            ("windows.csv", windows),
        ];
        let fixture = Fixture::new(&specification(&header, &derivation, own), &inputs);
        if owner.is_some() {
            assert_eq!(fixture.checked(), Vec::<Value>::new(), "{derivation}");
        }
        let (csv, _) = fixture.saved();
        // Day 12 falls in LATE; day 99 in no window.
        assert_eq!(
            csv,
            "STUDYID,USUBJID,SEQ,WINDOW\nS,S-1,1,CLINIC\nS,S-1,2,CLINIC\nS,S-1,3,LATE\nS,S-1,4,\n",
            "{derivation}"
        );
        // When the clinic template leaves WINDOW to the column too, a clinic
        // row has no home record.
        let fixture = Fixture::new(&specification(&header, &derivation, ""), &inputs);
        if let Some(owner) = owner {
            assert_eq!(
                agreeing(&fixture),
                [refusal(path, "HOME.DAY", owner)],
                "{derivation}"
            );
        }
        let failure = fixture.failure();
        assert_eq!(failure["condition"], "unknown_field", "{derivation}");
        assert_eq!(failure["spec_paths"], yamaa_adapters::json!([path]));
        assert_eq!(failure["context"]["identifier"], "HOME.DAY");
    }
}

/// Each subject's reference date in OTHER, an input that drives no event row.
const OTHER: &str = "STUDYID,USUBJID,REFDT\nS,S-1,2025-01-05\nS,S-2,2025-03-10\n";

fn with_other(specification: String) -> String {
    specification.edit(
        "input:\n",
        "input:\n  OTHER: {path: other.csv, types: {REFDT: date}}\n",
    )
}

/// The refusal of a range value that names no value of the rows that read
/// the lookup (LOOKUP-9), at the range's `between.value`, naming the value and
/// the lookup that declares it.
fn refusal(path: &str, identifier: &str, (owner, name): (&str, &str)) -> Value {
    yamaa_adapters::json!({
        "phase": "validation",
        "condition": "unknown_field",
        "spec_paths": [path],
        "context": {"identifier": identifier, (owner): name},
    })
}

/// An issue's phase, condition, specification paths and context.
fn finding(issue: &Value) -> Value {
    let mut object = yamaa_core::json::Map::new();
    for field in ["phase", "condition", "spec_paths", "context"] {
        object.insert(field.into(), issue[field].clone());
    }
    Value::Object(object)
}

/// The findings `check` reports, after asserting that the run reports exactly
/// the same issues, with the same rule.
#[track_caller]
fn agreeing(fixture: &Fixture) -> Vec<Value> {
    let checked = fixture.checked();
    assert_eq!(checked, fixture.issues(), "check and domain");
    checked.iter().map(finding).collect()
}

/// A named intermediate P whose range value is `value`, and Q, whose range
/// value reads P's selection for the same row.
fn named_ranges(value: &str) -> String {
    format!(
        "base: AE\n\
         intermediates:\n\
         \x20 - id: P\n\
         \x20   dataset: PERIODS\n\
         \x20   key: [STUDYID, USUBJID]\n\
         \x20   between: {{value: {value}, lower: APERSDT, upper: APEREDT}}\n\
         \x20   order_by: [PERIODS.APERIOD]\n\
         \x20   keep: first\n\
         \x20 - id: Q\n\
         \x20   dataset: PERIODS\n\
         \x20   key: [STUDYID, USUBJID]\n\
         \x20   between: {{value: P.APERSDT, lower: APERSDT, upper: APEREDT}}\n\
         \x20   order_by: [PERIODS.APERIOD]\n\
         \x20   keep: first\n"
    )
}

/// `check` reports a named intermediate's range value that names an input
/// driving no row that reads the intermediate, wherever binding does, from
/// the declarations alone (LOOKUP-9): whichever operation of a row reads the
/// intermediate, through another intermediate's, an aggregate's or a source
/// binding's range value, from an aggregate's derive binding, and from a
/// dataset verification.
#[test]
fn check_reports_an_intermediate_range_value_of_another_input_wherever_binding_does() {
    let inputs = [
        ("ae.csv", AE),
        ("periods.csv", PERIODS),
        ("other.csv", OTHER),
    ];
    let refused = refusal(
        "intermediates.P.between.value",
        "OTHER.REFDT",
        ("intermediate", "P"),
    );
    for read in [
        "{name: READ, type: int, derivation: P.APERIOD}",
        "{name: READ, type: int, derivation: 'P.APERIOD + 1'}",
        "{name: READ, type: str, derivation: {flag: 'P.APERIOD > 1'}}",
        "{name: READ, type: int, derivation: {case: [{when: 'P.APERIOD > 1', then: 1}]}}",
        "{name: READ, type: int, derivation: {case: [{when: 'AESEQ > 1', then: P.APERIOD}]}}",
        "{name: READ, type: int, derivation: {first_available: {sources: [P.APERIOD, AESEQ]}}}",
        "{name: READ, type: int, derivation: {greatest: {sources: [P.APERIOD, AESEQ]}}}",
        "{name: READ, type: str, derivation: {str_template: 'period {P.APERIOD}'}}",
        // Each of these runs P for the same row through a range value.
        "{name: READ, type: int, derivation: Q.APERIOD}",
        "{name: READ, type: int, derivation: {aggregate: {between: {value: P.APERSDT, upper: APEREDT}, expr: 'COUNT(PERIODS.*)'}}}",
        "{name: READ, type: int, derivation: {source: {variable: PERIODS.APERIOD, between: {value: P.APERSDT, lower: APERSDT}, order_by: [PERIODS.APERIOD], keep: first}}}",
        // A derive binding reads P's selection for the row.
        "{name: READ, type: int, derivation: {aggregate: {derivations: {START: {type: date, derivation: PERIODS.APERSDT}, FIRST: {type: date, derivation: P.APERSDT}}, expr: 'COUNT(FIRST)'}}}",
    ] {
        let fixture = Fixture::new(
            &with_other(specification(
                &named_ranges("OTHER.REFDT"),
                &format!("  - {read}\n"),
                ", READ",
            )),
            &inputs,
        );
        assert_eq!(agreeing(&fixture), std::slice::from_ref(&refused), "{read}");
    }
    // A dataset verification reads P for every row.
    let fixture = Fixture::new(
        &with_other(specification(
            &named_ranges("OTHER.REFDT"),
            "  - {name: READ, type: int, derivation: 1}\n\
             verifications:\n\
             \x20 - assert: {id: dated, require: 'P.APERIOD IS NOT NULL'}\n",
            ", READ",
        )),
        &inputs,
    );
    assert_eq!(agreeing(&fixture), std::slice::from_ref(&refused));
    // A range no row reads, and a value of the base driver's record, are
    // admitted.
    for (value, read) in [("OTHER.REFDT", "1"), ("AE.ASTDT", "P.APERIOD")] {
        let fixture = Fixture::new(
            &with_other(specification(
                &named_ranges(value),
                &format!("  - {{name: READ, type: int, derivation: {read}}}\n"),
                ", READ",
            )),
            &inputs,
        );
        assert_eq!(fixture.checked(), Vec::<Value>::new(), "{value}: {read}");
        fixture.saved();
    }
}

/// `check` reports a source binding's or a qualified aggregate's range value
/// that names an input driving no row, wherever binding does (LOOKUP-9): at
/// the top of a derivation, in a case result, among a coalescing selection's
/// sources and as a classified source.
#[test]
fn check_reports_a_join_or_aggregate_range_value_of_another_input_wherever_binding_does() {
    let inputs = [
        ("ae.csv", AE),
        ("periods.csv", PERIODS),
        ("other.csv", OTHER),
    ];
    let joined =
        "{variable: PERIODS.APERIOD, between: {value: OTHER.REFDT, lower: APERSDT, upper: APEREDT}}";
    let column = |kind: &str, derivation: &str| {
        with_other(specification(
            "base: AE\n",
            &format!("  - {{name: READ, type: {kind}, derivation: {derivation}}}\n"),
            ", READ",
        ))
    };
    for (kind, derivation, path) in [
        ("int", format!("{{source: {joined}}}"), "source"),
        (
            "int",
            format!("{{case: [{{when: 'AESEQ > 0', then: {{source: {joined}}}}}]}}"),
            "case[0].then.source",
        ),
        (
            "int",
            format!("{{first_available: {{sources: [{joined}, AESEQ]}}}}"),
            "first_available.sources[0]",
        ),
        (
            "str",
            format!("{{cut: {{source: {joined}, breaks: [1.5], labels: [first, later]}}}}"),
            "cut.source",
        ),
    ] {
        let fixture = Fixture::new(&column(kind, &derivation), &inputs);
        assert_eq!(
            agreeing(&fixture),
            [refusal(
                &format!("columns.READ.derivation.{path}.between.value"),
                "OTHER.REFDT",
                ("variable", "PERIODS.APERIOD"),
            )],
            "{derivation}"
        );
    }
    let counted =
        "{aggregate: {between: {value: OTHER.REFDT, upper: APEREDT}, expr: 'COUNT(PERIODS.*)'}}";
    for (derivation, path) in [
        (counted.to_owned(), "aggregate"),
        (
            format!("{{case: [{{when: 'AESEQ > 0', then: {counted}}}]}}"),
            "case[0].then.aggregate",
        ),
    ] {
        let fixture = Fixture::new(&column("int", &derivation), &inputs);
        let expected = yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "spec_paths": [format!("columns.READ.derivation.{path}.between.value")],
            "context": {"identifier": "OTHER.REFDT"},
        });
        assert_eq!(agreeing(&fixture), [expected], "{derivation}");
    }
}

/// With row templates, `check` admits a range value against the driver of
/// every row that reads it, as binding does (LOOKUP-9): a column derivation's
/// against the drivers of the templates that leave the column to it, a
/// dataset verification's against the drivers of every template, and a
/// template's derivations' and filter's against its own driver.
#[test]
fn check_admits_a_row_template_range_value_against_the_rows_that_read_it() {
    let inputs = [
        ("clinic.csv", CLINIC),
        ("home.csv", HOME),
        ("windows.csv", WINDOWS),
    ];
    let named = "intermediates:\n\
        \x20 - id: W\n\
        \x20   dataset: WINDOWS\n\
        \x20   key: [STUDYID]\n\
        \x20   between: {value: CLINIC.DAY, lower: LO, upper: HI}\n";
    let column_phase = |derivation: &str, header: &str| {
        measurements("NONE", "NONE", header)
            .replace("      WINDOW: NONE\n", "")
            .edit(
                "  - {name: WINDOW, type: str}\n",
                &format!("  - {{name: WINDOW, type: str, derivation: {derivation}}}\n"),
            )
    };
    let refused = refusal(
        "intermediates.W.between.value",
        "CLINIC.DAY",
        ("intermediate", "W"),
    );
    // A home row has no clinic record, however it reads W: through the column
    // phase or a dataset verification, as every row does, or through the home
    // template's derivations or filter.
    for specification in [
        column_phase("W.NAME", named),
        measurements("\"'x'\"", "\"'y'\"", named)
            + "verifications:\n  - assert: {id: windowed, require: 'W.NAME IS NOT NULL'}\n",
        measurements("\"'x'\"", "W.NAME", named),
        measurements("\"'x'\"", "{str_template: 'in {W.NAME}'}", named),
        measurements("\"'x'\"", "\"'y'\"", named).edit(
            "    dataset: HOME\n",
            "    dataset: HOME\n    filter: \"W.NAME IS NOT NULL\"\n",
        ),
    ] {
        let fixture = Fixture::new(&specification, &inputs);
        assert_eq!(
            agreeing(&fixture),
            std::slice::from_ref(&refused),
            "{specification}"
        );
    }
    // Read by the clinic template alone, W reads each row's own record.
    let fixture = Fixture::new(&measurements("W.NAME", "\"'y'\"", named), &inputs);
    assert_eq!(fixture.checked(), Vec::<Value>::new());
    let (csv, _) = fixture.saved();
    assert_eq!(
        csv,
        "STUDYID,USUBJID,SEQ,WINDOW\nS,S-1,1,EARLY\nS,S-1,2,LATE\nS,S-1,3,y\nS,S-1,4,y\n"
    );
    // A join's range in the home template, and one a column derives for the
    // rows of both templates.
    for (specification, path, value) in [
        (
            measurements("\"'x'\"", &window("CLINIC.DAY"), ""),
            "rows.home.derivations.WINDOW.source.between.value",
            "CLINIC.DAY",
        ),
        (
            column_phase(&window("HOME.DAY"), ""),
            "columns.WINDOW.derivation.source.between.value",
            "HOME.DAY",
        ),
    ] {
        let fixture = Fixture::new(&specification, &inputs);
        assert_eq!(
            agreeing(&fixture),
            [refusal(path, value, ("variable", "WINDOWS.NAME"))],
            "{specification}"
        );
    }
}
