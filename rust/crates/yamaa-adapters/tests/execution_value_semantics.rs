#![cfg(feature = "yaml")]
//! How a run treats values, as its saved artifact and its issues show it:
//! order terms sort each type and keep ties in construction order (VALUE-22,
//! VALUE-23); numbers compare exactly, and a negative zero equals zero yet
//! keeps its sign (VALUE-19); a result converts only when a row holds it
//! (VALUE-3); a `unique` check counts missing values (CHECK-10); and SUM adds
//! in record order (FORMULA-19).
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        let root = crate::support::Scratch::new("value-semantics");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, text) in files {
            fs::write(root.join(name), text).unwrap();
        }
        Self(root)
    }
    fn specification(&self) -> String {
        self.0.join("spec.yaml").to_str().unwrap().to_owned()
    }
    fn identity(specification: &str) -> Identity<'_> {
        crate::support::study::identity("value-semantics", specification)
    }
    /// The saved artifact, or the issues that kept anything from being saved.
    fn run(&self) -> Result<String, Vec<Value>> {
        let specification = self.specification();
        let mut domain = file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            Self::identity(&specification),
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
    /// The issues `check` reports without reading any input.
    fn check(&self) -> Vec<Value> {
        let specification = self.specification();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            Self::identity(&specification),
        )
        .iter()
        .map(issue)
        .collect()
    }
}

fn issue(issue: &Diagnostic) -> Value {
    crate::support::study::issue_row(issue)
}

/// One record per ID. Each column holds a high value on 1, a low value on 3
/// and on 4 a value equal to the low one, and nothing on 2 and 5: a NUL is an
/// ordinary scalar, the integers 2^53 and 2^53 + 1 differ although a float
/// cannot tell them apart, `-0` equals `0`, and a date or datetime completed
/// from a coarser text equals the finer one.
const ORDERED: &str = "ID,S,I,F,D,T\n\
    1,\u{e9},9007199254740993,1,2021-01-01,2021-01-01T00:00:00\n\
    2,,,,,\n\
    3,A\0z,9007199254740992,-0,2020,2020-01-01\n\
    4,A\0z,9007199254740992,0,2020-01,2020-01-01T00:00:00\n\
    5,,,,,\n";

/// The saved record of each ID, in construction order.
const SAVED: [&str; 5] = [
    "1,\u{e9},9007199254740993,1,2021-01-01,2021-01-01T00:00:00\n",
    "2,,,,,\n",
    "3,A\0z,9007199254740992,-0,2020-01-01,2020-01-01T00:00:00\n",
    "4,A\0z,9007199254740992,0,2020-01-01,2020-01-01T00:00:00\n",
    "5,,,,,\n",
];

fn ordered(term: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n\
         input: {{SRC: {{path: input.csv, types: {{ID: int, I: int, F: float}}}}}}\n\
         output: {{path: out.csv, columns: [ID, S, I, F, D, T], order_by: [{term}]}}\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - {{name: S, type: str, derivation: SRC.S}}\n\
         \x20 - {{name: I, type: int, derivation: SRC.I}}\n\
         \x20 - {{name: F, type: float, derivation: SRC.F}}\n\
         \x20 - name: D\n    type: date\n    derivation:\n      \
         impute: {{type: date, source: SRC.D, month: 1, day: first}}\n\
         \x20 - name: T\n    type: datetime\n    derivation:\n      \
         impute: {{type: datetime, source: SRC.T, month: 1, day: first, time: first}}\n"
    )
}

/// An `output.order_by` term sorts the present values of each type by that
/// type's order in its direction, and places missing values by `nulls`, first
/// under `asc` and last under `desc` when `nulls` is omitted (VALUE-22). Rows
/// equal on the term keep their construction order (VALUE-23), whatever
/// collected precision or zero sign tells the equal values apart, and every
/// row keeps its cells as constructed.
#[test]
fn output_order_sorts_every_type_places_missing_by_nulls_and_keeps_ties_in_construction_order() {
    for column in ["S", "I", "F", "D", "T"] {
        for descending in [false, true] {
            for nulls in [None, Some("first"), Some("last")] {
                let term = match (descending, nulls) {
                    (false, None) => column.to_owned(),
                    (false, Some(nulls)) => format!("{{variable: {column}, nulls: {nulls}}}"),
                    (true, None) => format!("{{variable: {column}, direction: desc}}"),
                    (true, Some(nulls)) => {
                        format!("{{variable: {column}, direction: desc, nulls: {nulls}}}")
                    }
                };
                let present: &[usize] = if descending { &[1, 3, 4] } else { &[3, 4, 1] };
                let order = if nulls.map_or(!descending, |nulls| nulls == "first") {
                    [&[2, 5], present].concat()
                } else {
                    [present, &[2, 5]].concat()
                };
                let expected = order
                    .iter()
                    .fold(String::from("ID,S,I,F,D,T\n"), |saved, id| {
                        saved + SAVED[id - 1]
                    });
                assert_eq!(
                    Fixture::new(&ordered(&term), &[("input.csv", ORDERED)]).run(),
                    Ok(expected),
                    "{term}"
                );
            }
        }
    }
}

/// A `unique` check counts a missing value as a value, so two rows whose X is
/// missing repeat one combination, and both rows offend (CHECK-10).
#[test]
fn unique_counts_missing_values_as_one_repeated_combination() {
    let fixture = Fixture::new(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n\
         input: {SRC: {path: input.csv, types: {ID: int, X: int}}}\n\
         output: {path: out.csv, report: report.csv, columns: [ID, X]}\n\
         columns:\n\
         \x20 - {name: ID, type: int, derivation: SRC.ID}\n\
         \x20 - {name: X, type: int, derivation: SRC.X}\n\
         verifications:\n  - unique: [X]\n",
        &[("input.csv", "ID,X\n1,\n2,\n")],
    );
    assert_eq!(
        fixture.run(),
        Err(vec![yamaa_adapters::json!({
            "phase": "verification",
            "condition": "unique_failed",
            "rule": "CHECK-10",
            "spec_paths": ["verifications[0].unique"],
            "context": {"columns": ["X"], "failure_count": 1, "keys": [{"ID": 1}, {"ID": 2}]},
        })])
    );
    // The report counts the one distinct combination the check evaluated
    // (CHECK-24).
    let report = fs::read_to_string(fixture.0.join("report.csv")).unwrap();
    assert!(
        report.lines().any(|line| line.starts_with(
            "out.csv,check,verifications[0].unique,unique,,,CHECK-10,error,violated,unique_failed,1,1,"
        )),
        "{report}"
    );
}

/// A float VALUE holding `literal` beside a text key ID. The row template
/// derives VALUE when `row_phase`, and otherwise the column does; it builds
/// one row per record, or one per group of records when `grouped`.
fn literal(literal: &str, grouped: bool, row_phase: bool) -> String {
    let group = if grouped { ", group_by: [SRC.ID]" } else { "" };
    let (row, column) = if row_phase {
        (format!("VALUE: {{literal: {literal}}}"), String::new())
    } else {
        (
            String::new(),
            format!(", derivation: {{literal: {literal}}}"),
        )
    };
    format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\n\
         output: {{path: out.csv, columns: [ID, VALUE]}}\n\
         columns:\n\
         \x20 - {{name: ID, type: str, derivation: SRC.ID}}\n\
         \x20 - {{name: VALUE, type: float{column}}}\n\
         rows:\n  - {{id: values{group}, derivations: {{{row}}}}}\n"
    )
}

/// A conversion failure reports the keys its row has completed, a key whose
/// value is missing among them, and none while the row lacks one: the row
/// phase converts its values before the column phase derives ID (VALUE-3,
/// COLUMN-8).
#[test]
fn a_conversion_failure_reports_only_the_keys_its_row_has_completed() {
    for (grouped, row_phase, keys) in [
        (false, false, Some(yamaa_adapters::json!([{"ID": null}]))),
        (true, false, Some(yamaa_adapters::json!([{"ID": null}]))),
        (false, true, None),
    ] {
        let specification = literal("x", grouped, row_phase);
        let mut context = yamaa_adapters::json!({"from": "str", "to": "float", "value": "x"});
        if let Some(keys) = keys {
            context["keys"] = keys;
        }
        // N keeps the record's line from being empty; its ID is missing.
        let fixture = Fixture::new(&specification, &[("input.csv", "ID,N\n,1\n")]);
        assert_eq!(
            fixture.run(),
            Err(vec![yamaa_adapters::json!({
                "phase": "convert",
                "condition": "conversion_failed",
                "rule": "VALUE-3",
                "spec_paths": ["columns.VALUE"],
                "context": context,
            })]),
            "{specification}"
        );
    }
}

/// A literal converts to its column's type when a row holds it, not when the
/// specification is read, so a text literal in a float column is no finding
/// of `check` (VALUE-24). Over no records the artifact has its header alone,
/// and over one record the literal fails conversion (VALUE-3), whether the
/// row template or the column derives it, over records or over groups.
#[test]
fn a_literal_converts_only_in_the_rows_that_hold_it() {
    for grouped in [false, true] {
        for row_phase in [true, false] {
            let specification = literal("TOTAL", grouped, row_phase);
            let empty = Fixture::new(&specification, &[("input.csv", "ID\n")]);
            assert_eq!(empty.check(), Vec::<Value>::new(), "{specification}");
            assert_eq!(empty.run(), Ok("ID,VALUE\n".to_owned()), "{specification}");
            let mut issues = Fixture::new(&specification, &[("input.csv", "ID\na\n")])
                .run()
                .unwrap_err();
            // Which keys the failure reports is the previous test's subject.
            for issue in &mut issues {
                issue["context"].as_object_mut().unwrap().remove("keys");
            }
            assert_eq!(
                issues,
                [yamaa_adapters::json!({
                    "phase": "convert",
                    "condition": "conversion_failed",
                    "rule": "VALUE-3",
                    "spec_paths": ["columns.VALUE"],
                    "context": {"from": "str", "to": "float", "value": "TOTAL"},
                })],
                "{specification}"
            );
        }
    }
}

/// Two records for each ID. ID 1's carry 2^53 and 2^53 + 1, and `-0` before
/// `0`; ID 2's carry 1 twice, and `0` before `-0`. No record carries S.
const COLLECTED: &str = "ID,I,F,S\n\
    1,9007199254740992,-0,\n\
    1,9007199254740993,0,\n\
    2,1,0,\n\
    2,1,-0,\n";

fn collected(columns: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n\
         input: {{SRC: {{path: input.csv, types: {{ID: int, I: int, F: float}}}}}}\n\
         output: {{path: out.csv, columns: [ID, {columns}]}}\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - {{name: I, type: int, derivation: SRC.I}}\n\
         \x20 - {{name: F, type: float, derivation: SRC.F}}\n\
         \x20 - {{name: S, type: str, derivation: SRC.S}}\n"
    )
}

/// Without row templates, a column reads one value for each key from the
/// records that built it, by exact equality (COLUMN-2, VALUE-19): 2^53 and
/// 2^53 + 1 are two values and fail, while `-0` and `0` are one value, which
/// keeps the representation its first record carries. Missing values give
/// missing.
#[test]
fn a_key_reads_one_value_by_exact_equality_and_keeps_its_first_representation() {
    assert_eq!(
        Fixture::new(&collected("F, S"), &[("input.csv", COLLECTED)]).run(),
        Ok("ID,F,S\n1,-0,\n2,0,\n".to_owned())
    );
    assert_eq!(
        Fixture::new(&collected("I"), &[("input.csv", COLLECTED)]).run(),
        Err(vec![yamaa_adapters::json!({
            "phase": "derivation",
            "condition": "multiple_values_per_key",
            "rule": "COLUMN-2",
            "spec_paths": ["columns.I.derivation.source"],
            "context": {"identifier": "SRC.I", "value_count": 2, "keys": [{"ID": 1}]},
        })])
    );
}

/// Equal extremes keep the first present operand in authored order, its type
/// included (EXPR-10, VALUE-19), so a zero result takes its sign from that
/// operand: the int 0 converts to `0` and the float -0.0 stays `-0`, for
/// `greatest` and `least` alike. A missing operand is skipped.
#[test]
fn equal_extremes_take_a_zeros_sign_from_the_first_present_operand() {
    let extreme = |name: &str, operation: &str, sources: &str| {
        format!("\x20 - {{name: {name}, type: float, derivation: {{{operation}: {{sources: [{sources}]}}}}}}\n")
    };
    let specification = format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n\
         input: {{SRC: {{path: input.csv, types: {{I: int, F: float, M: float}}}}}}\n\
         output: {{path: out.csv, columns: [ID, G1, G2, L1, L2]}}\n\
         columns:\n\
         \x20 - {{name: ID, type: str, derivation: SRC.ID}}\n{}{}{}{}",
        extreme("G1", "greatest", "SRC.M, SRC.I, SRC.F"),
        extreme("G2", "greatest", "SRC.M, SRC.F, SRC.I"),
        extreme("L1", "least", "SRC.M, SRC.I, SRC.F"),
        extreme("L2", "least", "SRC.M, SRC.F, SRC.I"),
    );
    assert_eq!(
        Fixture::new(&specification, &[("input.csv", "ID,I,F,M\na,0,-0,\n")]).run(),
        Ok("ID,G1,G2,L1,L2\na,0,-0,0,-0\n".to_owned())
    );
}

/// SUM adds the values of the donor records ID 1 matches in their stored
/// order, skipping a missing one, and MEAN divides that sum by the count
/// (FORMULA-19). Stored 1e16, 1, -1e16, the 1 is lost to rounding before
/// -1e16 cancels the sum; stored 1e16, -1e16, 1, the sum cancels first and
/// keeps the 1. The record of ID 2 enters neither reduction.
#[test]
fn sum_adds_matched_donor_records_in_stored_order_and_mean_divides_that_sum() {
    let specification = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n\
        input:\n  SRC: {path: input.csv, types: {ID: int}}\n  \
        S: {path: donor.csv, types: {ID: int, V: float}}\n\
        base: SRC\n\
        output: {path: out.csv, columns: [ID, SUM, MEAN]}\n\
        columns:\n\
        \x20 - {name: ID, type: int, derivation: SRC.ID}\n\
        \x20 - {name: SUM, type: float, derivation: {aggregate: 'SUM(S.V)'}}\n\
        \x20 - {name: MEAN, type: float, derivation: {aggregate: 'MEAN(S.V)'}}\n";
    for (last, saved) in [
        (["1", "-10000000000000000"], "ID,SUM,MEAN\n1,0,0\n"),
        (
            ["-10000000000000000", "1"],
            "ID,SUM,MEAN\n1,1,0.3333333333333333\n",
        ),
    ] {
        let donor = format!(
            "ID,V\n1,10000000000000000\n2,99\n1,{}\n1,\n1,{}\n",
            last[0], last[1]
        );
        assert_eq!(
            Fixture::new(
                specification,
                &[("input.csv", "ID\n1\n"), ("donor.csv", &donor)]
            )
            .run(),
            Ok(saved.to_owned()),
            "{donor}"
        );
    }
}
