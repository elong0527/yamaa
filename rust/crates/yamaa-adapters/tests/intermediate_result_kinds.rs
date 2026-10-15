//! Untyped donor expressions retain their scalar kind before later reads.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new(specification: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-intermediate-kinds-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("spec.yaml"), specification).unwrap();
        Self(fs::canonicalize(directory).unwrap())
    }
    fn specification(&self) -> String {
        let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path
            .strip_prefix(r"\\?\")
            .unwrap_or(&path)
            .replace('\\', "/");
        path
    }
    fn input(&self) {
        fs::write(
            self.0.join("ex.csv"),
            "USUBJID,EXSTDTC,EXENDTC\n\
             S01,2024-01-01,2024-01-10\n\
             S02,2024-01-05,\n\
             S03,2024-01-03,2024-01-02\n",
        )
        .unwrap();
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn identity(specification: &str) -> Identity<'_> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "test",
        specification,
        base_directory: ".",
    }
}
const SPECIFICATION: &str = "schema_version: '1.0'
domain: ADSL
keys: [USUBJID]
input: {EX: {path: ex.csv}}
base: EX
output: {path: out.csv, columns: [USUBJID, TRTSDT, TRTEDT]}
intermediates:
  - id: EXP
    dataset: EX
    derivations:
      EXSTDT: {to_date: {source: EXSTDTC}}
      EXENDT0: {to_date: {source: EXENDTC}}
      EXENDT: DERIVATION
    filter: 'EX.EXENDT >= EX.EXSTDT'
    no_match: null
columns:
  - {name: USUBJID, type: str, derivation: EX.USUBJID}
  - {name: TRTSDT, type: date, derivation: EXP.EXSTDT}
  - {name: TRTEDT, type: date, derivation: EXP.EXENDT}
";

fn assert_output(specification: &str, expected: &str) {
    let study = Study::new(specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let checked = file_application::check(request(), identity(&path));
    assert!(checked.is_empty(), "{checked:?}\n{specification}");
    assert!(!study.0.join("ex.csv").exists());
    study.input();
    let mut result = file_application::domain(request(), identity(&path)).unwrap();
    assert!(
        result.issues().is_empty(),
        "{:?}\n{specification}",
        result.issues()
    );
    assert!(result.output().is_some());
    assert!(!study.0.join("out.csv").exists());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("out.csv")).unwrap(),
        expected.as_bytes()
    );
}

#[test]
fn selector_dates_agree_in_check_and_build_with_or_without_typed_binding() {
    for (operation, expected) in [
        (
            "first_available",
            "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,2024-01-05\nS03,,\n",
        ),
        (
            "greatest",
            "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,2024-01-05\nS03,2024-01-03,2024-01-03\n",
        ),
        (
            "least",
            "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-01\nS02,2024-01-05,2024-01-05\nS03,,\n",
        ),
    ] {
        for typed in [false, true] {
            let expression = format!("{{{operation}: {{sources: [EXENDT0, EXSTDT]}}}}");
            let expression = if typed {
                format!("{{type: date, derivation: {expression}}}")
            } else {
                expression
            };
            assert_output(&SPECIFICATION.replace("DERIVATION", &expression), expected);
        }
    }
}

#[test]
fn selector_kind_flows_to_later_derivations_and_qualified_source_objects() {
    for (operation, expected) in [
        (
            "first_available",
            "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,2024-01-05\nS03,,\n",
        ),
        (
            "greatest",
            "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,2024-01-05\nS03,2024-01-03,2024-01-03\n",
        ),
        (
            "least",
            "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-01\nS02,2024-01-05,2024-01-05\nS03,,\n",
        ),
    ] {
        assert_output(
            &SPECIFICATION.replace(
                "EXENDT: DERIVATION",
                &format!(
                    "EXCHOICE: {{{operation}: {{sources: [EX.EXENDT0, EX.EXSTDT]}}}}\n      \
                     EXENDT: {{first_available: {{sources: [{{variable: EX.EXCHOICE, filter: 'EX.EXSTDT IS NOT NULL'}}, EX.EXSTDT], missing: null}}}}"
                ),
            ),
            expected,
        );
    }
}

#[test]
fn first_available_infers_the_declared_missing_result_without_sources() {
    assert_output(
        &SPECIFICATION
            .replace(
                "DERIVATION",
                "{first_available: {sources: [], missing: 17}}",
            )
            .replace("EX.EXENDT >= EX.EXSTDT", "EX.EXENDT >= 17")
            .replace(
                "type: date, derivation: EXP.EXENDT",
                "type: int, derivation: EXP.EXENDT",
            ),
        "USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,17\nS02,2024-01-05,17\nS03,2024-01-03,17\n",
    );
}

#[test]
fn known_date_choices_reject_incompatible_filter_types_in_check_and_build() {
    for operation in ["first_available", "greatest", "least"] {
        let expression = format!("{{{operation}: {{sources: [EXENDT0, EXSTDT]}}}}");
        let study = Study::new(&SPECIFICATION.replace("DERIVATION", &expression).replace(
            "filter: 'EX.EXENDT >= EX.EXSTDT'",
            "filter: \"EX.EXENDT >= 'not a date'\"",
        ));
        let path = study.specification();
        let request = || Request {
            specification: &path,
            environment: None,
        };
        let checked = file_application::check(request(), identity(&path));
        assert_eq!(checked.len(), 1, "{operation}: {checked:?}");
        assert_eq!(checked[0].condition, "incompatible_input_type");
        assert_eq!(checked[0].requirement.as_deref(), Some("REQ-0190"));
        assert_eq!(checked[0].spec_paths, ["intermediates[0].filter"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&checked[0].context).unwrap(),
            serde_json::json!({"left_type":"date","right_type":"str"})
        );
        study.input();
        let result = file_application::domain(request(), identity(&path)).unwrap();
        assert_eq!(result.issues(), checked);
        assert!(result.output().is_none());
        assert!(!study.0.join("out.csv").exists());
    }
}

fn heterogeneous_specification(expression: &str) -> String {
    SPECIFICATION
        .replace("DERIVATION", expression)
        .replace(
            "      EXENDT:",
            "      ANCHOR: {to_date: {source: REFDTC}}\n      EXENDT:",
        )
        .replace("EX.EXENDT >= EX.EXSTDT", "EX.EXENDT >= EX.ANCHOR")
        .replace(
            "type: date, derivation: EXP.EXENDT",
            "type: str, derivation: EXP.EXENDT",
        )
}
const HETEROGENEOUS_CHOICES: [&str; 2] = [
    "{first_available: {sources: [EXENDT0, EXSTDT], missing: 17}}",
    "{case: [{when: 'EX.EXENDT0 IS NOT NULL', then: {source: EXENDT0}}, {otherwise: {literal: 17}}]}",
];

#[test]
fn unknown_choices_compare_the_actual_date_when_a_date_source_is_present() {
    for expression in HETEROGENEOUS_CHOICES {
        let study = Study::new(&heterogeneous_specification(expression));
        let path = study.specification();
        let request = || Request {
            specification: &path,
            environment: None,
        };
        let checked = file_application::check(request(), identity(&path));
        assert!(checked.is_empty(), "{checked:?}");
        fs::write(
            study.0.join("ex.csv"),
            "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,2024-01-01,2024-01-10,2024-01-01\n",
        )
        .unwrap();
        let mut result = file_application::domain(request(), identity(&path)).unwrap();
        assert!(
            result.issues().is_empty(),
            "{expression}: {:?}",
            result.issues()
        );
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read(study.0.join("out.csv")).unwrap(),
            b"USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\n"
        );
    }
}

#[test]
fn unknown_choices_report_the_actual_integer_when_date_sources_are_missing() {
    for expression in HETEROGENEOUS_CHOICES {
        let study = Study::new(&heterogeneous_specification(expression));
        let path = study.specification();
        let request = || Request {
            specification: &path,
            environment: None,
        };
        let checked = file_application::check(request(), identity(&path));
        assert!(checked.is_empty(), "{checked:?}");
        fs::write(
            study.0.join("ex.csv"),
            "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,,,2024-01-01\n",
        )
        .unwrap();
        let result = file_application::domain(request(), identity(&path)).unwrap();
        let issues = result.issues();
        assert_eq!(issues.len(), 1, "{expression}: {issues:?}");
        assert_eq!(issues[0].condition, "incompatible_input_type");
        assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0190"));
        assert_eq!(issues[0].spec_paths, ["intermediates[0].filter"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
            serde_json::json!({"left_type":"int","right_type":"date"})
        );
        assert!(result.output().is_none());
        assert!(!study.0.join("out.csv").exists());
    }
}

fn assert_integer_day_count(expression: &str, integer_row: &str, float_row: &str) {
    let specification = SPECIFICATION
            .replace("EX: {path: ex.csv}", "EX: {path: ex.csv, types: {EXINT: int, EXFLOAT: float}}")
            .replace("EXENDT: DERIVATION", &format!("SELECTED: {expression}\n      EXENDT: {{date_shift: {{source: EXSTDT, days: SELECTED}}}}"));
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let checked = file_application::check(request(), identity(&path));
    assert!(checked.is_empty(), "{expression}: {checked:?}");
    fs::write(
        study.0.join("ex.csv"),
        format!("USUBJID,EXSTDTC,EXENDTC,EXINT,EXFLOAT\n{integer_row}\n"),
    )
    .unwrap();
    let mut result = file_application::domain(request(), identity(&path)).unwrap();
    assert!(
        result.issues().is_empty(),
        "{expression}: {:?}",
        result.issues()
    );
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("out.csv")).unwrap(),
        b"USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-02\n"
    );

    fs::write(
        study.0.join("ex.csv"),
        format!("USUBJID,EXSTDTC,EXENDTC,EXINT,EXFLOAT\n{float_row}\n"),
    )
    .unwrap();
    let result = file_application::domain(request(), identity(&path)).unwrap();
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{expression}: {issues:?}");
    assert_eq!(issues[0].condition, "incompatible_input_type");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-1298"));
    assert_eq!(
        issues[0].spec_paths,
        ["intermediates[0].derivations.EXENDT.date_shift.days"]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
        serde_json::json!({"source":"SELECTED","expected":"int","actual":"float"})
    );
    assert!(result.output().is_none());
}

#[test]
fn numeric_choices_preserve_a_selected_integer_for_an_integer_only_operation() {
    for expression in [
        "{first_available: {sources: [EXINT, EXFLOAT]}}",
        "{greatest: {sources: [EXINT, EXFLOAT]}}",
        "{least: {sources: [EXINT, EXFLOAT]}}",
        "{case: [{when: 'EX.EXINT IS NOT NULL', then: {source: EXINT}}, {otherwise: {source: EXFLOAT}}]}",
    ] {
        assert_integer_day_count(expression, "S01,2024-01-01,,1,1.0", "S01,2024-01-01,,,1.0");
    }
}

#[test]
fn numeric_selection_functions_promote_present_values_without_inventing_missing_types() {
    for expression in [
        "{compute: {expr: 'COALESCE(EXINT, EXFLOAT)'}}",
        "{compute: {expr: 'GREATEST(EXINT, EXFLOAT)'}}",
        "{compute: {expr: 'LEAST(EXINT, EXFLOAT)'}}",
    ] {
        assert_integer_day_count(expression, "S01,2024-01-01,,1,", "S01,2024-01-01,,1,1.0");
    }
}

#[test]
fn unknown_kind_survives_later_aliases_and_choices() {
    for expression in HETEROGENEOUS_CHOICES {
        for alias in [
            "{source: EXENDT}",
            "{first_available: {sources: [EXENDT, EXSTDT]}}",
        ] {
            let specification = heterogeneous_specification(expression)
                .replace("    filter:", &format!("      ALIAS: {alias}\n    filter:"))
                .replace("EX.EXENDT >= EX.ANCHOR", "EX.ALIAS >= 17");
            let study = Study::new(&specification);
            let path = study.specification();
            let request = || Request {
                specification: &path,
                environment: None,
            };
            let checked = file_application::check(request(), identity(&path));
            assert!(checked.is_empty(), "{expression}, {alias}: {checked:?}");
            fs::write(
                study.0.join("ex.csv"),
                "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,,,2024-01-01\n",
            )
            .unwrap();
            let mut result = file_application::domain(request(), identity(&path)).unwrap();
            let issues = result.issues();
            assert!(issues.is_empty(), "{expression}, {alias}: {issues:?}");
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(study.0.join("out.csv")).unwrap(),
                b"USUBJID,TRTSDT,TRTEDT\nS01,,17\n"
            );
        }
    }
}

#[test]
fn unknown_kind_flows_through_an_earlier_selector_in_dependency_order() {
    let specification = heterogeneous_specification(HETEROGENEOUS_CHOICES[0])
        .replace("    filter: 'EX.EXENDT >= EX.ANCHOR'\n", "")
        .replace("intermediates:\n", "intermediates:\n  - id: EXP2\n    dataset: EX\n    derivations:\n      COPY: {source: EXP.EXENDT}\n      ANCHOR2: {source: EXP.ANCHOR}\n    filter: 'EX.COPY >= EX.ANCHOR2'\n    no_match: null\n")
        .replace("derivation: EXP.EXENDT", "derivation: EXP2.COPY");
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    assert!(file_application::check(request(), identity(&path)).is_empty());
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,2024-01-01,2024-01-10,2024-01-01\n",
    )
    .unwrap();
    let mut result = file_application::domain(request(), identity(&path)).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("out.csv")).unwrap(),
        b"USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\n"
    );
}

#[test]
fn generic_predicate_consumers_use_the_named_intermediates_unknown_kind() {
    let specification = heterogeneous_specification(HETEROGENEOUS_CHOICES[0])
        .replace("    filter: 'EX.EXENDT >= EX.ANCHOR'\n", "")
        .replace(
            "[USUBJID, TRTSDT, TRTEDT]",
            "[USUBJID, TRTSDT, TRTEDT, PASS]",
        )
        + "  - name: PASS\n    type: str\n    derivation: {flag: 'EXP.EXENDT >= EXP.ANCHOR'}\n";
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    assert!(file_application::check(request(), identity(&path)).is_empty());
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,2024-01-01,2024-01-10,2024-01-01\n",
    )
    .unwrap();
    let mut result = file_application::domain(request(), identity(&path)).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("out.csv")).unwrap(),
        b"USUBJID,TRTSDT,TRTEDT,PASS\nS01,2024-01-01,2024-01-10,Y\n"
    );
}

#[test]
fn unknown_numeric_operands_do_not_invent_an_integer_result_kind() {
    let specification = SPECIFICATION
        .replace("EX: {path: ex.csv}", "EX: {path: ex.csv, types: {EXINT: int, EXFLOAT: float}}")
        .replace("EXENDT: DERIVATION", "SELECTED: {first_available: {sources: [EXINT, EXFLOAT]}}\n      RESULT: {compute: {expr: 'SELECTED + 1'}}\n      EXENDT: {source: EXENDT0}")
        .replace("filter: 'EX.EXENDT >= EX.EXSTDT'", "filter: \"EX.RESULT >= 'not a number'\"");
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let checked = file_application::check(request(), identity(&path));
    assert!(checked.is_empty(), "{checked:?}");
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,EXINT,EXFLOAT\nS01,2024-01-01,,,1.5\n",
    )
    .unwrap();
    let result = file_application::domain(request(), identity(&path)).unwrap();
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "incompatible_input_type");
    assert_eq!(issues[0].spec_paths, ["intermediates[0].filter"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
        serde_json::json!({"left_type":"float","right_type":"str"})
    );
    assert!(result.output().is_none());
}
