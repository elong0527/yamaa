#![cfg(feature = "yaml")]
//! A matched aggregate reads the current row through its match values and
//! its range value. A match value read from an input's record names the
//! driver of every row that evaluates it (LOOKUP-5); a range value read
//! before the template's window pass completes it is not supported yet.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-aggregate-key-reads-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, text) in files {
            fs::write(root.join(name), text).unwrap();
        }
        Self(root)
    }
    /// The saved artifact, or the issues that kept anything from being saved.
    fn run(&self) -> Result<String, Vec<Value>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let mut domain = file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "aggregate-key-reads",
                specification,
                base_directory: ".",
            },
        )
        .map_err(|failure| failure.iter().map(issue).collect::<Vec<_>>())?;
        let issues = domain.issues().iter().map(issue).collect::<Vec<_>>();
        if !issues.is_empty() {
            assert!(domain.save().is_err());
            return Err(issues);
        }
        assert!(domain.save().unwrap());
        Ok(fs::read_to_string(self.0.join("out.csv")).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn issue(issue: &Issue) -> Value {
    yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
}

#[test]
fn an_aggregate_key_reads_the_driver_of_every_row_that_evaluates_it() {
    // Rows from clinic records, then from home records; NCAL counts the
    // calibrations of the device the match value names. `clinic` and `home`
    // add derivations to each template.
    let specification = |ncal: &str, clinic: &str, home: Option<&str>| {
        let mut text = format!(
            "schema_version: '1.0'\ndomain: VS\nkeys: [ID, ORIGIN]\n\
             input:\n  CLINIC: clinic.csv\n  HOME: home.csv\n  \
             CALIB: {{path: calib.csv, types: {{DAY: int}}}}\n\
             output: {{path: out.csv, columns: [ID, ORIGIN, NCAL]}}\n\
             columns:\n  - {{name: ID, type: str}}\n  - {{name: ORIGIN, type: str}}\n  \
             - {{name: DEVICE, type: str}}\n{ncal}\
             rows:\n  - id: clinic\n    dataset: CLINIC\n    derivations:\n      \
             ID: CLINIC.ID\n      ORIGIN: {{literal: CLINIC}}\n      DEVICE: CLINIC.DEVICE\n{clinic}"
        );
        if let Some(home) = home {
            text.push_str(&format!(
                "  - id: home\n    dataset: HOME\n    derivations:\n      ID: HOME.ID\n      \
                 ORIGIN: {{literal: HOME}}\n      DEVICE: {{literal: null}}\n{home}"
            ));
        }
        text
    };
    // The count as a column derivation, or nested in a `case` that a row
    // template may derive itself instead.
    let column = |key: &str| {
        format!(
            "  - name: NCAL\n    type: int\n    derivation:\n      aggregate:\n        \
             expr: 'COUNT(CALIB.DAY)'\n        key: {{DEVICE: {key}}}\n"
        )
    };
    let nested = |key: &str| {
        format!(
            "  - name: NCAL\n    type: int\n    derivation:\n      case:\n        \
             - when: ID IS NOT NULL\n          then:\n            aggregate:\n              \
             expr: 'COUNT(CALIB.DAY)'\n              key: {{DEVICE: {key}}}\n"
        )
    };
    let own = "      NCAL: {literal: 0}\n";
    let files = [
        ("clinic.csv", "ID,DEVICE\na,D1\nb,D2\n"),
        ("home.csv", "ID,DEVICE\na,D2\n"),
        ("calib.csv", "DEVICE,DAY\nD1,1\nD1,2\nD2,3\n"),
    ];
    let refused = |path: &str, identifier: &str| {
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "requirement": "LOOKUP-5",
            "spec_paths": [path],
            "context": {"identifier": identifier}
        })])
    };
    let run = |text: String| Fixture::new(&text, &files).run();
    // Home rows have no clinic record to read the device from.
    assert_eq!(
        run(specification(&column("CLINIC.DEVICE"), "", Some(""))),
        refused("columns.NCAL.derivation.aggregate.key.DEVICE", "CLINIC.DEVICE")
    );
    assert_eq!(
        run(specification(&nested("CLINIC.DEVICE"), "", Some(""))),
        refused(
            "columns.NCAL.derivation.case[0].then.aggregate.key.DEVICE",
            "CLINIC.DEVICE"
        )
    );
    // When only clinic rows evaluate the count, the clinic record names one
    // device: with no home template, or when the home template derives NCAL
    // itself. A column every template derives serves both templates.
    let counted = "ID,ORIGIN,NCAL\na,CLINIC,2\nb,CLINIC,1\n";
    assert_eq!(
        run(specification(&column("CLINIC.DEVICE"), "", None)),
        Ok(counted.into())
    );
    assert_eq!(
        run(specification(&nested("CLINIC.DEVICE"), "", Some(own))),
        Ok(format!("{counted}a,HOME,0\n"))
    );
    assert_eq!(
        run(specification(&column("DEVICE"), "", Some(""))),
        Ok(format!("{counted}a,HOME,\n"))
    );
    // A column derivation reads the primary input's record as its current
    // record, so a key naming the home record names no current-row value,
    // even where only home rows evaluate it. A plain read of the home record
    // there fails the same way (NAME-3).
    assert_eq!(
        run(specification(&nested("HOME.DEVICE"), own, Some(""))),
        refused(
            "columns.NCAL.derivation.case[0].then.aggregate.key.DEVICE",
            "HOME.DEVICE"
        )
    );
}

#[test]
fn an_aggregate_range_read_before_its_window_completes_is_unsupported() {
    // Without row templates, the window key SEQ gives each record its own
    // row, and the key WT sums the weights of the records with the row's ID
    // (the applicable output key SRC carries) whose bounds hold the range
    // value.
    let specification = |value: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, SEQ, WT]\n\
             input: {{SRC: {{path: source.csv, types: {{ID: int, VAL: int, LO: int, HI: int, W: int}}}}}}\n\
             output: {{path: out.csv, columns: [ID, SEQ, WT]}}\n\
             columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             \x20 - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
             window: {{group_by: [ID], order_by: [SRC.VAL]}}\n\
             \x20 - name: WT\n    type: int\n    derivation:\n      aggregate:\n        \
             expr: 'SUM(SRC.W)'\n        between: {{value: {value}, lower: LO, upper: HI}}\n"
        )
    };
    let files = [(
        "source.csv",
        "ID,VAL,LO,HI,W\n1,3,1,3,10\n1,5,4,9,20\n2,4,4,4,30\n",
    )];
    // SEQ completes only in the window pass, after the key that reads it.
    let fixture = Fixture::new(&specification("SEQ"), &files);
    assert_eq!(
        fixture.run(),
        Err(vec![yamaa_adapters::json!({
            "phase": "planning",
            "condition": "unsupported_operation",
            "requirement": null,
            "spec_paths": ["columns.WT.derivation.aggregate.between.value"],
            "context": {"operation": "aggregate_range_window_read"}
        })])
    );
    // A range value the record holds before the window pass builds. Among
    // ID 1's records VAL 3 falls only in 1..3 (weight 10) and VAL 5 only in
    // 4..9 (20); ID 2's VAL 4 falls in its own 4..4 (30), and ID 1's 4..9
    // is not read for it.
    let fixture = Fixture::new(&specification("SRC.VAL"), &files);
    assert_eq!(fixture.run(), Ok("ID,SEQ,WT\n1,1,10\n1,2,20\n2,1,30\n".into()));
}
