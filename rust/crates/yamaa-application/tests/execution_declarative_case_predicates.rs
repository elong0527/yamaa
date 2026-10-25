#![cfg(feature = "yaml")]
//! Execution contracts: declarative case predicates, declarative flags,
//! declarative intermediate keys, declarative intermediate ranges,
//! declarative output ordering and declarative row intermediates. Their cases
//! that write files, run one specification and compare what it reports are
//! cases of `tests/fixtures/diagnostics/declarative_case_predicates.json`
//! (#3798). The tests here read what the replay does not: benchmarks, the
//! truth files the installed hosts share, and a Parquet input rendered for
//! the test.
mod execution {
    mod declarative_case_predicates {
        //! Independently authored full case reports and unchanged original CSV truth.
        use std::fs;
        use yamaa_application::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("case-predicates");
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
                let path = self.0.join("spec.yaml");
                fs::write(&path, body).unwrap();
                let specification = path.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification,
                        environment: None,
                    },
                    crate::support::study::identity("independent-case-predicates", specification),
                )
            }
        }
        fn findings(result: &file_application::Domain) -> Vec<yamaa_core::json::Value> {
            result
                .issues()
                .iter()
                .map(|issue| {
                    let mut value = yamaa_adapters::report_storage::diagnostic_value(issue);
                    assert!(value["context"].is_object());
                    value["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                    value
                })
                .collect()
        }
        /// Seams the replay cannot express: the original build route, an
        /// absent output table, the save's return value and unchanged primary
        /// bytes. Their authored contracts live beside the original fixtures.
        #[test]
        fn refused_case_predicate_domains_have_no_output_and_fail_saves() {
            use std::collections::BTreeSet;

            use crate::support::diagnostic::{operations, replay};
            use yamaa_core::json::Value;

            fn primary_bytes(path: &std::path::Path, label: &str) -> Option<Vec<u8>> {
                match fs::read(path) {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => panic!("{label}: reading {}: {error}", path.display()),
                }
            }

            let contracts = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/case_predicate_refusals.json"
            ))
            .unwrap();
            replay::closed(&contracts, &["about", "cases"]);
            let contracts = replay::field(&contracts, "cases").as_object().unwrap();
            assert!(!contracts.is_empty(), "the family has authored refusals");

            let cases = replay::cases()
                .into_iter()
                .filter(|case| {
                    case.label.starts_with("declarative_case_predicates.json::")
                        && !case.issues.is_empty()
                        && case
                            .operations
                            .iter()
                            .any(|operation| matches!(operation.as_str(), "build" | "save"))
                })
                .collect::<Vec<_>>();
            let name = |label: &str| {
                label
                    .strip_prefix("declarative_case_predicates.json::")
                    .unwrap()
                    .to_owned()
            };
            assert_eq!(
                cases
                    .iter()
                    .map(|case| name(&case.label))
                    .collect::<BTreeSet<_>>(),
                contracts.keys().cloned().collect::<BTreeSet<_>>(),
                "every authored refusal has exactly one API contract"
            );
            for case in cases {
                let expected = &contracts[&name(&case.label)];
                replay::closed(expected, &["route", "primary", "before"]);
                let route = replay::text(replay::field(expected, "route"));
                assert!(
                    matches!(route, "domain" | "refused" | "either"),
                    "{}: unknown route {route}",
                    case.label
                );
                let fixture = Fixture(operations::study(&case.input));
                let primary = fixture
                    .0
                    .join(replay::text(replay::field(expected, "primary")));
                let before = match replay::field(expected, "before") {
                    Value::Null => None,
                    Value::String(text) => Some(text.as_bytes().to_vec()),
                    other => panic!(
                        "{}: expected initial primary bytes or null, not {other:?}",
                        case.label
                    ),
                };
                assert_eq!(
                    primary_bytes(&primary, &case.label),
                    before,
                    "{}: authored initial primary state",
                    case.label
                );
                let body = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
                let built = fixture.run(&body);
                let actual_route = if built.is_ok() { "domain" } else { "refused" };
                assert!(
                    route == "either" || route == actual_route,
                    "{}: expected {route}, got {actual_route}",
                    case.label
                );
                let after_build = primary_bytes(&primary, &case.label);
                match built {
                    Ok(mut result) => {
                        let has_output = result.output().is_some();
                        let save_failed =
                            matches!(result.save(), Err(file_application::FailedBuild));
                        let after_save = primary_bytes(&primary, &case.label);
                        assert_eq!(
                            (has_output, save_failed, after_build, after_save),
                            (false, true, before.clone(), before),
                            "{}: a refused domain exposes no output, fails its save and preserves its primary",
                            case.label
                        );
                    }
                    Err(_) => assert_eq!(
                        after_build, before,
                        "{}: preparation preserves its primary",
                        case.label
                    ),
                }
            }
        }
        /// A seam the replay cannot reach: these benchmarks' own files, run
        /// by `support::benchmarks::reproduce`. They pin case predicates and
        /// filtered sources in the bytes they save.
        #[test]
        fn unchanged_original_case_and_filtered_source_truth_passes_in_full() {
            for name in [
                "adam-adae-protocol-review",
                "schema-expressions",
                "schema-predicates",
                "sdtm-ae-seriousness",
                "sdtm-mh-ongoing-conditions",
                "sdtm-tr-tumor-measurements",
                "sdtm-tu-tumor-identification",
                "sdtm-ds-reasons",
            ] {
                crate::support::benchmarks::reproduce(name);
            }
        }

        /// A seam the replay cannot reach: the native reader of
        /// `fixtures/case_predicates.json` and its dispositions, which the
        /// installed Python and R suites read in the same format
        /// (`tools/staging/stage_specification_inputs.py`). Each case is an
        /// authored complete report, its findings or saved bytes, which no
        /// benchmark states.
        #[test]
        fn independently_authored_installed_host_truth_has_the_same_complete_native_observations() {
            let truth: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(include_str!("fixtures/case_predicates.json"))
                    .unwrap();
            let dispositions: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/case_predicate_dispositions.json"),
            )
            .unwrap();
            for name in dispositions["cases"].as_object().unwrap().keys() {
                assert!(
                    truth["cases"].get(name).is_some(),
                    "unknown disposition {name}"
                );
            }
            for (name, case) in truth["cases"].as_object().unwrap() {
                if let Some(disposition) = dispositions["cases"].get(name) {
                    assert_eq!(
                        disposition["classification"],
                        "historical_private_reference_defect"
                    );
                    assert_eq!(case["csv"], disposition["historical_csv"]);
                    assert_eq!(disposition["rule"], "LOOKUP-8");
                    continue;
                }
                let fixture = Fixture::new();
                for (name, contents) in case["inputs"].as_object().unwrap() {
                    fs::write(fixture.0.join(name), contents.as_str().unwrap()).unwrap();
                }
                let mut result = fixture
                    .run(case["specification"].as_str().unwrap())
                    .unwrap();
                let findings = findings(&result);
                assert_eq!(
                    yamaa_adapters::json!(findings),
                    case.get("findings")
                        .cloned()
                        .unwrap_or_else(|| yamaa_adapters::json!([])),
                    "{name}"
                );
                if let Some(csv) = case.get("csv") {
                    assert!(result.save().unwrap(), "{name}");
                    assert_eq!(
                        fs::read(fixture.0.join("result.csv")).unwrap(),
                        csv.as_str().unwrap().as_bytes(),
                        "{name}"
                    );
                } else {
                    assert!(result.output().is_none(), "{name}");
                    assert!(result.save().is_err(), "{name}");
                    assert!(!fixture.0.join("result.csv").exists(), "{name}");
                }
            }
        }
    }
    mod declarative_flags {
        //! Unchanged public originals and independently captured authored witnesses.
        use std::{fs, path::Path};
        use yamaa_application::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("flag");
                Self(path)
            }
            fn original(name: &str) -> Self {
                let fixture = Self::new();
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                fn copy(src: &Path, dst: &Path) {
                    for e in fs::read_dir(src).unwrap() {
                        let e = e.unwrap();
                        let target = dst.join(e.file_name());
                        if e.file_type().unwrap().is_dir() {
                            fs::create_dir(&target).unwrap();
                            copy(&e.path(), &target);
                        } else {
                            fs::copy(e.path(), target).unwrap();
                        }
                    }
                }
                copy(&source, &fixture.0);
                fixture
            }
            fn run(
                &self,
                name: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
                let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    crate::support::study::identity(name, &specification),
                )
            }
        }
        fn issues(issues: &[yamaa_core::diagnostic::Diagnostic]) -> Vec<yamaa_core::json::Value> {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        fn reference() -> yamaa_core::json::Value {
            yamaa_adapters::json_wire::decode(include_str!("fixtures/flag_reference.json")).unwrap()
        }
        /// A seam the replay cannot reach: each benchmark's own files, and the
        /// native reader of `flag_reference.json`'s originals, which the
        /// installed Python and R suites also read.
        /// The benchmarks run as they are written, in either language: their
        /// handler counts and findings are the same in both (#3422), but for
        /// an intermediate's absence, which a 1.0 file counts under `absent`
        /// (D202). The captured truth holds for each benchmark's own
        /// specification, and the capture's specification text is not
        /// compared. Each benchmark pins the bytes it saves; what no
        /// benchmark pins is the handler ledger each run reports, as
        /// `flag_reference.json` captured it.
        #[test]
        fn six_unchanged_original_positive_documents_match_csv_and_full_handlers() {
            let truth = reference();
            for name in [
                "schema-str-contains",
                "adam-adae-occurrence-flags",
                "adam-adex-dose-reduction",
                "adam-adlb-closest-visit",
                "adam-adsl-completion-flag",
                "sdtm-dm-death",
            ] {
                crate::support::benchmarks::reproduce(name);
                let fixture = Fixture::original(name);
                let result = fixture
                    .run(name)
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert_eq!(
                    result.observations()["handler_counts"],
                    truth["originals"][name]["handlers"],
                    "{name}"
                );
            }
        }
        /// A seam the replay cannot reach: each benchmark's own files, and the
        /// native reader of `flag_reference.json`'s originals, which the
        /// installed Python and R suites also read.
        /// Each benchmark's `error.yaml` states its first issue, which public
        /// conformance compares in part; what no benchmark pins is each run's
        /// complete findings as `flag_reference.json` captured them: the one
        /// issue, with every context key (`negative-flag-condition`'s `expr`,
        /// `negative-review-date-text`'s `actual`).
        #[test]
        fn unchanged_negative_originals_retain_complete_semantic_findings() {
            let truth = reference();
            for name in [
                "negative-flag-condition",
                "negative-flag-missing-value",
                "negative-flag-payload",
                "negative-review-arithmetic",
                "negative-review-date-text",
                "negative-review-unknown-date",
            ] {
                let fixture = Fixture::original(name);
                let mut expected = truth["originals"][name]["findings"].clone();
                let mut actual = match fixture.run(name) {
                    Err(issues_) => issues(&issues_),
                    Ok(mut result) => {
                        assert!(result.output().is_none());
                        assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                        issues(&result.issues())
                    }
                };
                if name == "negative-flag-payload" {
                    // The capture names the files it loaded; a 1.x
                    // specification's schema finding names the files this
                    // run loaded, and a 0.5 one names none.
                    for d in expected.as_array_mut().unwrap() {
                        let c = d["context"].as_object_mut().unwrap();
                        assert!(c.remove("source").is_some());
                        assert!(c.remove("entry").is_some());
                    }
                    for d in &mut actual {
                        let c = d["context"].as_object_mut().unwrap();
                        c.remove("source");
                        c.remove("entry");
                    }
                }
                assert_eq!(actual, expected.as_array().unwrap().as_slice(), "{name}");
            }
        }
        /// A seam the replay cannot reach: the native reader of
        /// `flag_reference.json`'s witnesses, which the installed Python and R
        /// suites also read.
        /// The witnesses are authored cases; no benchmark states their
        /// complete results: findings, handler ledgers and saved bytes.
        #[test]
        fn captured_scope_presence_precedence_and_lazy_named_witnesses_match_full_result() {
            let truth = reference();
            for (name, case) in truth["witnesses"].as_object().unwrap() {
                // Generic case-reference requirements and recursive map report projection
                // are existing separate debt, retained in the complete reference fixture.
                if ["nested_case_mixed_declarations", "implicit_raw_ambiguous"]
                    .contains(&name.as_str())
                {
                    continue;
                }
                let fixture = Fixture::new();
                fs::write(
                    fixture.0.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (path, bytes) in case["inputs"].as_object().into_iter().flatten() {
                    fs::write(fixture.0.join(path), bytes.as_str().unwrap()).unwrap();
                }
                let mut expected = case["findings"].clone();
                if case["result_type"] == "SpecificationError" {
                    // A schema finding names the files its run loaded: the
                    // capture names its own, and this run loads spec.yaml,
                    // named by its identity.
                    let loaded = yamaa_adapters::json!(fixture.0.identity("spec.yaml"));
                    for d in expected.as_array_mut().unwrap() {
                        let c = d["context"].as_object_mut().unwrap();
                        assert!(c.insert("source".into(), loaded.clone()).is_some());
                        assert!(c.insert("entry".into(), loaded.clone()).is_some());
                    }
                }
                match fixture.run(name) {
                    Err(actual) => assert_eq!(
                        issues(&actual),
                        expected.as_array().unwrap().as_slice(),
                        "{name}"
                    ),
                    Ok(mut result) => {
                        assert_eq!(
                            issues(&result.issues()),
                            expected.as_array().unwrap().as_slice(),
                            "{name}"
                        );
                        assert_eq!(
                            result.observations()["handler_counts"],
                            case["handlers"],
                            "{name}"
                        );
                        if case["result_type"] == "ExecutionSuccess" {
                            assert!(result.save().unwrap());
                            assert_eq!(
                                fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                                case["csv"].as_str().unwrap(),
                                "{name}"
                            );
                        } else {
                            assert!(result.output().is_none());
                            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                            assert!(!fixture.0.join("out.csv").exists());
                        }
                    }
                }
            }
        }
    }
    mod declarative_intermediate_keys {
        //! Unchanged public YAML and independent plain-reference outputs/findings.
        use std::fs;
        use yamaa_application::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("expression-key");
                Self(path)
            }
            fn run(&self, name: &str, case: &yamaa_core::json::Value) -> file_application::Domain {
                fs::write(
                    self.0.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (path, input) in case["inputs"].as_object().unwrap() {
                    fs::write(self.0.join(path), input.as_str().unwrap()).unwrap();
                }
                let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                let identity = crate::support::study::identity(name, &specification);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    identity,
                )
                .unwrap()
            }
        }
        fn issue_json(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        /// A seam the replay cannot reach: the native reader of
        /// `fixtures/intermediate_key_reference.json`, which the installed
        /// Python and R suites also read. Each case is an authored complete
        /// report, its findings or saved bytes, which no benchmark states.
        #[test]
        fn independent_original_language_captures_match_all_output_bytes_and_complete_findings() {
            let cases: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/intermediate_key_reference.json"
            ))
            .unwrap();
            for (name, case) in cases.as_object().unwrap() {
                let fixture = Fixture::new();
                let mut result = fixture.run(name, case);
                if let Some(csv) = case.get("csv") {
                    assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read(fixture.0.join("out.csv")).unwrap(),
                        csv.as_str().unwrap().as_bytes(),
                        "{name}"
                    );
                } else {
                    assert!(result.output().is_none(), "{name}");
                    assert_eq!(
                        result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                        case["findings"].as_array().unwrap().as_slice(),
                        "{name}"
                    );
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("out.csv").exists());
                }
            }
        }

        /// A seam the replay cannot reach: the benchmark
        /// `adam-adlb-supplb-padded-key`'s own files, run by
        /// `support::benchmarks::reproduce`. It selects through a padded
        /// expression key without a filter, checks clean and saves the bytes
        /// it states. The twin with an integer filter beside an expression
        /// key is `declarative_case_predicates.json`'s
        /// `a_valid_expression_key_filter_selects_and_saves`.
        #[test]
        fn a_padded_expression_key_reproduces_its_benchmark() {
            crate::support::benchmarks::reproduce("adam-adlb-supplb-padded-key");
        }
    }
    mod declarative_intermediate_ranges {
        //! Complete independent reports for inclusive ranges and visible donor columns.
        use std::{fs, path::PathBuf};
        use yamaa_application::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("intermediate-ranges");
                Self(path)
            }
            fn run(&self, body: &str) -> file_application::Domain {
                let path = self.0.join("spec.yaml");
                fs::write(&path, body).unwrap();
                let path = path.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    crate::support::study::identity("independent-intermediate-range", path),
                )
                .unwrap()
            }
        }
        /// A seam the replay cannot reach: the benchmark's own files, and the
        /// finding its `expected/error.yaml` states, read with
        /// `support::benchmarks::error`.
        /// The benchmark's own specification and inputs, compared with the
        /// finding its `error.yaml` states; what no benchmark pins is that the
        /// finding is the run's only issue, with an empty handler ledger.
        #[test]
        fn unchanged_bad_range_document_preserves_its_single_complete_historical_finding() {
            let f = Fixture::new();
            let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/negative-lookup-bad-range");
            fs::create_dir(f.0.join("input")).unwrap();
            for name in ["vs.csv", "epoch.csv"] {
                fs::copy(
                    original.join("input").join(name),
                    f.0.join("input").join(name),
                )
                .unwrap();
            }
            let mut result = f.run(&fs::read_to_string(original.join("spec.yaml")).unwrap());
            let findings = result
                .issues()
                .iter()
                .map(|issue| {
                    let mut value = yamaa_adapters::report_storage::diagnostic_value(issue);
                    value["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                    value
                })
                .collect::<Vec<_>>();
            // The one complete finding is what the benchmark's
            // expected/error.yaml states, in the language it is written in.
            assert_eq!(
                findings,
                [crate::support::benchmarks::error(
                    "negative-lookup-bad-range"
                )]
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!f.0.join("vs.csv").exists());
        }
    }
    mod declarative_output_ordering {
        //! Authored public-file observations and immutable original benchmark artifacts.
        use crate::support::issues::assert_issues;
        use std::{fs, path::Path};
        use yamaa_application::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                let root = crate::support::Scratch::new("output-order");
                fn copy(from: &Path, to: &Path) {
                    for entry in fs::read_dir(from).unwrap() {
                        let entry = entry.unwrap();
                        let target = to.join(entry.file_name());
                        if entry.file_type().unwrap().is_dir() {
                            fs::create_dir(&target).unwrap();
                            copy(&entry.path(), &target)
                        } else {
                            fs::copy(entry.path(), target).unwrap();
                        }
                    }
                }
                copy(
                    &Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../../benchmarks")
                        .join(name),
                    &root,
                );
                Self(root)
            }
            fn run(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
                let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    crate::support::study::identity("test", &specification),
                )
            }
        }
        // Its one caller reads a Parquet input.
        #[cfg(feature = "parquet")]
        fn authored(spec: &str, input: &[u8]) -> Fixture {
            let f = Fixture::new("schema-ordering");
            fs::write(f.0.join("spec.yaml"), spec).unwrap();
            fs::write(f.0.join("source.csv"), input).unwrap();
            f
        }
        /// A seam the replay cannot reach: the benchmark's own files, run by
        /// `support::benchmarks::reproduce`. The benchmark states the bytes
        /// its ordering saves.
        #[test]
        fn original_ordering_artifact_matches_immutable_csv_truth() {
            crate::support::benchmarks::reproduce("schema-ordering");
        }
        /// A seam the replay cannot reach: these benchmarks' own files, run by
        /// `support::benchmarks::reproduce`.
        /// `adam-adae-review-order` orders its rows by `SEVORD`, a working
        /// column outside the output, `schema-ordering` keeps tied rows in
        /// construction order, and `adam-adsl-site-parse`, which declares no
        /// order, keeps its subjects in the order its input holds them.
        #[test]
        fn internal_working_columns_order_the_projection_and_omission_keeps_construction_order() {
            for benchmark in [
                "adam-adae-review-order",
                "schema-ordering",
                "adam-adsl-site-parse",
            ] {
                crate::support::benchmarks::reproduce(benchmark);
            }
        }
        /// A seam the replay cannot reach: these benchmarks' own files, and
        /// the issue each one's `expected/error.yaml` states, read with
        /// `support::benchmarks::error`.
        /// The benchmarks' own specifications, compared with the issue each
        /// one's `error.yaml` states; what no benchmark pins is that it is the
        /// run's only issue.
        #[test]
        fn ordering_and_unknown_profile_negatives_keep_registered_first_findings() {
            for name in [
                "negative-order-repeated",
                "negative-order-unknown-column",
                "negative-source-unknown-format",
            ] {
                let fixture = Fixture::new(name);
                let issues = match fixture.run() {
                    Ok(result) => {
                        assert!(result.output().is_none());
                        result.issues().to_vec()
                    }
                    Err(issues) => issues,
                };
                assert_eq!(
                    issues
                        .iter()
                        .map(crate::support::study::issue_row)
                        .collect::<Vec<_>>(),
                    [crate::support::benchmarks::error(name)],
                    "{name}"
                );
            }
        }

        /// A seam the replay cannot reach: the benchmark's own files, and the
        /// issue its `expected/error.yaml` states, read with
        /// `support::benchmarks::error`. What no benchmark pins is that it is
        /// the run's only issue. An ordinal on the fixed ODM schema (INPUT-4)
        /// is `declarative_case_predicates.json`'s
        /// `an_ordinal_on_the_fixed_odm_schema_is_refused_before_capture`.
        #[test]
        fn ordinal_collision_and_fixed_odm_schema_have_complete_public_diagnostics() {
            let fixture = Fixture::new("negative-source-ordinal-collision");
            let result = fixture.run().unwrap();
            assert!(result.output().is_none());
            let issues = result.issues();
            assert_issues(
                &issues,
                &[crate::support::benchmarks::error(
                    "negative-source-ordinal-collision",
                )],
                "negative-source-ordinal-collision",
            );
        }

        /// A seam the replay cannot reach: a Parquet input the test renders
        /// with `parquet_artifact::render`, bytes a case's text files cannot
        /// hold. No benchmark declares an ordinal on a Parquet input.
        #[test]
        #[cfg(feature = "parquet")]
        fn parquet_records_expose_identical_ordinal_policy_without_changing_stored_fields() {
            use yamaa_adapters::{parquet_artifact, typed_csv};
            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: source.parquet, ordinal: REC}}
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#,
                b"unused\n",
            );
            for (input, expected) in [
                (
                    b"ID\nb\na\n".as_slice(),
                    b"ID,POSITION\nb,1\na,2\n".as_slice(),
                ),
                (b"ID\n", b"ID,POSITION\n"),
            ] {
                // Known physical input; the expected generated positions above are authored independently.
                let table = typed_csv::parse(input, &[]).unwrap();
                let bytes = parquet_artifact::render(&table, &[0]).unwrap();
                fs::write(fixture.0.join("source.parquet"), bytes).unwrap();
                let mut result = fixture.run().unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
            }
        }
    }
    mod declarative_row_intermediates {
        //! Unchanged public YAML and independent plain-reference outputs/findings.
        use std::fs;
        use yamaa_application::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("row-context");
                Self(path)
            }
        }
        fn issue_json(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        /// A seam the replay cannot reach: the native reader of
        /// `fixtures/row_intermediate_reference.json`, which the installed
        /// Python and R suites also read.
        /// The two originals copy `adam-adlb-supplb-padded-key` and
        /// `adam-adlb-end-of-treatment`, whose saved bytes their
        /// `expected/adlb.csv` pins; what no benchmark pins is each run's
        /// complete handler ledger, and the authored cases' complete reports.
        #[test]
        fn complete_independent_record_contexts_and_unchanged_original() {
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/row_intermediate_reference.json"),
            )
            .unwrap();
            for (name, case) in reference["cases"]
                .as_object()
                .unwrap()
                .iter()
                .chain(reference["originals"].as_object().unwrap())
            {
                let fixture = Fixture::new();
                fs::write(
                    fixture.0.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (path, input) in case["inputs"].as_object().unwrap() {
                    let path = if name.starts_with("adam-") {
                        fixture.0.join("input").join(path)
                    } else {
                        fixture.0.join(path)
                    };
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, input.as_str().unwrap()).unwrap();
                }
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let identity = crate::support::study::identity(name, &specification);
                let built = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    identity,
                );
                let expected = case["findings"].as_array().unwrap().clone();
                let mut result = match built {
                    Ok(result) => result,
                    Err(issues) => {
                        assert_eq!(
                            issues.iter().map(issue_json).collect::<Vec<_>>(),
                            expected,
                            "{name}"
                        );
                        continue;
                    }
                };
                assert_eq!(
                    result.observations()["handler_counts"],
                    case.get("handler_counts")
                        .cloned()
                        .unwrap_or_else(|| yamaa_adapters::json!([])),
                    "{name} complete handler ledger"
                );
                if let Some(csv) = case.get("csv") {
                    // The committed artifact remains the oracle; an older rendered
                    // reference capture uses redundant .0 on whole binary64 values.
                    let csv = case.get("expected_csv").unwrap_or(csv);
                    assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                    assert!(result.save().unwrap());
                    let output = if matches!(
                        name.as_str(),
                        "adam-adlb-supplb-padded-key" | "adam-adlb-end-of-treatment"
                    ) {
                        "adlb.csv"
                    } else {
                        "result.csv"
                    };
                    assert_eq!(
                        fs::read(fixture.0.join(output)).unwrap(),
                        csv.as_str().unwrap().as_bytes(),
                        "{name}"
                    );
                } else {
                    assert!(result.output().is_none(), "{name}");
                    assert_eq!(
                        result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                        case["findings"].as_array().unwrap().as_slice(),
                        "{name}"
                    );
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                }
            }
        }
    }
}
