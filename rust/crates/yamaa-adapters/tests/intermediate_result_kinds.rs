#![cfg(feature = "yaml")]
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
        Self(yamaa_adapters::canonical_path::canonicalize(directory).unwrap())
    }
    fn specification(&self) -> String {
        let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
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
input: {EX: ex.csv}
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
    no_match: {literal: null}
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
    let checked = file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    );
    assert!(checked.is_empty(), "{checked:?}\n{specification}");
    assert!(!study.0.join("ex.csv").exists());
    study.input();
    let mut result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
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
                     EXENDT: {{first_available: {{sources: [{{variable: EX.EXCHOICE, filter: 'EX.EXSTDT IS NOT NULL'}}, EX.EXSTDT], missing: {{literal: null}}}}}}"
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
        let checked = file_application::check(
            yamaa_core::project_function::Language::Python,
            request(),
            identity(&path),
        );
        assert_eq!(checked.len(), 1, "{operation}: {checked:?}");
        assert_eq!(checked[0].condition(), "incompatible_input_type");
        assert_eq!(checked[0].rule(), Some("FORMULA-5"));
        assert_eq!(checked[0].spec_paths, ["intermediates[0].filter"]);
        assert_eq!(
            yamaa_core::json::Value::Object(checked[0].context.clone()),
            yamaa_adapters::json!({"left_type":"date","right_type":"str"})
        );
        study.input();
        let result = file_application::domain(
            yamaa_core::project_function::Language::Python,
            request(),
            identity(&path),
        )
        .unwrap();
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
    "{case: [{when: 'EX.EXENDT0 IS NOT NULL', then: EXENDT0}, {otherwise: {literal: 17}}]}",
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
        let checked = file_application::check(
            yamaa_core::project_function::Language::Python,
            request(),
            identity(&path),
        );
        assert!(checked.is_empty(), "{checked:?}");
        fs::write(
            study.0.join("ex.csv"),
            "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,2024-01-01,2024-01-10,2024-01-01\n",
        )
        .unwrap();
        let mut result = file_application::domain(
            yamaa_core::project_function::Language::Python,
            request(),
            identity(&path),
        )
        .unwrap();
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
        let checked = file_application::check(
            yamaa_core::project_function::Language::Python,
            request(),
            identity(&path),
        );
        assert!(checked.is_empty(), "{checked:?}");
        fs::write(
            study.0.join("ex.csv"),
            "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,,,2024-01-01\n",
        )
        .unwrap();
        let result = file_application::domain(
            yamaa_core::project_function::Language::Python,
            request(),
            identity(&path),
        )
        .unwrap();
        let issues = result.issues();
        assert_eq!(issues.len(), 1, "{expression}: {issues:?}");
        assert_eq!(issues[0].condition(), "incompatible_input_type");
        assert_eq!(issues[0].rule(), Some("FORMULA-5"));
        assert_eq!(issues[0].spec_paths, ["intermediates[0].filter"]);
        assert_eq!(
            yamaa_core::json::Value::Object(issues[0].context.clone()),
            yamaa_adapters::json!({"left_type":"int","right_type":"date"})
        );
        assert!(result.output().is_none());
        assert!(!study.0.join("out.csv").exists());
    }
}

fn assert_integer_day_count(expression: &str, integer_row: &str, float_row: &str) {
    let specification = SPECIFICATION
            .replace("EX: ex.csv", "EX: {path: ex.csv, types: {EXINT: int, EXFLOAT: float}}")
            .replace("EXENDT: DERIVATION", &format!("SELECTED: {expression}\n      EXENDT: {{shift: {{type: date, source: EXSTDT, days: SELECTED}}}}"));
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let checked = file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    );
    assert!(checked.is_empty(), "{expression}: {checked:?}");
    fs::write(
        study.0.join("ex.csv"),
        format!("USUBJID,EXSTDTC,EXENDTC,EXINT,EXFLOAT\n{integer_row}\n"),
    )
    .unwrap();
    let mut result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
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
    let result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{expression}: {issues:?}");
    assert_eq!(issues[0].condition(), "incompatible_input_type");
    assert_eq!(issues[0].rule(), Some("TIME-14"));
    assert_eq!(
        issues[0].spec_paths,
        ["intermediates[0].derivations.EXENDT.shift.days"]
    );
    assert_eq!(
        yamaa_core::json::Value::Object(issues[0].context.clone()),
        yamaa_adapters::json!({"source":"SELECTED","expected":"int","actual":"float"})
    );
    assert!(result.output().is_none());
}

#[test]
fn numeric_choices_preserve_a_selected_integer_for_an_integer_only_operation() {
    for expression in [
        "{first_available: {sources: [EXINT, EXFLOAT]}}",
        "{greatest: {sources: [EXINT, EXFLOAT]}}",
        "{least: {sources: [EXINT, EXFLOAT]}}",
        "{case: [{when: 'EX.EXINT IS NOT NULL', then: EXINT}, {otherwise: EXFLOAT}]}",
    ] {
        assert_integer_day_count(expression, "S01,2024-01-01,,1,1.0", "S01,2024-01-01,,,1.0");
    }
}

/// VALUE-10: `NULLIF` over an `int` and a `float` is a `float`, also where the
/// `float` is missing, so binding refuses it as integer days before any row
/// is evaluated, the integer row included.
#[test]
fn nullif_over_an_int_and_a_float_is_refused_as_integer_days_when_bound() {
    let specification = SPECIFICATION
        .replace("EX: ex.csv", "EX: {path: ex.csv, types: {EXINT: int, EXFLOAT: float}}")
        .replace(
            "EXENDT: DERIVATION",
            "SELECTED: {compute: {expr: 'NULLIF(EXINT, EXFLOAT)'}}\n      EXENDT: {shift: {type: date, source: EXSTDT, days: SELECTED}}",
        );
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let assert_refused = |issues: &[yamaa_core::diagnostic::Diagnostic]| {
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition(), "incompatible_input_type");
        assert_eq!(issues[0].rule(), Some("TIME-14"));
        assert_eq!(
            issues[0].spec_paths,
            ["intermediates[0].derivations.EXENDT.shift.days"]
        );
        assert_eq!(
            yamaa_core::json::Value::Object(issues[0].context.clone()),
            yamaa_adapters::json!({"source":"SELECTED","expected":"int","actual":"float"})
        );
    };
    // Shift operands of an intermediate derivation are typed when bound.
    let checked = file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    );
    assert!(checked.is_empty(), "{checked:?}");
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,EXINT,EXFLOAT\nS01,2024-01-01,,1,\n",
    )
    .unwrap();
    let result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
    assert_refused(&result.issues());
    assert!(result.output().is_none());
}

/// VALUE-10 at run time: the `int` `NULLIF` returns widens to its `float`
/// type when the `float` argument is missing, as when it is present.
#[test]
fn nullif_widens_its_int_result_when_the_float_argument_is_missing() {
    let specification = "schema_version: '1.0'
domain: ADSL
keys: [USUBJID]
input: {EX: {path: ex.csv, types: {EXINT: int, EXFLOAT: float}}}
base: EX
output: {path: out.csv, columns: [USUBJID, VALUE]}
columns:
  - {name: USUBJID, type: str, derivation: EX.USUBJID}
  - {name: EXINT, type: int, derivation: EX.EXINT}
  - {name: EXFLOAT, type: float, derivation: EX.EXFLOAT}
  - {name: VALUE, type: str, derivation: {compute: {expr: 'NULLIF(EXINT, EXFLOAT)'}}}
";
    let study = Study::new(specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    assert!(file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path)
    )
    .is_empty());
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXINT,EXFLOAT\nS01,9007199254740993,\nS02,9007199254740993,0.5\nS03,3,3.0\n",
    )
    .unwrap();
    let mut result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("out.csv")).unwrap(),
        b"USUBJID,VALUE\nS01,9007199254740992\nS02,9007199254740992\nS03,\n"
    );
}

#[test]
fn unknown_kind_survives_later_aliases_and_choices() {
    for expression in HETEROGENEOUS_CHOICES {
        for alias in ["EXENDT", "{first_available: {sources: [EXENDT, EXSTDT]}}"] {
            let specification = heterogeneous_specification(expression)
                .replace("    filter:", &format!("      ALIAS: {alias}\n    filter:"))
                .replace("EX.EXENDT >= EX.ANCHOR", "EX.ALIAS >= 17");
            let study = Study::new(&specification);
            let path = study.specification();
            let request = || Request {
                specification: &path,
                environment: None,
            };
            let checked = file_application::check(
                yamaa_core::project_function::Language::Python,
                request(),
                identity(&path),
            );
            assert!(checked.is_empty(), "{expression}, {alias}: {checked:?}");
            fs::write(
                study.0.join("ex.csv"),
                "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,,,2024-01-01\n",
            )
            .unwrap();
            let mut result = file_application::domain(
                yamaa_core::project_function::Language::Python,
                request(),
                identity(&path),
            )
            .unwrap();
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
        .replace("intermediates:\n", "intermediates:\n  - id: EXP2\n    dataset: EX\n    derivations:\n      COPY: EXP.EXENDT\n      ANCHOR2: EXP.ANCHOR\n    filter: 'EX.COPY >= EX.ANCHOR2'\n    no_match: {literal: null}\n")
        .replace("derivation: EXP.EXENDT", "derivation: EXP2.COPY");
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    assert!(file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path)
    )
    .is_empty());
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,2024-01-01,2024-01-10,2024-01-01\n",
    )
    .unwrap();
    let mut result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
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
    assert!(file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path)
    )
    .is_empty());
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,REFDTC\nS01,2024-01-01,2024-01-10,2024-01-01\n",
    )
    .unwrap();
    let mut result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
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
        .replace("EX: ex.csv", "EX: {path: ex.csv, types: {EXINT: int, EXFLOAT: float}}")
        .replace("EXENDT: DERIVATION", "SELECTED: {first_available: {sources: [EXINT, EXFLOAT]}}\n      RESULT: {compute: {expr: 'SELECTED + 1'}}\n      EXENDT: EXENDT0")
        .replace("filter: 'EX.EXENDT >= EX.EXSTDT'", "filter: \"EX.RESULT >= 'not a number'\"");
    let study = Study::new(&specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let checked = file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    );
    assert!(checked.is_empty(), "{checked:?}");
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC,EXINT,EXFLOAT\nS01,2024-01-01,,,1.5\n",
    )
    .unwrap();
    let result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition(), "incompatible_input_type");
    assert_eq!(issues[0].spec_paths, ["intermediates[0].filter"]);
    assert_eq!(
        yamaa_core::json::Value::Object(issues[0].context.clone()),
        yamaa_adapters::json!({"left_type":"float","right_type":"str"})
    );
    assert!(result.output().is_none());
}

fn nested_lookup_specification() -> &'static str {
    "schema_version: '1.0'
domain: TEST
keys: [USUBJID]
input: {EX: ex.csv}
base: EX
output: {path: out.csv, columns: [USUBJID, SELECTED]}
intermediates:
  - id: DATES
    dataset: EX
    derivations:
      DATE: {to_date: {source: EXENDTC}}
    key: {DATE: {source: EX.CHOICE}}
    no_match: {literal: null}
  - id: RESULT
    dataset: EX
    derivations:
      DATE: {to_date: {source: EXSTDTC}}
      CHOICE: {first_available: {sources: [DATE], missing: 17}}
      SELECTED: DATES.DATE
    no_match: {literal: null}
columns:
  - {name: USUBJID, type: str, derivation: EX.USUBJID}
  - {name: SELECTED, type: date, derivation: RESULT.SELECTED}
"
}

fn assert_nested_lookup_output(specification: &str, input: &str, expected: &[u8]) {
    let study = Study::new(specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    let checked = file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    );
    assert!(checked.is_empty(), "{checked:?}");
    fs::write(
        study.0.join("ex.csv"),
        format!("USUBJID,EXSTDTC,EXENDTC\n{input}"),
    )
    .unwrap();
    let mut result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(study.0.join("out.csv")).unwrap(), expected);
}

#[test]
fn nested_lookup_keys_retain_unknown_current_kinds() {
    for expression in [
        "{source: EX.CHOICE}",
        "EX.CHOICE",
        "{greatest: {sources: [EX.CHOICE]}}",
    ] {
        assert_nested_lookup_output(
            &nested_lookup_specification().replace("{source: EX.CHOICE}", expression),
            "S01,2024-01-01,2024-01-01\n",
            b"USUBJID,SELECTED\nS01,2024-01-01\n",
        );
    }
}

fn assert_nested_lookup_type_error(
    specification: &str,
    expected_source: &str,
    expected_path: &str,
) {
    let study = Study::new(specification);
    let path = study.specification();
    let request = || Request {
        specification: &path,
        environment: None,
    };
    assert!(file_application::check(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path)
    )
    .is_empty());
    fs::write(
        study.0.join("ex.csv"),
        "USUBJID,EXSTDTC,EXENDTC\nS01,,2024-01-01\n",
    )
    .unwrap();
    let result = file_application::domain(
        yamaa_core::project_function::Language::Python,
        request(),
        identity(&path),
    )
    .unwrap();
    let issues = result.issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition(), "incompatible_input_type");
    assert_eq!(issues[0].rule(), Some("LOOKUP-9"));
    assert_eq!(issues[0].spec_paths, [expected_path]);
    assert_eq!(
        yamaa_core::json::Value::Object(issues[0].context.clone()),
        yamaa_adapters::json!({
            "intermediate":"DATES", "source":expected_source, "expected":"int", "actual":"date"
        })
    );
    assert!(result.output().is_none());
    assert!(!study.0.join("out.csv").exists());
}

#[test]
fn nested_lookup_keys_report_actual_incompatible_types_instead_of_no_match() {
    for (expression, source, path) in [
        (
            "{source: EX.CHOICE}",
            "key[DATE]",
            "intermediates[0].key.DATE",
        ),
        ("EX.CHOICE", "EX.CHOICE", "intermediates[0].key"),
        (
            "{greatest: {sources: [EX.CHOICE]}}",
            "key[DATE]",
            "intermediates[0].key.DATE",
        ),
    ] {
        assert_nested_lookup_type_error(
            &nested_lookup_specification().replace("{source: EX.CHOICE}", expression),
            source,
            path,
        );
    }
}

/// A missing key value seeks no record. A record's key is compared before the
/// filter narrows the matched records, so a filter cannot hide a key value
/// that cannot compare (LOOKUP-25).
#[test]
fn unknown_keys_keep_no_match_when_missing_and_refuse_an_incomparable_filtered_donor() {
    assert_nested_lookup_no_match(nested_lookup_specification(), "S01,,\n");
    assert_nested_lookup_type_error(
        &nested_lookup_specification().replace(
            "    key: {DATE:",
            "    filter: \"EX.USUBJID = 'S02'\"\n    key: {DATE:",
        ),
        "key[DATE]",
        "intermediates[0].key.DATE",
    );
}

fn assert_nested_lookup_no_match(specification: &str, input: &str) {
    assert_nested_lookup_output(specification, input, b"USUBJID,SELECTED\nS01,\n");
}

#[test]
fn raw_boolean_key_values_use_equality_without_requiring_an_order() {
    for (missing, expected) in [
        ("true", b"USUBJID,SELECTED\nS01,S01\n".as_slice()),
        ("false", b"USUBJID,SELECTED\nS01,\n".as_slice()),
    ] {
        let specification = nested_lookup_specification()
            .replace("{to_date: {source: EXENDTC}}", "{literal: true}")
            .replace("missing: 17", &format!("missing: {missing}"))
            .replace("SELECTED: DATES.DATE", "SELECTED: DATES.USUBJID")
            .replace(
                "type: date, derivation: RESULT.SELECTED",
                "type: str, derivation: RESULT.SELECTED",
            );
        assert_nested_lookup_output(&specification, "S01,,\n", expected);
    }
}

#[test]
fn incompatible_raw_key_pairs_fail_independently_of_another_pairs_order_or_value() {
    for pairs in [
        "USUBJID: {literal: S02}, DATE: {source: EX.CHOICE}",
        "DATE: {source: EX.CHOICE}, USUBJID: {literal: S02}",
    ] {
        let specification =
            nested_lookup_specification().replace("DATE: {source: EX.CHOICE}", pairs);
        assert_nested_lookup_type_error(&specification, "key[DATE]", "intermediates[0].key.DATE");
        assert_nested_lookup_no_match(&specification, "S01,,\n");
        assert_nested_lookup_no_match(&specification, "S01,2024-01-01,2024-01-02\n");
    }
}
