#![cfg(feature = "yaml")]
//! Collected date text at run time. `dtc` reads its parts in order and stops
//! at the first missing one (TIME-19): a part after it is never read, so its
//! lookup cannot fail the run and its absence answer is not counted in the
//! run report, while a part the text reaches is read, counted and refused as
//! before.
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
            "yamaa-date-text-reads-{}-{}",
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
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "date-text-reads",
                specification,
                base_directory: ".",
            },
        )
    }
    /// The saved artifact and the run report's handler rows as
    /// `(spec path, count)`.
    fn saved(&self) -> (String, Vec<(String, i64)>) {
        let mut domain = self.run().unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        let report = fs::read_to_string(self.0.join("report.csv")).unwrap();
        let handlers = report
            .lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split(',').collect();
                (fields[1] == "handler").then(|| (fields[2].into(), fields[10].parse().unwrap()))
            })
            .collect();
        (
            fs::read_to_string(self.0.join("out.csv")).unwrap(),
            handlers,
        )
    }
    /// The run's only issue.
    fn failure(&self) -> Value {
        let issues = match self.run() {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        assert_eq!(issues.len(), 1, "{issues:?}");
        issues[0].as_value()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

/// Each named column assembles the base's year and month with the day it
/// names: `DAY` is a keyed intermediate answering absence with missing by
/// default, `DAYS` one declaring that answer as its `no_match` handler, and
/// `LK.D` an implicit join answering with missing under `absent`.
fn dtc(columns: &[(&str, &str)]) -> String {
    let names: String = columns
        .iter()
        .map(|(name, _)| format!(", {name}"))
        .collect();
    let mut text = format!(
        "schema_version: \"1.0\"\n\
         domain: XX\n\
         keys: [ID]\n\
         input:\n\
         \x20 RAW: {{path: raw.csv, types: {{ID: int}}}}\n\
         \x20 LK: {{path: lk.csv, types: {{ID: int}}}}\n\
         base: RAW\n\
         intermediates:\n\
         \x20 - {{id: DAY, dataset: LK, key: [ID]}}\n\
         \x20 - {{id: DAYS, dataset: LK, key: [ID], no_match: {{literal: null}}}}\n\
         output:\n\
         \x20 path: out.csv\n\
         \x20 report: report.csv\n\
         \x20 columns: [ID{names}]\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: RAW.ID}}\n"
    );
    for (name, day) in columns {
        text.push_str(&format!(
            "  - name: {name}\n\
             \x20   type: str\n\
             \x20   derivation:\n\
             \x20     dtc: {{year: RAW.Y, month: RAW.M, day: {day}}}\n"
        ));
    }
    text
}

/// Subject 2 has a year and no month, so no day is read for it.
const RAW: &str = "ID,Y,M\n1,2025,03\n2,2025,\n3,2025,04\n";

#[test]
fn a_day_after_a_missing_month_is_never_looked_up() {
    // The lookup table holds two records for subject 2, which the keyed
    // intermediate refuses only when a read reaches them.
    let lookup = "ID,D\n1,15\n2,10\n2,11\n3,20\n";
    let fixture = Fixture::new(
        &dtc(&[("DTC", "DAY.D")]),
        &[("raw.csv", RAW), ("lk.csv", lookup)],
    );
    let (csv, handlers) = fixture.saved();
    assert_eq!(csv, "ID,DTC\n1,2025-03-15\n2,2025\n3,2025-04-20\n");
    assert_eq!(handlers, []);
}

#[test]
fn an_absence_answer_after_a_missing_month_is_not_counted() {
    // Only subject 1 has a day record: subject 3 reaches its day and finds
    // none, while subject 2 never reads one.
    let fixture = Fixture::new(
        &dtc(&[("DFLT", "DAY.D"), ("DECL", "DAYS.D"), ("JOIN", "LK.D")]),
        &[("raw.csv", RAW), ("lk.csv", "ID,D\n1,15\n")],
    );
    let (csv, handlers) = fixture.saved();
    assert_eq!(
        csv,
        "ID,DFLT,DECL,JOIN\n\
         1,2025-03-15,2025-03-15,2025-03-15\n\
         2,2025,2025,2025\n\
         3,2025-04,2025-04,2025-04\n"
    );
    let mut handlers = handlers;
    handlers.sort();
    assert_eq!(
        handlers,
        [
            ("columns.DECL.derivation.dtc.day.no_match".into(), 1),
            ("columns.DFLT.derivation.dtc.day.no_match".into(), 1),
            ("columns.JOIN.derivation.dtc.day.absent".into(), 1),
        ]
    );
    // When only subject 2 lacks a day record, no row reaches an absence.
    let fixture = Fixture::new(
        &dtc(&[("DFLT", "DAY.D"), ("JOIN", "LK.D")]),
        &[("raw.csv", RAW), ("lk.csv", "ID,D\n1,15\n3,20\n")],
    );
    let (csv, handlers) = fixture.saved();
    assert_eq!(
        csv,
        "ID,DFLT,JOIN\n1,2025-03-15,2025-03-15\n2,2025,2025\n3,2025-04-20,2025-04-20\n"
    );
    // The implicit join's site is reported before any row fires it, and the
    // intermediate's when it first fires.
    assert_eq!(
        handlers,
        [("columns.JOIN.derivation.dtc.day.absent".into(), 0)]
    );
}

#[test]
fn a_reached_part_is_read_and_refused_as_before() {
    // With a month, subject 2's day is read, and its two records fail the run.
    let fixture = Fixture::new(
        &dtc(&[("DTC", "DAY.D")]),
        &[
            ("raw.csv", "ID,Y,M\n1,2025,03\n2,2025,05\n"),
            ("lk.csv", "ID,D\n1,15\n2,10\n2,11\n"),
        ],
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "multiple_matches");
    assert_eq!(failure["requirement"], "REQ-0127");
    assert_eq!(
        failure["context"]["keys"],
        yamaa_adapters::json!([{"ID": 2}])
    );
    // A day that is not two digits is refused once the month makes the text
    // reach it, and never read after a missing month.
    let fixture = Fixture::new(
        &dtc(&[("DTC", "LK.D")]),
        &[("raw.csv", RAW), ("lk.csv", "ID,D\n1,15\n2,5\n3,5\n")],
    );
    let failure = fixture.failure();
    assert_eq!(failure["condition"], "invalid_date_text");
    assert_eq!(failure["requirement"], "TIME-19");
    assert_eq!(
        failure["context"],
        yamaa_adapters::json!({"operation": "dtc", "part": "day", "source": "LK.D", "value": "5", "keys": [{"ID": 3}]})
    );
}
