#![cfg(feature = "yaml")]
//! Execution contracts for completed intermediates that read authored truth:
//! the benchmarks of named intermediates and their refusals, and the
//! completed lookup assertions both installed hosts read. The family's other
//! cases, including the handlers of `adam-adtte-first-ae`'s case branch, are
//! `fixtures/diagnostics/completed_intermediates.json`.
mod execution {
    mod completed_intermediate_scopes {
        //! Original committed YAML and independent expected artifacts remain the oracle.
        use std::{fs, path::Path};
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let p = crate::support::Scratch::new("intermediate-scopes");
                Self(p)
            }
        }
        fn copy(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for item in fs::read_dir(source).unwrap() {
                let item = item.unwrap();
                if item.file_type().unwrap().is_dir() {
                    copy(&item.path(), &destination.join(item.file_name()));
                } else {
                    fs::copy(item.path(), destination.join(item.file_name())).unwrap();
                }
            }
        }
        fn issue(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }

        /// Already data: each benchmark reads named intermediates and states
        /// the CSV it saves, which `reproduce` compares byte for byte.
        #[test]
        fn completed_intermediate_benchmarks_reproduce_their_expected_files() {
            for name in [
                "adam-adtte-first-ae",
                "schema-lookup-intermediate-unique",
                "sdtm-ae-meddra-hierarchy",
                "sdtm-cm-whodrug-coding",
                "sdtm-vs-collected-form",
                "sdtm-lb-last-observation-before-exposure",
                "sdtm-vs-planned-time-points",
                "sdtm-qs-questionnaire-items",
                "sdtm-fa-event-findings",
                "sdtm-ex-from-ec",
                "schema-lookup-intermediate-derived-filter",
                "schema-lookup-intermediate-correlated-derive",
                "schema-window-intermediate-rank",
                "adam-adlb-end-of-treatment",
                "adam-adae-prior-serious-event",
            ] {
                crate::support::benchmarks::reproduce(name);
            }
        }

        /// Already data: each benchmark's refusal holds exactly what its
        /// `error.yaml` states and is the run's only issue, which that file,
        /// a partial assertion about the first issue, leaves out.
        #[test]
        fn completed_intermediate_refusals_are_their_runs_only_issue() {
            for name in [
                "negative-lookup-duplicate-intermediate",
                "negative-lookup-id-collision",
            ] {
                let original = crate::support::benchmarks::path(name);
                let fixture = Fixture::new();
                copy(&original.join("input"), &fixture.0.join("input"));
                fs::copy(original.join("spec.yaml"), fixture.0.join("spec.yaml")).unwrap();
                let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let issues = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &spec,
                        environment: None,
                    },
                    crate::support::study::identity(name, &spec),
                ) {
                    Ok(mut result) => {
                        assert!(result.save().is_err(), "{name}");
                        result.issues()
                    }
                    Err(issues) if name == "negative-lookup-id-collision" => issues,
                    Err(issues) => panic!("{name}: refused before its run: {issues:?}"),
                };
                assert_eq!(
                    issues.iter().map(issue).collect::<Vec<_>>(),
                    [crate::support::benchmarks::error(name)],
                    "{name}"
                );
            }
        }
    }

    mod completed_lookup_assertions {
        //! Independent completed-row assertion scope, order and whole-record truth.
        use std::fs;
        use yamaa_application::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[Diagnostic]) -> yamaa_core::json::Value {
            yamaa_core::json::Value::Array(
                rows.iter()
                    .map(|issue| {
                        let mut value = yamaa_adapters::report_storage::diagnostic_value(issue);
                        value["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                        value
                    })
                    .collect(),
            )
        }
        /// Seam: the Rust reader of `completed_lookup_assertions.json`, the
        /// truth both installed hosts read once `tools/staging` stages it. It
        /// compares what the replay's operations do not report: each
        /// verification record of the observations, the empty artifact list,
        /// a second run of each case and a second save.
        ///
        /// `negative-verification-lookup-key` refuses an assertion's lookup
        /// for its key before any record is read. No benchmark runs an
        /// assertion that reads a lookup, whose `check` issues, run issues,
        /// handler ledger and verification records the crate fixture's cases
        /// state whole.
        #[test]
        fn complete_independent_lookup_assertions_keep_declaring_sites_and_prior_records() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/completed_lookup_assertions.json"
            ))
            .unwrap();
            for (name, case) in truth["cases"].as_object().unwrap() {
                for _ in 0..2 {
                    let fixture = Fixture(crate::support::Scratch::new("completed-assert"));
                    let entry = fixture.0.join("spec.yaml");
                    fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
                    for (path, content) in case["inputs"].as_object().unwrap() {
                        fs::write(fixture.0.join(path), content.as_str().unwrap()).unwrap();
                    }
                    let written = entry.to_str().unwrap();
                    let identity = || crate::support::study::identity("completed-assert", written);
                    let request = || Request {
                        specification: written,
                        environment: None,
                    };
                    assert_eq!(
                        issues(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{name}: check"
                    );
                    let mut result = crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    )
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                    assert_eq!(
                        issues(&result.issues()),
                        case["issues"],
                        "{name}: complete findings"
                    );
                    let report = result.observations();
                    assert_eq!(
                        report["handler_counts"], case["handlers"],
                        "{name}: declared handler ledger"
                    );
                    let mut expected = case["verifications"].clone();
                    for record in expected.as_array_mut().unwrap() {
                        record["specification"] = written.into();
                    }
                    assert_eq!(
                        report["verifications"], expected,
                        "{name}: complete ordered verification records"
                    );
                    assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
                    if let Some(csv) = case.get("csv") {
                        assert!(result.output().is_some(), "{name}");
                        assert!(result.save().unwrap(), "{name}");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            csv.as_str().unwrap().as_bytes(),
                            "{name}: exact CSV"
                        );
                        assert!(result.save().unwrap());
                    } else {
                        assert!(result.output().is_none(), "{name}");
                        assert!(result.save().is_err(), "{name}");
                        assert!(!fixture.0.join("result.csv").exists());
                    }
                }
            }
        }
    }
}
