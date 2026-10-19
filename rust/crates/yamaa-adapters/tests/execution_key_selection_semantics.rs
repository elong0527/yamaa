#![cfg(feature = "yaml")]
//! How a run reads the records behind a value. Without row templates, a
//! column counts the distinct raw readings of a key combination's records
//! (COLUMN-2), and a source's `order_by` and `keep` keep one present reading
//! instead, ties in record order, counting only real choices (COLUMN-12,
//! COLUMN-14). Records missing a key stay apart until the output key gate
//! (OUTPUT-3). A lookup evaluates every match value before it reads a record
//! and compares calendar values and numbers exactly (LOOKUP-25, VALUE-19,
//! VALUE-21). A pattern read from data is checked on every record or row its
//! predicate is asked about (FORMULA-7).
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, json, specification_report::Identity};
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-key-selection-semantics-{}-{}",
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
    /// The saved artifact and the run's handler counts, or the issues that
    /// kept the artifact from being saved; a failed run still saves its
    /// declared report.
    fn run(&self) -> Result<(String, Value), Vec<Value>> {
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
                example: "key-selection-semantics",
                specification,
                base_directory: ".",
            },
        )
        .map_err(|failure| failure.iter().map(issue).collect::<Vec<_>>())?;
        let issues = domain.issues().iter().map(issue).collect::<Vec<_>>();
        if !issues.is_empty() {
            assert!(domain.save().is_err());
            assert!(!self.0.join("out.csv").exists());
            return Err(issues);
        }
        assert!(domain.save().unwrap());
        Ok((
            fs::read_to_string(self.0.join("out.csv")).unwrap(),
            domain.observations()["handler_counts"].clone(),
        ))
    }
    /// The saved artifact of a run that succeeds.
    fn saved(&self) -> String {
        self.run().unwrap_or_else(|issues| panic!("{issues:?}")).0
    }
    /// The check rows of the saved run report.
    fn report_checks(&self) -> Vec<String> {
        fs::read_to_string(self.0.join("report.csv"))
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("out.csv,check,"))
            .map(String::from)
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn issue(issue: &Issue) -> Value {
    json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
}

/// The head of a specification without row templates whose one input SRC
/// reads `src.csv`, declaring `types` when there are any.
fn keyed(keys: &str, types: &str, columns: &str, rest: &str) -> String {
    let input = if types.is_empty() {
        "src.csv".to_owned()
    } else {
        format!("{{path: src.csv, types: {{{types}}}}}")
    };
    format!(
        "schema_version: '1.0'\ndomain: OUT\nkeys: [{keys}]\ninput: {{SRC: {input}}}\n\
         output: {{path: out.csv, columns: [{columns}]{rest}}}\n"
    )
}

#[test]
fn a_require_pattern_read_from_data_fails_although_when_exempts_the_row() {
    // `require` is evaluated on every row whatever `when` answers (CHECK-12),
    // so the second row's pattern `tail!`, which ends in its escape, fails at
    // the assertion. The row count checked before it held, and the report
    // keeps that check.
    let fixture = Fixture::new(
        &format!(
            "{}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             \x20 - {{name: P, type: str, derivation: SRC.P}}\n\
             verifications:\n  - row_count: {{min: 2, max: 2}}\n\
             \x20 - assert: {{when: 'FALSE', require: \"'text' LIKE P ESCAPE '!'\"}}\n",
            keyed("ID", "ID: int", "ID, P", ", report: report.csv"),
        ),
        &[("src.csv", "ID,P\n1,ok\n2,tail!\n")],
    );
    assert_eq!(
        fixture.run(),
        Err(vec![json!({
            "phase": "validation", "condition": "invalid_expression", "requirement": "REQ-0191",
            "spec_paths": ["verifications[1].assert.require"],
            "context": {"reason": "LIKE pattern has a dangling escape"}
        })])
    );
    assert_eq!(
        fixture.report_checks(),
        ["out.csv,check,verifications[0].row_count,row_count,,,REQ-0385,error,held,,0,1,,{}"]
    );
}

#[test]
fn a_source_filter_is_asked_about_every_record_of_the_key() {
    // The first record already passes the filter and both carry the same
    // value, yet the second record's pattern `!` ends in its escape: the
    // filter decides every record before the value is read (COLUMN-2).
    let fixture = Fixture::new(
        &format!(
            "{}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             \x20 - name: V\n    type: str\n    derivation:\n      \
             source: {{variable: SRC.V, filter: \"'x' LIKE SRC.P ESCAPE '!'\"}}\n",
            keyed("ID", "ID: int", "ID, V", ""),
        ),
        &[("src.csv", "ID,V,P\n1,unread,%\n1,unread,!\n")],
    );
    assert_eq!(
        fixture.run(),
        Err(vec![json!({
            "phase": "validation", "condition": "invalid_expression", "requirement": "REQ-0191",
            "spec_paths": ["columns.V.derivation.source.filter"],
            "context": {"reason": "LIKE pattern has a dangling escape"}
        })])
    );
}

#[test]
fn a_key_counts_every_distinct_raw_reading_before_conversion() {
    // V is text: `07` and `7` are two readings although both convert to the
    // int 7, the repeated `8` is one, and the missing reading is none.
    let fixture = Fixture::new(
        &format!(
            "{}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             \x20 - {{name: V, type: int, derivation: SRC.V}}\n",
            keyed("ID", "ID: int", "ID, V", ""),
        ),
        &[("src.csv", "ID,V\n1,07\n1,7\n1,8\n1,\n1,8\n")],
    );
    assert_eq!(
        fixture.run(),
        Err(vec![json!({
            "phase": "derivation", "condition": "multiple_values_per_key", "requirement": "REQ-0075",
            "spec_paths": ["columns.V.derivation.source"],
            "context": {"identifier": "SRC.V", "value_count": 3, "keys": [{"ID": 1}]}
        })])
    );
}

#[test]
fn records_missing_a_key_reach_the_output_gate_without_collapsing() {
    // The two records missing ID stay two rows, so their different values
    // never meet in one column; the output gate reports both the missing key
    // and their duplicate combination. The text `__missing_key__` is a value
    // like any other, never a missing key.
    let fixture = Fixture::new(
        &format!(
            "{}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             \x20 - {{name: TAG, type: str, derivation: SRC.TAG}}\n\
             \x20 - {{name: V, type: int, derivation: SRC.V}}\n",
            keyed("TAG, ID", "", "ID, TAG, V", ""),
        ),
        &[(
            "src.csv",
            "ID,TAG,V\n1,__missing_key__,7\n,present,8\n,present,9\n",
        )],
    );
    let missing = json!({"TAG": "present", "ID": null});
    assert_eq!(
        fixture.run(),
        Err(vec![
            json!({
                "phase": "output", "condition": "missing_key", "requirement": "REQ-0240",
                "spec_paths": ["keys[1]"],
                "context": {"column": "ID", "missing_count": 2, "keys": [missing.clone(), missing.clone()]}
            }),
            json!({
                "phase": "output", "condition": "duplicate_key", "requirement": "REQ-0240",
                "spec_paths": ["keys"],
                "context": {"duplicate_count": 1, "keys": [missing]}
            }),
        ])
    );
}

/// V reads SRC.V from the record of its ID that `keep` chooses by ORDER.
fn ordered(direction: &str, nulls: &str, keep: &str) -> String {
    format!(
        "{}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - name: V\n    type: int\n    derivation:\n      source:\n        \
         variable: SRC.V\n        \
         order_by: [{{variable: SRC.ORDER, direction: {direction}, nulls: {nulls}}}]\n        \
         keep: {keep}\n",
        keyed("ID", "ID: int, ORDER: int", "ID, V", ""),
    )
}

#[test]
fn source_order_keeps_a_present_reading_stably_and_counts_real_choices() {
    // ORDER compares exactly beyond 2^53, a missing ORDER sorts where `nulls`
    // places it in either direction, and the record missing V is no
    // candidate. One key makes one choice, whichever record it keeps.
    let records = "ID,V,ORDER\n1,7,9007199254740993\n1,8,9007199254740992\n1,9,\n1,,\n";
    let counted = |count: u32| {
        json!([{
            "spec_path": "columns.V.derivation.source.multiple_matches",
            "handler": "multiple_matches", "count": count
        }])
    };
    for (direction, nulls, first, last) in [
        ("asc", "last", 8, 9),
        ("desc", "last", 7, 9),
        ("asc", "first", 9, 7),
        ("desc", "first", 9, 8),
    ] {
        for (keep, expected) in [("first", first), ("last", last)] {
            let fixture = Fixture::new(&ordered(direction, nulls, keep), &[("src.csv", records)]);
            assert_eq!(
                fixture.run(),
                Ok((format!("ID,V\n1,{expected}\n"), counted(1))),
                "{direction} {nulls} {keep}"
            );
        }
    }
    let fixture = Fixture::new(
        &ordered("asc", "last", "last"),
        &[("src.csv", &format!("{records}2,10,0\n2,11,1\n"))],
    );
    assert_eq!(fixture.run(), Ok(("ID,V\n1,9\n2,11\n".into(), counted(2))));
    // Records tied on ORDER keep their record order.
    for (keep, expected) in [("first", 7), ("last", 9)] {
        let fixture = Fixture::new(
            &ordered("desc", "first", keep),
            &[("src.csv", "ID,V,ORDER\n1,7,0\n1,8,0\n1,9,0\n")],
        );
        assert_eq!(fixture.saved(), format!("ID,V\n1,{expected}\n"), "{keep}");
    }
}

#[test]
fn match_values_compare_as_calendar_values_and_exact_numbers() {
    // The date completed from `2025` keeps collected precision year and the
    // datetime completed from a day keeps precision day, while the donor's
    // were collected in full; the float key is negative zero, the donor's
    // positive. Each pair is still equal (VALUE-21, VALUE-19).
    let fixture = Fixture::new(
        "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\n\
         input:\n  SRC: {path: src.csv, types: {ID: int, F: float}}\n  \
         TAB: {path: tab.csv, types: {DAY: date, MOMENT: datetime, NUM: float}}\n\
         output: {path: out.csv, columns: [ID, FROM_YEAR, FROM_DAY, FROM_ZERO]}\n\
         intermediates:\n  \
         - id: YEARLY\n    dataset: TAB\n    \
         key: {DAY: {impute: {type: date, source: SRC.YEAR, month: 1, day: first}}}\n  \
         - id: DAILY\n    dataset: TAB\n    \
         key: {MOMENT: {impute: {type: datetime, source: SRC.DAY, minimum_source_precision: day, time: first}}}\n  \
         - id: SIGNED\n    dataset: TAB\n    key: {NUM: SRC.F}\n\
         columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  \
         - {name: FROM_YEAR, type: str, derivation: YEARLY.VAL}\n  \
         - {name: FROM_DAY, type: str, derivation: DAILY.VAL}\n  \
         - {name: FROM_ZERO, type: str, derivation: SIGNED.VAL}\n",
        &[
            ("src.csv", "ID,YEAR,DAY,F\n1,2025,2025-01-01,-0.0\n"),
            (
                "tab.csv",
                "DAY,MOMENT,NUM,VAL\n2025-01-01,2025-01-01T00:00:00,0,7\n",
            ),
        ],
    );
    assert_eq!(
        fixture.saved(),
        "ID,FROM_YEAR,FROM_DAY,FROM_ZERO\n1,7,7,7\n"
    );
}

/// LOOK pairs the always missing KNUM with K, SRC.N padded to `width`.
fn padded(width: &str) -> Fixture {
    Fixture::new(
        &format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\n\
             input:\n  SRC: {{path: src.csv, types: {{N: int}}}}\n  \
             TAB: {{path: tab.csv, types: {{KINT: int, KNUM: float}}}}\n\
             output: {{path: out.csv, columns: [ID, V, V2]}}\n\
             intermediates:\n  - id: LOOK\n    dataset: TAB\n    \
             key: {{KNUM: {{literal: null}}, K: {{str_pad: {{source: SRC.N, width: {width}}}}}}}\n    \
             no_match: {{literal: null}}\n\
             columns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  \
             - {{name: V, type: str, derivation: LOOK.VAL}}\n  \
             - {{name: V2, type: str, derivation: LOOK.VAL}}\n"
        ),
        &[
            ("src.csv", "ID,N\na,7\nb,259\nc,\n"),
            (
                "tab.csv",
                "K,KINT,KNUM,VAL\n\"       7\",7,7,A\n\"     259\",259,259,B\n",
            ),
        ],
    )
}

#[test]
fn a_missing_first_match_value_does_not_skip_a_later_one() {
    // A missing match value seeks no record, so every row answers no_match;
    // yet each match value is evaluated before any record is read
    // (LOOKUP-25), and padding to a width no text can reach refuses the run.
    let answered = |path: &str| json!({"spec_path": path, "handler": "no_match", "count": 3});
    assert_eq!(
        padded("8").run(),
        Ok((
            "ID,V,V2\na,,\nb,,\nc,,\n".into(),
            json!([
                answered("columns.V.derivation.source.no_match"),
                answered("columns.V2.derivation.source.no_match"),
            ])
        ))
    );
    assert_eq!(
        padded(&"9".repeat(81)).run(),
        Err(vec![json!({
            "phase": "engine", "condition": "engine_rejected", "requirement": null,
            "spec_paths": [], "context": {"code": "result_projection", "stage": "build"}
        })])
    );
}
