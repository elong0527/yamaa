#![cfg(feature = "yaml")]
//! Execution contracts read from shared truth: declarative source selection,
//! diagnostic sites, intermediate dependencies, intermediate row drivers, key
//! expressions, namespace admission, the window defaults of an output key,
//! and predicate suggestions. The family's other cases are in
//! `tests/fixtures/diagnostics/`: `declarative_self.json`,
//! `inheritance.json`, `integer_literals.json`, `windows.json`,
//! `verification.json`, `missing_order.json` and `empty_child.json` (#3798);
//! `api_contracts` holds the build and save seams the replay cannot express.

mod execution {
    mod declarative_source_selection {
        //! Independently authored typed public results and ordered source-selection findings.
        //!
        //! These stay beside the replay (#3798): each reads a fixture that
        //! `tools/staging/stage_specification_inputs.py` also stages for both
        //! installed hosts, so its truth keeps that one form, and each checks
        //! what a replay case cannot state, a save repeated after the input
        //! changes and every run made twice.
        use std::fs;
        use yamaa_adapters::{
            file_application, public_table::PublicTable, specification_report::Identity,
        };
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(case: &yamaa_core::json::Value) -> Self {
                let root = crate::support::Scratch::new("source-selection");
                fs::write(
                    root.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    fs::write(root.join(path), bytes.as_str().unwrap()).unwrap();
                }
                Self(root)
            }
        }
        fn issues(rows: &[Diagnostic]) -> yamaa_core::json::Value {
            yamaa_core::json::Value::Array(
                rows.iter()
                    .map(|issue| {
                        let mut row = yamaa_adapters::report_storage::diagnostic_value(issue);
                        assert!(row["context"].is_object());
                        row["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                        row
                    })
                    .collect(),
            )
        }
        fn table(view: &PublicTable) -> yamaa_core::json::Value {
            use arrow_array::{Array, Int64Array, StringArray};
            let columns: Vec<_> = view
                .columns()
                .iter()
                .map(|field| {
                    let kind = match field.data_type() {
                        arrow_schema::DataType::Utf8 => "str",
                        arrow_schema::DataType::Int64 => "int",
                        _ => panic!("unexpected control type"),
                    };
                    yamaa_adapters::json!([field.name(), kind])
                })
                .collect();
            let rows: Vec<Vec<Option<String>>> = view
                .batches()
                .iter()
                .flat_map(|batch| {
                    (0..batch.num_rows()).map(move |row| {
                        batch
                            .columns()
                            .iter()
                            .map(|array| {
                                if array.is_null(row) {
                                    return None;
                                }
                                match array.data_type() {
                                    arrow_schema::DataType::Utf8 => Some(
                                        array
                                            .as_any()
                                            .downcast_ref::<StringArray>()
                                            .unwrap()
                                            .value(row)
                                            .to_owned(),
                                    ),
                                    arrow_schema::DataType::Int64 => Some(
                                        array
                                            .as_any()
                                            .downcast_ref::<Int64Array>()
                                            .unwrap()
                                            .value(row)
                                            .to_string(),
                                    ),
                                    _ => panic!("unexpected control type"),
                                }
                            })
                            .collect()
                    })
                })
                .collect();
            yamaa_adapters::json!({"columns": columns, "rows": rows})
        }
        fn identity(specification: &str) -> Identity<'_> {
            crate::support::study::identity("independent-source-selection", specification)
        }

        fn verify_truth(fixture: &str) {
            let truth: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(fixture).unwrap();
            for (name, case) in truth["cases"].as_object().unwrap() {
                for _ in 0..2 {
                    let fixture = Fixture::new(case);
                    let entry = fixture.0.join("spec.yaml");
                    let written = entry.to_str().unwrap();
                    let request = || Request {
                        specification: written,
                        environment: None,
                    };
                    assert_eq!(
                        issues(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(written)
                        )),
                        case["check_issues"],
                        "{name}: static check"
                    );
                    let mut result = crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(written),
                    )
                    .unwrap();
                    assert_eq!(
                        issues(&result.issues()),
                        case["issues"],
                        "{name}: complete issues"
                    );
                    let report = result.observations();
                    assert_eq!(
                        report["handler_counts"], case["handlers"],
                        "{name}: handler ledger"
                    );
                    let reads: Vec<_> = report["source_reads"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|read| {
                            assert_eq!(read["outcome"], "captured", "{name}");
                            assert_eq!(read["snapshots_created"], 1, "{name}");
                            read["path"].clone()
                        })
                        .collect();
                    assert_eq!(
                        yamaa_adapters::json!(reads),
                        case["source_reads"],
                        "{name}: capture order"
                    );
                    assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
                    if let Some(expected) = case.get("table") {
                        assert_eq!(
                            table(result.output().unwrap()),
                            *expected,
                            "{name}: typed output"
                        );
                        assert!(result.save().unwrap(), "{name}");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes(),
                            "{name}: exact CSV"
                        );
                        fs::write(fixture.0.join("input.csv"), b"changed after build").unwrap();
                        assert!(result.save().unwrap(), "{name}: retained save");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes(),
                            "{name}: retained bytes"
                        );
                    } else {
                        assert!(result.output().is_none(), "{name}");
                        assert!(
                            result.save().is_err(),
                            "{name}: failed build cannot publish"
                        );
                        assert!(!fixture.0.join("result.csv").exists(), "{name}");
                    }
                }
            }
        }

        /// A seam the replay cannot reach: the Rust reader of
        /// `source_selection.json`, which both installed hosts' suites stage
        /// too, run twice, with each save repeated after its input changes.
        /// Each case is an authored complete result, its typed table, issues,
        /// handler ledger, capture order and retained save, which no benchmark
        /// states.
        #[test]
        fn every_source_selection_case_keeps_exact_types_diagnostics_counts_and_saved_bytes() {
            verify_truth(include_str!("fixtures/source_selection.json"));
        }
        /// A seam the replay cannot reach: the Rust reader of
        /// `source_filter_scope.json`, which both installed hosts' suites
        /// stage too, run twice, with each save repeated after its input
        /// changes. Each case is an authored complete result, its typed table,
        /// issues, handler ledger, capture order and retained save, which no
        /// benchmark states.
        #[test]
        fn group_key_filters_have_complete_normative_findings_and_record_controls() {
            verify_truth(include_str!("fixtures/source_filter_scope.json"));
        }

        /// A seam the replay cannot reach: the Rust reader of
        /// `source_filter_mixed.json`, which both installed hosts' suites
        /// stage too, run twice, with each save repeated after its input
        /// changes. Each case is an authored complete result, its typed table,
        /// issues, handler ledger, capture order and retained save, which no
        /// benchmark states.
        #[test]
        fn mixed_grouped_and_record_templates_preserve_only_shared_group_key_scalars() {
            verify_truth(include_str!("fixtures/source_filter_mixed.json"));
        }
    }
    mod diagnostic_sites {
        //! Original public unknown-reference findings retain the authored diagnostic site.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read, and it checks each specification
        //! before and after its inputs exist, where a replay case writes one
        //! set of files.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        /// Reads the shared host-fixture family `diagnostic_sites`: authored
        /// refusals, checked before and after their inputs exist, that no
        /// benchmark states.
        #[test]
        fn unknown_references_preserve_original_identifiers_and_dataset_context() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["diagnostic_sites"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("diagnostic-sites"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("diagnostic-sites", path);
                assert_eq!(
                    rows(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"]
                );
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    let input = fixture.0.join(input);
                    fs::create_dir_all(input.parent().unwrap()).unwrap();
                    fs::write(input, bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let issues = match crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    ) {
                        Ok(mut result) => {
                            assert!(result.save().is_err());
                            result.issues().to_vec()
                        }
                        Err(issues) => issues,
                    };
                    assert_eq!(
                        rows(&issues),
                        case["issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    assert!(!fixture.0.join(case["output"].as_str().unwrap()).exists());
                }
            }
        }
    }
    mod intermediate_dependencies {
        //! Original cyclic donor reads and valid dependency order through public routes.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read, and it checks each specification
        //! before and after its inputs exist, where a replay case writes one
        //! set of files.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        /// Reads the shared host-fixture family `intermediate_dependencies`:
        /// authored cycles and forward reads, checked before and after their
        /// inputs exist, that no benchmark states.
        #[test]
        fn cyclic_donor_reads_preserve_original_findings_and_forward_dependencies_execute() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["intermediate_dependencies"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("intermediate-dependencies"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity =
                    || crate::support::study::identity("intermediate-dependencies", path);
                let failing = !case["issues"].as_array().unwrap().is_empty();
                if failing {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["issues"]
                    );
                    assert_eq!(
                        rows(
                            &crate::support::study::domain(
                                yamaa_core::project_function::Language::Python,
                                request(),
                                identity()
                            )
                            .err()
                            .unwrap()
                        ),
                        case["issues"]
                    );
                }
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    let input = fixture.0.join(input);
                    fs::create_dir_all(input.parent().unwrap()).unwrap();
                    fs::write(input, bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    let checked = file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    );
                    assert_eq!(
                        rows(&checked),
                        case["issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    if failing {
                        assert_eq!(
                            rows(
                                &crate::support::study::domain(
                                    yamaa_core::project_function::Language::Python,
                                    request(),
                                    identity()
                                )
                                .err()
                                .unwrap()
                            ),
                            case["issues"]
                        );
                        assert!(!fixture.0.join("out.csv").exists());
                    } else {
                        let mut result = crate::support::study::domain(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(),
                        )
                        .unwrap();
                        assert!(
                            result.issues().is_empty(),
                            "{}: {:?}",
                            yamaa_adapters::json_wire::encode(&case["name"]),
                            result.issues()
                        );
                        assert!(result.save().unwrap());
                        assert_eq!(
                            fs::read(fixture.0.join("out.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes()
                        );
                        fs::remove_file(fixture.0.join("out.csv")).unwrap();
                    }
                }
            }
        }
    }
    mod intermediate_row_drivers {
        //! Named row drivers traverse filtered donor records without lookup keys.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[yamaa_core::diagnostic::Diagnostic]) -> yamaa_core::json::Value {
            yamaa_adapters::json!(rows
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>())
        }
        /// Reads the shared host-fixture family `intermediate_row_drivers`:
        /// authored row drivers over named intermediates, with their check
        /// route, that no benchmark states.
        #[test]
        fn source_only_named_row_drivers_preserve_lookup_key_admission() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["intermediate_row_drivers"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("intermediate-row-drivers"));
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    let path = fixture.0.join(path);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, bytes.as_str().unwrap()).unwrap();
                }
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("intermediate_row_drivers", path);
                assert_eq!(
                    issues(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let mut result = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                ) {
                    Ok(result) => result,
                    Err(rows) => {
                        assert_eq!(
                            issues(&rows),
                            case["issues"],
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(case["csv"].is_null());
                        assert!(!fixture.0.join("out.csv").exists());
                        continue;
                    }
                };
                assert_eq!(
                    issues(&result.issues()),
                    case["issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                if let Some(csv) = case["csv"].as_str() {
                    assert!(result.save().unwrap());
                    assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), csv.as_bytes());
                } else {
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("out.csv").exists());
                }
            }
        }
    }
    mod key_expressions {
        //! Independent current-row key operands through complete file check/build/save routes.
        //!
        //! These stay beside the replay (#3798): each reads the host-fixture
        //! family both installed hosts read, the first checking each
        //! specification before and after its inputs are captured, and the
        //! second running one of its specifications with inputs missing.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                Self(crate::support::Scratch::new("key-expressions"))
            }
        }
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        /// Reads the shared host-fixture family `key_expressions`: authored
        /// expression keys, with their check route, handler ledgers and saved
        /// bytes, that no benchmark states.
        #[test]
        fn scalar_and_window_keys_reuse_current_operands_types_handlers_and_independent_saved_truth(
        ) {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["key_expressions"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture::new();
                let entry = fixture.0.join("spec.yaml");
                fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
                let entry = entry.to_str().unwrap();
                let request = || Request {
                    specification: entry,
                    environment: None,
                };
                let identity = || crate::support::study::identity("key-expressions", entry);
                // A refusal binding decides from the declarations is reported
                // by the build only once it has captured the inputs.
                let after_capture = case
                    .get("check_after_capture")
                    .and_then(yamaa_core::json::Value::as_bool)
                    == Some(true);
                if !case["check_issues"].as_array().unwrap().is_empty() && !after_capture {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{} before input capture",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let issues = match crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    ) {
                        Ok(mut result) => {
                            assert!(
                                result.output().is_none(),
                                "{}",
                                yamaa_adapters::json_wire::encode(&case["name"])
                            );
                            assert!(
                                !result.save().unwrap_or(false),
                                "{}",
                                yamaa_adapters::json_wire::encode(&case["name"])
                            );
                            result.issues()
                        }
                        Err(issues) => issues,
                    };
                    assert_eq!(
                        rows(&issues),
                        case["check_issues"],
                        "{} build before input capture",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    assert!(
                        !fixture.0.join("out.csv").exists(),
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                }
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    fs::write(fixture.0.join(input), bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{} check",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let result = crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    );
                    if case["issues"].as_array().unwrap().is_empty() {
                        let mut result = result.unwrap_or_else(|issues| {
                            panic!(
                                "{}: {:?}",
                                yamaa_adapters::json_wire::encode(&case["name"]),
                                rows(&issues)
                            )
                        });
                        assert!(
                            result.issues().is_empty(),
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert_eq!(
                            result.observations()["handler_counts"],
                            case["handlers"],
                            "{} handlers",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(result.save().unwrap());
                        assert_eq!(
                            fs::read(fixture.0.join("out.csv")).unwrap(),
                            case["csv"].as_str().unwrap().as_bytes(),
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        fs::remove_file(fixture.0.join("out.csv")).unwrap();
                    } else {
                        let issues = match result {
                            Ok(mut result) => {
                                let issues = result.issues();
                                assert!(!result.save().unwrap_or(false));
                                issues
                            }
                            Err(issues) => issues,
                        };
                        assert_eq!(
                            rows(&issues),
                            case["issues"],
                            "{} build",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(!fixture.0.join("out.csv").exists());
                    }
                }
            }
        }

        /// A seam the replay cannot reach: one case of the shared
        /// host-fixture family `key_expressions`, which both installed hosts
        /// read, run with its inputs missing.
        /// `negative-path-missing` and `negative-path-directory` refuse one
        /// missing input and a directory in an input's place; no benchmark
        /// meets either beside an ill-typed filter or an output declaration
        /// fault left for a later checkpoint, nor reports two inputs at once.
        #[test]
        fn missing_sources_keep_filter_types_but_other_resource_and_output_checkpoints_stay_separate(
        ) {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            let case = truth["families"]["original-specifications"]["key_expressions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| {
                    case["name"]
                        == "generic-key-valid-cardinality-retains-donor-filter-type-refusal"
                })
                .unwrap();
            let fixture = Fixture::new();
            let entry = fixture.0.join("spec.yaml");
            let specification = case["specification"].as_str().unwrap();
            fs::write(&entry, specification).unwrap();
            let refused = || {
                let entry = entry.to_str().unwrap();
                let mut result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: entry,
                        environment: None,
                    },
                    crate::support::study::identity("missing-key-expression-input", entry),
                )
                .unwrap_or_else(|issues| panic!("{:?}", rows(&issues)));
                assert!(result.output().is_none());
                assert_eq!(
                    result.observations()["source_reads"],
                    yamaa_adapters::json!([])
                );
                assert!(!result.save().unwrap_or(false));
                assert!(!fixture.0.join("out.csv").exists());
                rows(&result.issues())
            };
            for present in ["src.csv", "tab.csv"] {
                fs::write(
                    fixture.0.join(present),
                    case["inputs"][present].as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(refused(), case["check_issues"], "only {present} exists");
                fs::remove_file(fixture.0.join(present)).unwrap();
            }
            let missing_sources = yamaa_adapters::json!([
                {"phase": "validation", "condition": "resource_path_missing", "rule": "FILE-7",
                 "spec_paths": ["input.SRC.path"], "context": {"dataset": "SRC", "path": "src.csv"}},
                {"phase": "validation", "condition": "resource_path_missing", "rule": "FILE-7",
                 "spec_paths": ["input.TAB.path"], "context": {"dataset": "TAB", "path": "tab.csv"}},
            ]);
            fs::create_dir(fixture.0.join("tab.csv")).unwrap();
            let mut mixed_resources = missing_sources.clone();
            mixed_resources[1]["condition"] =
                yamaa_adapters::json!("resource_path_not_regular_file");
            assert_eq!(refused(), mixed_resources);
            fs::remove_dir(fixture.0.join("tab.csv")).unwrap();

            // A valid filter has no static refusal. Even an independently known output
            // declaration error retains its later checkpoint when sources are missing.
            fs::write(
                &entry,
                specification
                    .edit("TAB.KINT > 'bad'", "TAB.KINT > 0")
                    .edit("path: out.csv", "path: out.csv\n  decimals: 19"),
            )
            .unwrap();
            assert_eq!(refused(), missing_sources);
        }
    }
    mod namespace_admission {
        //! The original namespace obligations through complete public check and build.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);

        /// Reads the shared host-fixture family `namespace_admission`:
        /// authored namespace refusals, compared between the check route and
        /// the build, that no benchmark states.
        #[test]
        fn authored_namespace_refusals_precede_build_and_source_access() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["namespace_admission"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::canonical("namespace"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                // Deliberately absent: an invalid namespace grants no source authority.
                let path = path.to_str().unwrap();
                // `{entry}` stands for the specification's absolute path as
                // the engine resolves it, with forward slashes.
                let expected = yamaa_adapters::json_wire::decode(
                    &yamaa_adapters::json_wire::encode(&case["issues"]).replace(
                        "\"{entry}\"",
                        &yamaa_adapters::json_wire::encode(&yamaa_adapters::json!(
                            path.replace('\\', "/")
                        )),
                    ),
                )
                .unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("namespace_admission", path);
                let checked = file_application::check(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                );
                let built = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .err()
                .unwrap();
                assert_eq!(
                    checked,
                    built,
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let rows = checked
                    .iter()
                    .map(crate::support::study::issue_row)
                    .collect::<Vec<_>>();
                assert_eq!(
                    yamaa_adapters::json!(rows),
                    expected,
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                assert!(!fixture.0.join("out.csv").exists());
            }
        }
    }
    mod nested_windows {
        //! The window defaults an output key reads transitively, through two
        //! benchmarks. This stays beside the replay (#3798): it reproduces
        //! benchmarks, which a replay case cannot run. The family's other
        //! cases are in `tests/fixtures/diagnostics/windows.json`.
        /// `adam-adlb-bds` keys its rows by `ASEQ`, a column window over the
        /// rows of three filtered templates ordered by the `PARAMCD` each
        /// template derives, and `sdtm-dm-race-ethnicity` keys `SUPPDM` by
        /// `QNAM`, a `str_template` over a column copied from a window.
        #[test]
        fn output_key_template_keeps_transitive_window_defaults_over_all_constructed_rows() {
            for benchmark in ["adam-adlb-bds", "sdtm-dm-race-ethnicity"] {
                crate::support::benchmarks::reproduce(benchmark);
            }
        }
    }
    mod predicate_suggestions {
        //! Original qualified-name suggestions and their unknown/ambiguous neighbors.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[yamaa_core::diagnostic::Diagnostic]) -> yamaa_core::json::Value {
            yamaa_adapters::json!(rows
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>())
        }
        /// Reads the shared host-fixture family `predicate_suggestions`:
        /// authored suggestions and unknown names, with their check route,
        /// that no benchmark states.
        #[test]
        fn original_predicate_suggestions_preserve_scopes_and_unknown_names() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["predicate_suggestions"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("predicate-suggestions"));
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    let path = fixture.0.join(path);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, bytes.as_str().unwrap()).unwrap();
                }
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("predicate_suggestions", path);
                assert_eq!(
                    issues(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let mut result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .unwrap();
                assert_eq!(
                    issues(&result.issues()),
                    case["issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                if let Some(csv) = case["csv"].as_str() {
                    assert!(result.save().unwrap());
                    assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), csv.as_bytes());
                } else {
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("out.csv").exists());
                }
            }
        }
    }
    mod api_contracts {
        //! Seams the replay cannot express: required preparation routes,
        //! output presence, save return values, publication timing and a
        //! second save after input changes. Expectations come from the
        //! original tests, independently of the replay's issue/byte checks.
        use crate::support::diagnostic::{operations, replay};
        use std::{collections::BTreeSet, fs};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        #[test]
        fn migrated_cases_preserve_build_and_save_contracts() {
            let contracts = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/declarative_self_contracts.json"
            ))
            .unwrap();
            replay::closed(&contracts, &["about", "cases"]);
            let contracts = replay::field(&contracts, "cases").as_object().unwrap();
            assert!(
                !contracts.is_empty(),
                "the migrated families have API contracts"
            );
            let families = [
                "declarative_self.json::",
                "inheritance.json::",
                "integer_literals.json::",
                "verification.json::",
                "windows.json::",
                "missing_order.json::",
            ];
            let cases = replay::cases()
                .into_iter()
                .filter(|case| {
                    families.iter().any(|family| case.label.starts_with(family))
                        && case
                            .operations
                            .iter()
                            .any(|operation| matches!(operation.as_str(), "build" | "save"))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                cases
                    .iter()
                    .map(|case| &case.label)
                    .collect::<BTreeSet<_>>(),
                contracts.keys().collect::<BTreeSet<_>>(),
                "every migrated build/save case has exactly one API contract"
            );
            for case in cases {
                let expected = &contracts[&case.label];
                replay::closed(
                    expected,
                    &["route", "primary", "absent", "save", "retained"],
                );
                let route = replay::text(replay::field(expected, "route"));
                assert!(matches!(route, "domain" | "refused" | "either"));
                let study = operations::study(&case.input);
                let primary = study.join(replay::text(replay::field(expected, "primary")));
                let absent = || {
                    assert!(
                        !primary.try_exists().unwrap(),
                        "{}: primary is absent",
                        case.label
                    );
                    for input in replay::items(replay::field(expected, "absent")) {
                        assert!(
                            !study.join(replay::text(input)).try_exists().unwrap(),
                            "{}: {input:?} stays absent",
                            case.label
                        );
                    }
                };
                absent();
                let name = case.input["specification"].as_str().unwrap();
                let specification = study.join(name).to_str().unwrap().replace('\\', "/");
                let host = case.input.get("host").and_then(|host| host.as_str());
                let built = crate::support::study::domain(
                    crate::support::host(host.unwrap_or("python")),
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    crate::support::study::identity("diagnostics", &specification),
                );
                let actual_route = if built.is_ok() { "domain" } else { "refused" };
                assert!(
                    route == "either" || route == actual_route,
                    "{}: expected {route}, got {actual_route}",
                    case.label
                );
                absent();
                match built {
                    Ok(mut result) => {
                        replay::compare_issues(&case, &result.issues(), Some(&study), &case.label);
                        assert_eq!(
                            result.output().is_some(),
                            case.issues.is_empty(),
                            "{}: output presence",
                            case.label
                        );
                        match expected["save"].as_str() {
                            Some("true") => {
                                assert!(
                                    result.save().unwrap(),
                                    "{}: successful save returns true",
                                    case.label
                                );
                                if expected["retained"].as_bool().unwrap() {
                                    fs::write(study.join("input.csv"), "changed after build")
                                        .unwrap();
                                    assert!(
                                        result.save().unwrap(),
                                        "{}: retained save returns true",
                                        case.label
                                    );
                                    for (path, stated) in
                                        case.stated.saved.as_ref().expect("authored saved bytes")
                                    {
                                        let bytes = fs::read(study.join(path)).unwrap();
                                        let label = format!("{}: retained", case.label);
                                        stated.compare(path, Some(&bytes), &label);
                                    }
                                }
                            }
                            Some("failed") => {
                                assert!(
                                    matches!(result.save(), Err(file_application::FailedBuild)),
                                    "{}: refused save fails",
                                    case.label
                                );
                                absent();
                            }
                            None => assert!(expected["save"].is_null()),
                            other => panic!("{}: unknown save contract {other:?}", case.label),
                        }
                    }
                    Err(issues) => {
                        replay::compare_issues(&case, &issues, Some(&study), &case.label)
                    }
                }
            }
        }
    }
}
