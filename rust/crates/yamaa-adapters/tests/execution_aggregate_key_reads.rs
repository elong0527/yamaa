#![cfg(feature = "yaml")]
//! A matched aggregate reads the current row through its match values, its
//! range value and its derive bindings. A match value read from an input's
//! record names the driver of every row that evaluates it (LOOKUP-5). During
//! row construction, a range value or a derive binding that reads a window
//! result before the stage split completes it, and an unqualified aggregate
//! key, are not supported yet.
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        let root = crate::support::Scratch::new("aggregate-key-reads");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, text) in files {
            fs::write(root.join(name), text).unwrap();
        }
        Self(root)
    }
    fn identity(specification: &str) -> Identity<'_> {
        crate::support::study::identity("aggregate-key-reads", specification)
    }
    /// The saved artifact, or the issues that kept anything from being saved.
    fn run(&self) -> Result<String, Vec<Value>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let mut domain = crate::support::study::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Self::identity(specification),
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
    /// What `check` reports without reading data.
    fn checked(&self) -> Vec<Value> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Self::identity(specification),
        )
        .iter()
        .map(issue)
        .collect()
    }
}

fn issue(issue: &Diagnostic) -> Value {
    crate::support::study::issue_row(issue)
}

/// One `unknown_field` finding at a key pair that names `identifier` (LOOKUP-5).
fn refused(path: &str, identifier: &str) -> Result<String, Vec<Value>> {
    Err(vec![yamaa_adapters::json!({
        "phase": "validation",
        "condition": "unknown_field",
        "rule": "LOOKUP-5",
        "spec_paths": [path],
        "context": {"identifier": identifier}
    })])
}

/// SPEC-6 refuses an override of a column default that holds an aggregate.
fn overridden(row: &str) -> Result<String, Vec<Value>> {
    Err(vec![yamaa_adapters::json!({
        "phase": "validation",
        "condition": "duplicate_derivation",
        "rule": "SPEC-6",
        "spec_paths": ["columns.NCAL.derivation"],
        "context": {"column": "NCAL", "not_row_local": "aggregate", "rows": [row]}
    })])
}

/// One `unsupported_operation` finding at `path`.
fn unsupported(operation: &str, path: &str) -> Vec<Value> {
    vec![yamaa_adapters::json!({
        "phase": "planning",
        "condition": "unsupported_operation",
        "rule": null,
        "spec_paths": [path],
        "context": {"operation": operation}
    })]
}

/// Clinic and home measurements, each naming the device used, and the
/// calibration records of each device. The home file stores its fields in
/// another order, so a read at the wrong input's position reads another field.
const DEVICES: [(&str, &str); 3] = [
    ("clinic.csv", "ID,DEVICE\na,D1\nb,D2\n"),
    ("home.csv", "DEVICE,ID\nD2,a\n"),
    ("calib.csv", "DEVICE,DAY\nD1,1\nD1,2\nD2,3\n"),
];

#[test]
fn an_aggregate_key_reads_the_driver_of_every_row_that_evaluates_it() {
    // Rows from clinic records, then from home records; NCAL counts the
    // calibrations of the device the match value names. `clinic` and `home`
    // add derivations to each template.
    let specification = |ncal: &str, clinic: &str, home: Option<&str>| {
        let mut text = format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: VS\nkeys: [ID, ORIGIN]\n\
             input:\n  CLINIC: clinic.csv\n  HOME: home.csv\n  \
             CALIB: {{path: calib.csv, types: {{DAY: int}}}}\n\
             output: {{path: out.csv, columns: [ID, ORIGIN, NCAL]}}\n\
             columns:\n  - {{name: ID, type: str}}\n  - {{name: ORIGIN, type: str}}\n  \
             - {{name: DEVICE, type: str}}\n{ncal}\
             rows:\n  - id: clinic\n    dataset: CLINIC\n    derivations:\n      \
             ID: CLINIC.ID\n      ORIGIN: \"'CLINIC'\"\n      DEVICE: CLINIC.DEVICE\n{clinic}"
        );
        if let Some(home) = home {
            text.push_str(&format!(
                "  - id: home\n    dataset: HOME\n    derivations:\n      ID: HOME.ID\n      \
                 ORIGIN: \"'HOME'\"\n      DEVICE: null\n{home}"
            ));
        }
        text
    };
    // The count as a column derivation, or nested in a `case`.
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
    let own = "      NCAL: 0\n";
    let run = |text: String| Fixture::new(&text, &DEVICES).run();
    // Home rows have no clinic record to read the device from, and clinic
    // rows no home record; either key is reported at its pair.
    assert_eq!(
        run(specification(&column("CLINIC.DEVICE"), "", Some(""))),
        refused(
            "columns.NCAL.derivation.aggregate.key.DEVICE",
            "CLINIC.DEVICE"
        )
    );
    assert_eq!(
        run(specification(&column("HOME.DEVICE"), "", Some(""))),
        refused(
            "columns.NCAL.derivation.aggregate.key.DEVICE",
            "HOME.DEVICE"
        )
    );
    assert_eq!(
        run(specification(&nested("CLINIC.DEVICE"), "", Some(""))),
        refused(
            "columns.NCAL.derivation.case[0].then.aggregate.key.DEVICE",
            "CLINIC.DEVICE"
        )
    );
    // With no home template, every row can read the clinic record's device.
    // SPEC-6 does not let a template override a column default that holds an
    // aggregate, including one nested in a case result (D132).
    let counted = "ID,ORIGIN,NCAL\na,CLINIC,2\nb,CLINIC,1\n";
    assert_eq!(
        run(specification(&column("CLINIC.DEVICE"), "", None)),
        Ok(counted.into())
    );
    assert_eq!(
        run(specification(&nested("CLINIC.DEVICE"), "", None)),
        Ok(counted.into())
    );
    assert_eq!(
        run(specification(&nested("CLINIC.DEVICE"), "", Some(own))),
        overridden("home")
    );
    assert_eq!(
        run(specification(&column("DEVICE"), "", Some(""))),
        Ok(format!("{counted}a,HOME,\n"))
    );
    // A home-record key, directly or inside an expression, also cannot make
    // a nested aggregate default eligible for a clinic-template override.
    for key in [
        "HOME.DEVICE",
        "{str_case: {source: HOME.DEVICE, to: upper}}",
    ] {
        assert_eq!(
            run(specification(&nested(key), own, Some(""))),
            overridden("clinic"),
            "{key}"
        );
    }
}

#[test]
fn without_row_templates_an_aggregate_key_reads_the_base_record() {
    let specification = |key: &str| {
        format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: VS\nkeys: [ID]\nbase: CLINIC\n\
             input:\n  CLINIC: clinic.csv\n  HOME: home.csv\n  \
             CALIB: {{path: calib.csv, types: {{DAY: int}}}}\n\
             output: {{path: out.csv, columns: [ID, NCAL]}}\n\
             columns:\n  - {{name: ID, type: str, derivation: CLINIC.ID}}\n\
             \x20 - name: NCAL\n    type: int\n    derivation:\n      aggregate:\n        \
             expr: 'COUNT(CALIB.DAY)'\n        key: {{DEVICE: {key}}}\n"
        )
    };
    let run = |text: String| Fixture::new(&text, &DEVICES).run();
    assert_eq!(
        run(specification("CLINIC.DEVICE")),
        Ok("ID,NCAL\na,2\nb,1\n".into())
    );
    // Every row is built from a clinic record, which no home record is.
    assert_eq!(
        run(specification("HOME.DEVICE")),
        refused(
            "columns.NCAL.derivation.aggregate.key.DEVICE",
            "HOME.DEVICE"
        )
    );
}

#[test]
fn an_aggregate_over_the_first_input_serves_rows_another_input_builds() {
    // Only home rows evaluate NCAL: it counts the clinic records of the
    // row's subject, a reduction of the first template's input.
    let specification = "yamaa: \"1.0\"\ntype: spec\ndomain: VS\nkeys: [ID, ORIGIN]\n\
        input:\n  CLINIC: clinic.csv\n  HOME: home.csv\n\
        output: {path: out.csv, columns: [ID, ORIGIN, NCAL]}\n\
        columns:\n  - {name: ID, type: str}\n  - {name: ORIGIN, type: str}\n\
        \x20 - name: NCAL\n    type: int\n    derivation:\n      case:\n        \
        - when: ORIGIN = 'HOME'\n          then:\n            aggregate:\n              \
        expr: 'COUNT(CLINIC.DEVICE)'\n              key: {ID: ID}\n        - otherwise: 0\n\
        rows:\n  - id: clinic\n    dataset: CLINIC\n    derivations:\n      \
        ID: CLINIC.ID\n      ORIGIN: \"'CLINIC'\"\n\
        \x20 - id: home\n    dataset: HOME\n    derivations:\n      ID: HOME.ID\n      \
        ORIGIN: \"'HOME'\"\n";
    assert_eq!(
        Fixture::new(specification, &DEVICES).run(),
        Ok("ID,ORIGIN,NCAL\na,CLINIC,0\nb,CLINIC,0\na,HOME,1\n".into())
    );
}

/// Without row templates, the window key SEQ gives each record its own row;
/// `columns` follows ID and SEQ and holds the keys. SRC declares the key that
/// an aggregate over it without a key of its own matches on.
fn window_key(keys: &str, intermediates: &str, columns: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID, SEQ, {keys}]\nbase: SRC\n\
         input:\n  SRC: {{path: source.csv, key: [ID], types: {{ID: int, VAL: int, LO: int, HI: int, W: int}}}}\n  \
         CAL: {{path: cal.csv, types: {{SEQN: int, LO: int, HI: int, W: int}}}}\n\
         output: {{path: out.csv, columns: [ID, SEQ, {keys}]}}\n{intermediates}\
         columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
         window: {{group_by: [ID], order_by: [SRC.VAL]}}\n{columns}"
    )
}

/// Two records of ID 1 and one of ID 2; SEQ numbers them 1 and 2, then 1.
const WINDOWED: [(&str, &str); 2] = [
    (
        "source.csv",
        "ID,VAL,LO,HI,W\n1,3,1,1,10\n1,5,2,9,20\n2,4,1,9,30\n",
    ),
    ("cal.csv", "SEQN,LO,HI,W\n1,1,1,10\n1,1,5,5\n2,2,2,20\n"),
];

#[test]
fn an_aggregate_range_read_before_its_window_completes_is_unsupported() {
    let ranged = |value: &str| {
        format!(
            "  - name: WT\n    type: int\n    derivation:\n      aggregate:\n        \
             expr: 'SUM(SRC.W)'\n        between: {{value: {value}, lower: LO, upper: HI}}\n"
        )
    };
    let run = |columns: &str| Fixture::new(&window_key("WT", "", columns), &WINDOWED).run();
    // WT is computed before the window pass that completes SEQ.
    assert_eq!(
        run(&ranged("SEQ")),
        Err(unsupported(
            "aggregate_range_window_read",
            "columns.WT.derivation.aggregate.between.value"
        ))
    );
    // WT sums the weights of the records of the row's ID (the key SRC
    // declares) whose bounds hold the range value. A value the record holds
    // before the window pass builds: ID 1's VAL 3 and VAL 5 fall in 2..9
    // (20), not in 1..1, and ID 2's VAL 4 in 1..9 (30).
    assert_eq!(
        run(&ranged("SRC.VAL")),
        Ok("ID,SEQ,WT\n1,1,20\n1,2,20\n2,1,30\n".into())
    );
    // The stage split moves WT to the window pass when a `case` condition
    // around the aggregate reads SEQ, or when its key does; WT then reads the
    // completed SEQ. SEQ 1 of ID 1 falls in 1..1 (10), SEQ 2 in 2..9 (20),
    // and SEQ 1 of ID 2 in 1..9 (30).
    let guarded = "  - name: WT\n    type: int\n    derivation:\n      case:\n        \
        - when: SEQ > 0\n          then:\n            aggregate:\n              \
        expr: 'SUM(SRC.W)'\n              between: {value: SEQ, lower: LO, upper: HI}\n";
    assert_eq!(
        run(guarded),
        Ok("ID,SEQ,WT\n1,1,10\n1,2,20\n2,1,30\n".into())
    );
    // Matched on SEQN = SEQ: SEQ 1 sums the calibrations 10 and 5, whose
    // bounds 1..1 and 1..5 hold 1, and SEQ 2 the one of 20.
    let keyed = "  - name: WT\n    type: int\n    derivation:\n      aggregate:\n        \
        expr: 'SUM(CAL.W)'\n        key: {SEQN: SEQ}\n        \
        between: {value: SEQ, lower: LO, upper: HI}\n";
    assert_eq!(run(keyed), Ok("ID,SEQ,WT\n1,1,15\n1,2,20\n2,1,15\n".into()));
}

#[test]
fn an_aggregate_derive_read_before_its_window_completes_is_unsupported() {
    // K selects the lightest calibration of the row's SEQ; each record of the
    // row's ID contributes the greater of its weight and K's.
    let intermediate = "intermediates:\n  - id: K\n    dataset: CAL\n    key: {SEQN: SEQ}\n    \
        order_by: [CAL.W]\n    keep: first\n";
    let aggregate = "aggregate:\n  key: [ID]\n  derivations:\n    \
        D: {type: int, derivation: SRC.W}\n    \
        V: {type: int, derivation: K.W}\n    \
        T: {type: int, derivation: {greatest: {sources: [D, V]}}}\n  \
        expr: 'SUM(T)'\n";
    let indented = |text: &str, depth: usize| -> String {
        text.lines()
            .map(|line| format!("{}{line}\n", " ".repeat(depth)))
            .collect()
    };
    let run =
        |columns: String| Fixture::new(&window_key("WT", intermediate, &columns), &WINDOWED).run();
    let direct = format!(
        "  - name: WT\n    type: int\n    derivation:\n{}",
        indented(aggregate, 6)
    );
    assert_eq!(
        run(direct),
        Err(unsupported(
            "aggregate_derive_window_read",
            "columns.WT.derivation.aggregate.derivations.V.derivation.source"
        ))
    );
    // Behind a `case` condition that reads SEQ, WT is computed in the window
    // pass. SEQ 1 selects weight 5 and SEQ 2 weight 20: ID 1's weights 10
    // and 20 give 30 for SEQ 1 and 40 for SEQ 2, and ID 2's 30 gives 30.
    let guarded = format!(
        "  - name: WT\n    type: int\n    derivation:\n      case:\n        \
         - when: SEQ > 0\n          then:\n{}",
        indented(aggregate, 12)
    );
    assert_eq!(
        run(guarded),
        Ok("ID,SEQ,WT\n1,1,30\n1,2,40\n2,1,30\n".into())
    );
}

#[test]
fn an_unqualified_aggregate_key_beside_a_window_key_is_unsupported() {
    // N counts the rows of the row's ID, which row construction completes
    // only after every key.
    let fixture = Fixture::new(
        &window_key(
            "N",
            "",
            "  - name: N\n    type: int\n    derivation:\n      aggregate:\n        \
             group_by: [ID]\n        expr: 'COUNT(SEQ)'\n",
        ),
        &WINDOWED,
    );
    let refused = unsupported("output_aggregate_key", "columns.N.derivation.aggregate");
    assert_eq!(fixture.checked(), refused);
    assert_eq!(fixture.run(), Err(refused));
}
