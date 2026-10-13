#![cfg(any(unix, windows))]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-domain-submission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
    fn check(&self, raw: &str) -> Vec<yamaa_adapters::issue_rows::Issue> {
        fs::write(self.0.join("spec.yaml"), raw).unwrap();
        let path = self.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        file_application::check(
            Request {
                specification: path,
                environment: None,
            },
            identity(path),
        )
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn identity(path: &str) -> Identity<'_> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "submission",
        specification: path,
        base_directory: ".",
    }
}
fn copy(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &path)
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}
const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: str\n    derivation: SRC.VALUE\n    submission: {length: 3, origin: {type: Assigned}}\n";
#[test]
fn submission_fields_do_not_change_any_original_domain_artifact() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (example, artifact) in [
        ("sdtm-ae-coding", "ae.csv"),
        ("sdtm-dm-metadata", "dm.csv"),
        ("sdtm-lb-metadata", "lb.csv"),
        ("sdtm-vs-epoch-from-subject-elements", "vs.csv"),
    ] {
        let case = repository.join("benchmarks").join(example);
        let study = Study::new();
        copy(&case, &study.0);
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let mut run = file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(path),
        )
        .unwrap_or_else(|issues| panic!("{example}: {issues:?}"));
        assert!(run.issues().is_empty(), "{example}: {:?}", run.issues());
        run.save().unwrap();
        assert_eq!(
            fs::read(study.0.join(artifact)).unwrap(),
            fs::read(case.join("expected").join(artifact)).unwrap(),
            "{example}"
        );
    }
}
#[test]
fn metadata_admission_has_exact_causes_before_study_access() {
    let study = Study::new();
    assert!(study.check(VALID).is_empty());
    let issues = study.check(&format!("{VALID}\nmetadata: {{label: forbidden}}\n"));
    assert_eq!(issues.len(), 1);
    assert_eq!(
        (
            &issues[0].phase,
            &issues[0].condition,
            issues[0].requirement.as_deref()
        ),
        (
            &"validation".into(),
            &"reserved_metadata_key".into(),
            Some("REQ-0910")
        )
    );
    assert_eq!(issues[0].spec_paths, ["metadata.label"]);
    assert_eq!(issues[0].context, "{\"key\":\"label\",\"level\":\"root\"}");
    let issues = study.check(&VALID.replace("length: 3", "length: -99999999999999999999999999999"));
    assert_eq!(issues[0].condition, "submission_length_invalid");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0911"));
    assert_eq!(
        issues[0].context,
        "{\"column\":\"VALUE\",\"length\":-99999999999999999999999999999}"
    );
    let issues = study.check(&VALID.replace("origin: {type: Assigned}", "origin: {type: Derived}"));
    assert_eq!(
        issues
            .iter()
            .map(|issue| issue.condition.as_str())
            .collect::<Vec<_>>(),
        ["method_missing", "origin_contradicts_derivation"]
    );
    let raw = VALID.replace(
        "submission: {length: 3",
        "verifications: {max_length: {max: 4}}\n    submission: {length: 3",
    );
    let issues = study.check(&raw);
    assert_eq!(issues[0].condition, "declared_length_conflict");
    assert_eq!(
        issues[0].context,
        "{\"column\":\"VALUE\",\"length\":3,\"max_length\":4}"
    );
    assert!(!study.0.join("output.csv").exists());
}
#[test]
fn declared_length_is_checked_once_over_unicode_scalar_values() {
    let study = Study::new();
    fs::write(study.0.join("spec.yaml"), VALID).unwrap();
    fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,abcd\n").unwrap();
    let path = study.0.join("spec.yaml");
    let path = path.to_str().unwrap();
    let mut run = file_application::domain(
        Request {
            specification: path,
            environment: None,
        },
        identity(path),
    )
    .unwrap();
    let issues = run.issues();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "length_failed");
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0378"));
    assert_eq!(
        issues[0].context,
        "{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}],\"max\":3}"
    );
    assert!(run.save().is_err());
    assert!(!study.0.join("output.csv").exists());
}

const ROW_CT: &str = "schema_version: '1.0'\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, RESULT, LBTESTCD]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: RESULT, type: str, derivation: SRC.VALUE, submission: {origin: {type: Assigned}, codelist: DEFAULT}}\n  - {name: LBTESTCD, type: str}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: {literal: GLUC}}\n    submission: {RESULT: {codelist: GLUCOSE}}\n  - id: creatinine\n    derivations: {LBTESTCD: {literal: CREAT}}\n";
const CT_ENV: &str = "schema_version: '1.0'\ncodelists:\n  - codelists:\n      - {id: DEFAULT, name: Default values, items: [{value: D}]}\n      - {id: GLUCOSE, name: Glucose values, items: [{value: G}]}\n      - {id: EXT, name: Extended values, extensible: true, items: [{value: X}]}\n      - {id: DICT, name: Dictionary, external: {dictionary: MedDRA, version: '27.1'}}\n";
#[test]
fn per_value_codelists_inherit_origins_and_check_completed_discriminators() {
    let study = Study::new();
    fs::write(study.0.join("spec.yaml"), ROW_CT).unwrap();
    fs::write(study.0.join("environment.yaml"), CT_ENV).unwrap();
    fs::write(study.0.join("input.csv"), "ID,VALUE\n1,G\n2,D\n3,\n").unwrap();
    let path = study.0.join("spec.yaml");
    let path = path.to_str().unwrap();
    let environment = study.0.join("environment.yaml");
    let mut run = project_domain(path, environment.to_str().unwrap());
    assert_eq!(run.issues().len(), 2, "{:?}", run.issues());
    for issue in run.issues() {
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.condition.as_str(),
                issue.requirement.as_deref()
            ),
            ("verification", "allowed_values_failed", Some("REQ-0957"))
        );
        assert_eq!(issue.spec_paths, ["columns.RESULT.submission.codelist"]);
    }
    let contexts = run
        .issues()
        .iter()
        .map(|issue| serde_json::from_str::<serde_json::Value>(&issue.context).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        contexts[0],
        serde_json::json!({"column":"RESULT", "codelist":"DEFAULT", "failure_count":1, "keys":[{"ID":1,"LBTESTCD":"CREAT"}], "values":["G"]})
    );
    assert_eq!(
        contexts[1],
        serde_json::json!({"column":"RESULT", "codelist":"GLUCOSE", "failure_count":1, "keys":[{"ID":2,"LBTESTCD":"GLUC"}], "values":["D"]})
    );
    assert!(run.save().is_err());
    assert!(!study.0.join("output.csv").exists());
}
#[test]
fn per_value_external_and_extensible_bindings_exclude_default_enforcement() {
    for codelist in ["EXT", "DICT"] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            ROW_CT.replace("codelist: GLUCOSE", &format!("codelist: {codelist}")),
        )
        .unwrap();
        fs::write(study.0.join("environment.yaml"), CT_ENV).unwrap();
        fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let environment = study.0.join("environment.yaml");
        let mut run = project_domain(path, environment.to_str().unwrap());
        assert!(run.issues().is_empty(), "{codelist}: {:?}", run.issues());
        assert!(run.save().unwrap());
        assert_eq!(
            fs::read_to_string(study.0.join("output.csv")).unwrap(),
            "ID,RESULT,LBTESTCD\n1,D,GLUC\n2,,GLUC\n1,D,CREAT\n2,,CREAT\n"
        );
    }
}
#[test]
fn per_value_codelist_unknown_and_type_failures_retain_the_authored_row_path() {
    for (codelist, expected) in [
        ("UNKNOWN", "unknown_codelist"),
        ("INT", "codelist_type_mismatch"),
    ] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            ROW_CT.replace("codelist: GLUCOSE", &format!("codelist: {codelist}")),
        )
        .unwrap();
        fs::write(study.0.join("environment.yaml"), format!("{CT_ENV}      - {{id: INT, name: Numeric values, data_type: integer, items: [{{value: 1}}]}}\n")).unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let environment = study.0.join("environment.yaml");
        let host = yamaa_core::project_function::Language::Python;
        let Err(error) =
            yamaa_adapters::file_workflow::select(path, environment.to_str().unwrap(), host)
        else {
            panic!("invalid value codelist was admitted");
        };
        let issues = yamaa_adapters::project_application::selection_failure(&error, host);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, expected);
        assert_eq!(
            issues[0].spec_paths,
            ["rows.glucose.submission.RESULT.codelist"]
        );
        assert!(!study.0.join("input.csv").exists());
    }
}

struct NoActivation;
impl yamaa_engine::project_activation::ActivationPort for NoActivation {
    type Handle = ();
    type Error = ();
    fn verify_lock(
        &mut self,
        _: yamaa_core::project_function::Language,
        _: &yamaa_core::project_environment::LockReference,
        _: &[yamaa_core::function_signature::ProjectFunctionIdentity],
    ) -> Result<(), ()> {
        panic!("no callable definitions")
    }
    fn bind(
        &mut self,
        _: &yamaa_core::function_signature::ProjectFunctionIdentity,
        _: &yamaa_core::function_signature::LogicalSignature,
    ) -> Result<(), ()> {
        panic!("no callable definitions")
    }
    fn invoke(
        &mut self,
        _: &(),
        _: &[yamaa_engine::function_invocation::Argument<'_>],
    ) -> Result<yamaa_core::value::Value, yamaa_engine::function_invocation::HostError<()>> {
        panic!("no callable definitions")
    }
}
fn project_domain(path: &str, environment: &str) -> file_application::Domain {
    let yamaa_adapters::file_workflow::Selected::Ordinary(mut project) =
        yamaa_adapters::file_workflow::select(
            path,
            environment,
            yamaa_core::project_function::Language::Python,
        )
        .unwrap_or_else(|error| {
            panic!(
                "{:?}",
                yamaa_adapters::project_application::selection_failure(
                    &error,
                    yamaa_core::project_function::Language::Python
                )
            )
        })
    else {
        panic!("ordinary metadata capability")
    };
    let attempt = project.build(&mut NoActivation);
    let result =
        yamaa_adapters::project_report::build_result(project.run(), &attempt, identity(path), &[])
            .unwrap();
    file_application::Domain::from_project(result, &project).unwrap()
}

#[test]
fn wide_length_projection_borrows_the_exact_bound_from_compiled_metadata() {
    use yamaa_core::{
        dataset::Check, diagnostic::ConditionCode, project_diagnostics::Fact, value::Value,
    };
    use yamaa_engine::dataset::{CheckRecord, RowIdentity};
    let study = Study::new();
    let bound = "340282366920938463463374607431768211456";
    fs::write(
        study.0.join("spec.yaml"),
        VALID.replace("length: 3", &format!("length: {bound}")),
    )
    .unwrap();
    let resources = yamaa_adapters::file_resources::Resources::new(
        study.0.to_str().unwrap(),
        study.0.to_str().unwrap(),
        &[],
    )
    .unwrap();
    let prepared =
        yamaa_adapters::file_preparation::FileSpecification::prepare(resources, "spec.yaml")
            .unwrap();
    let compiled = prepared.run().compiled();
    let path = "columns.VALUE.verifications.max_length";
    let Some(Check::WideMaxLength(maximum)) = compiled.column_check(path) else {
        panic!("retained exact length bound");
    };
    assert_eq!(maximum.decimal(), bound);
    // This control tests the formatter's retained observation boundary without
    // pretending a physical native string can exceed this configured bound.
    let records = [CheckRecord {
        path: path.into(),
        code: ConditionCode::VerificationMaxLengthFailed,
        evaluated_count: 1,
        failed_count: 1,
        output_rows: 1,
        offending_rows: vec![RowIdentity {
            position: 0,
            values: vec![Value::Int(1)],
        }],
        codelist: None,
        inventory: None,
    }];
    let projected = yamaa_engine::verification_diagnostics::project(
        compiled,
        &records,
        yamaa_engine::verification_diagnostics::Phase::Verification,
        "spec.yaml",
    )
    .unwrap();
    assert_eq!(
        projected.diagnostics[0].context["max"],
        Fact::Bound(maximum)
    );
    assert!(!study.0.join("input.csv").exists());
}

fn inventory_spec() -> String {
    VALID.replace("length: 3", "inventory_vocabulary: true, length: 10")
}
const INVENTORY_ENV: &str = "schema_version: '1.0'\nstudy: {id: STUDY, name: Study, description: Study dataset inventory, protocol_name: Protocol}\nsdtm:\n  standard: {name: SDTMIG, version: '3.4', status: Final}\n  specs: [catalog/renamed.yaml]\n  define: define.xml\n";
#[test]
fn absent_study_inventory_checks_nonmissing_values_and_keeps_missing_values_admitted() {
    for input in ["ID,VALUE\n1,DM\n2,\n", "ID,VALUE\n1,\n"] {
        let study = Study::new();
        fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let run = file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(path),
        )
        .unwrap();
        if input.contains("DM") {
            assert_eq!(run.issues().len(), 1);
            assert_eq!(
                (
                    run.issues()[0].phase.as_str(),
                    run.issues()[0].condition.as_str(),
                    run.issues()[0].requirement.as_deref()
                ),
                ("verification", "allowed_values_failed", Some("REQ-1156"))
            );
            assert_eq!(
                run.issues()[0].spec_paths,
                ["columns.VALUE.submission.inventory_vocabulary"]
            );
            assert_eq!(run.issues()[0].context,"{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":1}],\"values\":[\"DM\"]}");
        } else {
            assert!(run.issues().is_empty(), "{:?}", run.issues());
        }
    }
}
#[test]
fn inventory_uses_inherited_declared_dataset_names_and_retained_original_metadata_in_both_hosts() {
    for language in [
        yamaa_core::project_function::Language::Python,
        yamaa_core::project_function::Language::R,
    ] {
        let study = Study::new();
        fs::create_dir(study.0.join("catalog")).unwrap();
        fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
        fs::write(study.0.join("environment.yaml"), INVENTORY_ENV).unwrap();
        fs::write(
            study.0.join("catalog/renamed.yaml"),
            "schema_version: '1.0'\nparents: [parent.yaml]\n",
        )
        .unwrap();
        fs::write(
            study.0.join("catalog/parent.yaml"),
            VALID
                .replace("domain: TEST", "domain: DM")
                .replace("input.csv", "never-read.csv"),
        )
        .unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let environment = study.0.join("environment.yaml");
        let yamaa_adapters::file_workflow::Selected::Ordinary(mut project) =
            yamaa_adapters::file_workflow::select(path, environment.to_str().unwrap(), language)
                .unwrap_or_else(|error| {
                    panic!(
                        "{:?}",
                        yamaa_adapters::project_application::selection_failure(&error, language)
                    )
                })
        else {
            panic!("ordinary inventory capability")
        };
        assert_eq!(project.run().environment().inventory().names(), ["DM"]);
        let retained = project.run().captured_environment().inventory_documents();
        assert_eq!(retained.len(), 1);
        assert!(retained[0]
            .source()
            .identity
            .ends_with("catalog/renamed.yaml"));
        assert!(!study.0.join("catalog/never-read.csv").exists());
        assert!(!study.0.join("input.csv").exists());
        fs::write(study.0.join("input.csv"), "ID,VALUE\n1,DM\n2,renamed\n3,\n").unwrap();
        let attempt = project.build(&mut NoActivation);
        let result = yamaa_adapters::project_report::build_result(
            project.run(),
            &attempt,
            identity(path),
            &[],
        )
        .unwrap();
        let run = file_application::Domain::from_project(result, &project).unwrap();
        assert_eq!(run.issues().len(), 1);
        assert_eq!(run.issues()[0].requirement.as_deref(), Some("REQ-1156"));
        assert_eq!(run.issues()[0].context,"{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}],\"values\":[\"renamed\"]}");
        assert!(!study.0.join("define.xml").exists());
    }
}
#[test]
fn value_inventory_override_inherits_base_metadata_and_selects_only_its_test_code() {
    let study = Study::new();
    let raw = ROW_CT
        .replace("codelist: DEFAULT", "inventory_vocabulary: true")
        .replace("codelist: GLUCOSE", "inventory_vocabulary: false");
    fs::write(study.0.join("spec.yaml"), raw).unwrap();
    fs::write(study.0.join("input.csv"), "ID,VALUE\n1,DM\n2,\n").unwrap();
    let path = study.0.join("spec.yaml");
    let path = path.to_str().unwrap();
    let run = file_application::domain(
        Request {
            specification: path,
            environment: None,
        },
        identity(path),
    )
    .unwrap();
    assert_eq!(run.issues().len(), 1);
    assert_eq!(run.issues()[0].context,"{\"column\":\"RESULT\",\"failure_count\":1,\"keys\":[{\"ID\":1,\"LBTESTCD\":\"CREAT\"}],\"values\":[\"DM\"]}");
}

#[test]
fn multiple_value_codelist_records_keep_the_shared_path_in_success_and_failure_logs() {
    for fail in [false, true] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            ROW_CT.replace(
                "path: output.csv",
                "path: output.csv, verification_log: checks.csv",
            ),
        )
        .unwrap();
        let environment = if fail {
            CT_ENV.to_owned()
        } else {
            CT_ENV.replace("items: [{value: G}]", "items: [{value: D}]")
        };
        fs::write(study.0.join("environment.yaml"), environment).unwrap();
        fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let environment = study.0.join("environment.yaml");
        let mut run = project_domain(path, environment.to_str().unwrap());
        assert_eq!(run.issues().len(), usize::from(fail));
        assert_eq!(run.log_table(false).unwrap().rows(), 2);
        assert!(!study.0.join("checks.csv").exists());
        let saved = run.save();
        assert_eq!(saved.is_ok(), !fail);
        let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
        assert_eq!(
            checks.matches("columns.RESULT.submission.codelist").count(),
            2
        );
        assert_eq!(checks.matches("REQ-0957").count(), 2);
        assert_eq!(study.0.join("output.csv").exists(), !fail);
    }
}

fn value_length_spec(length: &str) -> String {
    ROW_CT
        .replace("codelist: DEFAULT", "length: 3")
        .replace("codelist: GLUCOSE", &format!("length: {length}"))
}
#[test]
fn per_value_lengths_replace_inherited_bounds_only_for_their_completed_test_code() {
    for (bound, input, expected_path, expected_key, maximum) in [
        (
            "5",
            "ID,VALUE\n1,abcde\n2,é字😀\n",
            "columns.RESULT.verifications.max_length",
            "CREAT",
            3,
        ),
        (
            "2",
            "ID,VALUE\n1,abc\n2,é字\n3,\n",
            "rows.glucose.submission.RESULT.length",
            "GLUC",
            2,
        ),
    ] {
        let study = Study::new();
        fs::write(study.0.join("spec.yaml"), value_length_spec(bound)).unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let mut run = file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(path),
        )
        .unwrap();
        assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
        let issue = &run.issues()[0];
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.condition.as_str(),
                issue.requirement.as_deref()
            ),
            ("verification", "length_failed", Some("REQ-0378"))
        );
        assert_eq!(issue.spec_paths, [expected_path]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issue.context).unwrap(),
            serde_json::json!({"column":"RESULT","failure_count":1,"keys":[{"ID":1,"LBTESTCD":expected_key}],"max":maximum})
        );
        assert!(run.save().is_err());
    }
}
#[test]
fn equal_per_value_lengths_run_one_shared_check_and_wide_bounds_keep_exact_metadata() {
    for length in ["3", "340282366920938463463374607431768211456"] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            value_length_spec(length).replace(
                "path: output.csv",
                "path: output.csv, verification_log: checks.csv",
            ),
        )
        .unwrap();
        fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,\n").unwrap();
        let path = study.0.join("spec.yaml");
        let path = path.to_str().unwrap();
        let mut run = file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(path),
        )
        .unwrap();
        assert!(run.issues().is_empty(), "{:?}", run.issues());
        assert_eq!(
            run.log_table(false).unwrap().rows(),
            if length == "3" { 1 } else { 2 }
        );
        assert!(run.save().unwrap());
        assert_eq!(
            fs::read_to_string(study.0.join("output.csv")).unwrap(),
            "ID,RESULT,LBTESTCD\n1,é字😀,GLUC\n2,,GLUC\n1,é字😀,CREAT\n2,,CREAT\n"
        );
    }
}
#[test]
fn effective_value_metadata_rejects_invalid_bounds_types_and_explicit_length_conflicts_before_data()
{
    for (declaration, condition, path) in [
        (
            "length: -999999999999999999999999999999",
            "submission_length_invalid",
            "rows.glucose.submission.RESULT.length",
        ),
        (
            "length: 0",
            "submission_length_invalid",
            "rows.glucose.submission.RESULT.length",
        ),
        (
            "data_type: float",
            "submission_data_type_not_admitted",
            "rows.glucose.submission.RESULT",
        ),
        (
            "significant_digits: -999999999999999999999999999999",
            "submission_length_invalid",
            "rows.glucose.submission.RESULT.significant_digits",
        ),
        (
            "mandatory: true",
            "mandatory_not_enforced",
            "rows.glucose.submission.RESULT",
        ),
    ] {
        let study = Study::new();
        let raw = ROW_CT
            .replace("codelist: DEFAULT", "length: 3")
            .replace("codelist: GLUCOSE", declaration);
        let issues = study.check(&raw);
        assert_eq!(issues.len(), 1, "{declaration}: {issues:?}");
        assert_eq!(issues[0].condition, condition);
        assert_eq!(issues[0].spec_paths, [path]);
        assert!(!study.0.join("input.csv").exists());
    }
    let study = Study::new();
    let raw = value_length_spec("4").replace(
        "derivation: SRC.VALUE, submission",
        "derivation: SRC.VALUE, verifications: {max_length: {max: 3}}, submission",
    );
    let issues = study.check(&raw);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(
        (
            issues[0].condition.as_str(),
            issues[0].requirement.as_deref()
        ),
        ("declared_length_conflict", Some("REQ-0913"))
    );
    assert_eq!(issues[0].spec_paths, ["rows.glucose.submission.RESULT"]);
    assert_eq!(
        issues[0].context,
        "{\"column\":\"RESULT\",\"length\":4,\"max_length\":3}"
    );
}

#[test]
fn effective_value_fields_inherit_full_metadata_and_refute_inapplicable_lengths_and_digits() {
    let root = "\nsubmission: {label: Laboratory Results, class: FINDINGS, structure: One record per test, repeating: true}\n";
    let base = format!(
        "{}{root}",
        value_length_spec("3").replace(
            "origin: {type: Assigned}, length: 3",
            "core: Perm, origin: {type: Assigned}, length: 3"
        )
    );
    let study = Study::new();
    let baseline = study.check(&base);
    assert!(baseline.is_empty(), "{baseline:?}");
    for (entry, field) in [
        ("data_type: date", "data_type"),
        ("significant_digits: 2", "field"),
    ] {
        let raw = base.replace(
            "submission: {RESULT: {length: 3}}",
            &format!("submission: {{RESULT: {{{entry}}}}}"),
        );
        let issues = study.check(&raw);
        assert_eq!(issues.len(), 1, "{entry}: {issues:?}");
        assert_eq!(
            (
                issues[0].condition.as_str(),
                issues[0].requirement.as_deref()
            ),
            ("submission_length_not_applicable", Some("REQ-0912"))
        );
        assert_eq!(issues[0].spec_paths, ["rows.glucose.submission.RESULT"]);
        let context = serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap();
        assert_eq!(context["column"], "RESULT");
        assert_eq!(
            context[field],
            if field == "field" {
                "significant_digits"
            } else {
                "date"
            }
        );
    }
    assert!(!study.0.join("input.csv").exists());
}
