//! Signed-i64 runtime literals fail before study access, with the same complete
//! issue in both shared host routes. Declaration count bounds stay mathematical.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(column: &str, extra: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-integer-literal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), format!(
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input.csv}}\noutput: {{path: out.csv, columns: [USUBJID, N]}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n{column}\n{extra}"
        )).unwrap();
        Self(root)
    }

    fn input(&self) {
        fs::write(self.0.join("input.csv"), "USUBJID,AGE\nS-001,34\n").unwrap();
    }

    fn specification(&self) -> String {
        self.0.join("spec.yaml").to_str().unwrap().to_owned()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn identity<'a>(runtime: &'a str, specification: &'a str) -> Identity<'a> {
    Identity {
        runtime,
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "integer-literal-overflow",
        specification,
        base_directory: ".",
    }
}

fn issue(value: &Issue) -> serde_json::Value {
    serde_json::json!({"phase":value.phase,"condition":value.condition,"requirement":value.requirement,"spec_paths":value.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&value.context).unwrap()})
}

fn assert_overflow(fixture: &Fixture, path: &str, actual: &str) {
    let mut expected: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/integer_literal_overflow.json")).unwrap();
    expected["spec_paths"] = serde_json::json!([path]);
    expected["context"]["actual"] = serde_json::from_str(actual).unwrap();
    let specification = fixture.specification();
    for runtime in ["python", "r"] {
        let request = || Request {
            specification: &specification,
            environment: None,
        };
        let checked = file_application::check(request(), identity(runtime, &specification));
        let built = match file_application::domain(request(), identity(runtime, &specification)) {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        for (route, issues) in [("check", checked), ("domain", built)] {
            assert_eq!(issues.len(), 1, "{runtime}/{route}: {issues:?}");
            assert_eq!(issue(&issues[0]), expected, "{runtime}/{route}: {path}");
        }
    }
    assert!(!fixture.0.join("out.csv").exists());
}

#[test]
fn wide_runtime_literals_report_validation_before_missing_study_input() {
    let huge = format!("1{}", "0".repeat(400));
    for (written, actual) in [
        ("9223372036854775808", "9223372036854775808"),
        ("-9223372036854775809", "-9223372036854775809"),
        ("18446744073709551616", "18446744073709551616"),
        ("0x8000000000000000", "9223372036854775808"),
        (huge.as_str(), huge.as_str()),
    ] {
        let cases = [
            (format!("  - {{name: N, type: int, derivation: {{literal: {written}}}}}"), "columns.N.derivation.literal"),
            (format!("  - {{name: N, type: float, derivation: {{literal: {written}}}}}"), "columns.N.derivation.literal"),
            (format!("  - {{name: N, type: str, derivation: {{literal: {written}}}}}"), "columns.N.derivation.literal"),
            (format!("  - {{name: N, type: int, derivation: DM.AGE, verifications: {{allowed_values: {{values: [34, {written}]}}}}}}"), "columns.N.verifications[0].allowed_values.values[1]"),
            (format!("  - {{name: N, type: int, derivation: DM.AGE, verifications: {{range: {{min: {written}}}}}}}"), "columns.N.verifications[0].range.min"),
            (format!("  - {{name: N, type: int, derivation: DM.AGE, verifications: {{range: {{max: {written}}}}}}}"), "columns.N.verifications[0].range.max"),
            (format!("  - {{name: N, type: str, derivation: DM.USUBJID, verifications: {{max_length: {{max: {written}}}}}}}"), "columns.N.verifications[0].max_length.max"),
            (format!("  - {{name: N, type: int, derivation: {{value: {{source: DM.AGE}}, unconvertible: {written}}}}}"), "columns.N.derivation.unconvertible"),
        ];
        for (column, path) in cases {
            // No input.csv exists: preparation must report the literal, before IO.
            assert_overflow(&Fixture::new(&column, ""), path, actual);
        }
    }
}

#[test]
fn row_and_unselected_case_literals_keep_their_authored_paths() {
    let wide = "9223372036854775808";
    let row = Fixture::new(
        "  - {name: N, type: int}",
        &format!(
            "rows:\n  - {{id: record, dataset: DM, derivations: {{N: {{literal: {wide}}}}}}}\n"
        ),
    );
    assert_overflow(&row, "rows[0].derivations.N.literal", wide);
    let case = Fixture::new(
        &format!("  - {{name: N, type: int, derivation: {{case: [{{when: 'FALSE', then: {{literal: {wide}}}}}, {{otherwise: {{literal: 0}}}}]}}}}"),
        "",
    );
    assert_overflow(&case, "columns.N.derivation.case[0].then.literal", wide);
    fs::rename(case.0.join("spec.yaml"), case.0.join("parent.yaml")).unwrap();
    fs::write(
        case.0.join("spec.yaml"),
        "schema_version: '1.0'\nparents: parent.yaml\n",
    )
    .unwrap();
    assert_overflow(&case, "columns.N.derivation.case[0].then.literal", wide);
}

#[test]
fn integer_endpoints_quoted_identifiers_and_explicit_floats_still_build() {
    for (kind, written, expected) in [
        ("int", "-9223372036854775808", "-9223372036854775808"),
        ("int", "9223372036854775807", "9223372036854775807"),
        ("int", "0x7fffffffffffffff", "9223372036854775807"),
        ("str", "'9223372036854775808'", "9223372036854775808"),
        ("float", "9223372036854775808.0", "9223372036854776000"),
    ] {
        let fixture = Fixture::new(
            &format!("  - {{name: N, type: {kind}, derivation: {{literal: {written}}}}}"),
            "",
        );
        fixture.input();
        let specification = fixture.specification();
        for runtime in ["python", "r"] {
            let request = || Request {
                specification: &specification,
                environment: None,
            };
            assert!(
                file_application::check(request(), identity(runtime, &specification)).is_empty()
            );
            let mut domain =
                file_application::domain(request(), identity(runtime, &specification)).unwrap();
            assert!(
                domain.issues().is_empty(),
                "{written}: {:?}",
                domain.issues()
            );
            assert!(domain.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                format!("USUBJID,N\nS-001,{expected}\n")
            );
        }
    }
}

#[test]
fn wide_mathematical_count_bounds_still_check_and_build() {
    for maximum in [
        "9223372036854775808".to_owned(),
        format!("1{}", "0".repeat(400)),
    ] {
        let fixture = Fixture::new(
            "  - {name: N, type: int, derivation: DM.AGE}",
            &format!("verifications:\n  - row_count: {{max: {maximum}}}\n"),
        );
        fixture.input();
        let specification = fixture.specification();
        for runtime in ["python", "r"] {
            let request = || Request {
                specification: &specification,
                environment: None,
            };
            assert!(
                file_application::check(request(), identity(runtime, &specification)).is_empty()
            );
            let domain =
                file_application::domain(request(), identity(runtime, &specification)).unwrap();
            assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        }
    }
}
