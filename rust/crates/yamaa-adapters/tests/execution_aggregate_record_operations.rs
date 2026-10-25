#![cfg(feature = "yaml")]
//! Execution contracts: aggregate record operations, baseline window types,
//! case item order, check output validation and compiled model. The
//! aggregate record operations that write files, run one specification and
//! compare what it reports are cases of
//! `tests/fixtures/diagnostics/aggregate_record_operations.json` (#3798);
//! the tests left of that family read benchmarks' own files and the API
//! results the replay does not state.

mod execution {
    mod aggregate_record_operations {
        //! Per-record aggregates: benchmarks that pin their saved bytes, and
        //! the build route and save result of each case of
        //! `aggregate_record_operations.json`.
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn run(
                &self,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
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
        /// A seam the replay cannot reach: these benchmarks' own files, run
        /// by `support::benchmarks::reproduce`. They pin aggregates in the
        /// bytes they save.
        #[test]
        fn original_public_aggregate_fixtures_preserve_frozen_csv_bytes() {
            for name in [
                "schema-aggregation",
                "schema-derive-date-epoch",
                "schema-derive-text-score",
                "schema-aggregate-derive-intermediate",
                "adam-adsl-new-therapy",
                "adam-adtr-nadir",
                "adam-adtte-pfs",
                "sdtm-rs-timepoint-response",
            ] {
                crate::support::benchmarks::reproduce(name);
            }
        }
        /// A seam the replay cannot reach: these benchmarks' own files, run
        /// by `support::benchmarks::reproduce`. Without row templates the
        /// base input builds every row, so a qualified `between.value` read
        /// from the base input or from a column builds (#2904). The reads
        /// of rows that row templates build are the `a_between_value_...`
        /// cases of `aggregate_record_operations.json`.
        #[test]
        fn a_qualified_between_value_of_the_base_input_reproduces_its_benchmarks() {
            for name in [
                "schema-aggregate-range-driver",
                "adam-adtte-pro-deterioration",
            ] {
                crate::support::benchmarks::reproduce(name);
            }
        }
        /// A seam the replay cannot reach: `adam-adsl-treatment`'s own files,
        /// run by `support::benchmarks::reproduce`. The bindings of the first
        /// and last exposures impute with `time: first` and `time: last`,
        /// enum arguments rather than reads.
        #[test]
        fn original_aggregate_derive_enum_arguments_are_not_variable_references() {
            crate::support::benchmarks::reproduce("adam-adsl-treatment");
        }
        /// Seams the replay cannot express: the route a build takes and what
        /// a save returns. Each case of `aggregate_record_operations.json`
        /// that builds and reports no issue builds a domain, and one that
        /// saves writes none of its files before the save, which returns
        /// `Ok(true)`. A refused case may be refused before a domain exists,
        /// as the moved tests accepted, but a domain it builds exposes no
        /// output and fails its save.
        #[test]
        fn aggregate_record_cases_keep_their_build_routes_and_save_results() {
            use crate::support::diagnostic::{operations, replay};

            let cases = replay::cases()
                .into_iter()
                .filter(|case| {
                    case.label.starts_with("aggregate_record_operations.json::")
                        && case.operations.iter().any(|operation| operation != "check")
                })
                .collect::<Vec<_>>();
            assert!(!cases.is_empty(), "the family builds its cases");
            for case in cases {
                let fixture = Fixture(operations::study(&case.input));
                match (case.issues.is_empty(), fixture.run()) {
                    (true, Ok(mut domain)) => {
                        if case.operations == ["save"] {
                            let stated = case.stated.saved.as_ref().expect("a stated save");
                            for name in stated.keys() {
                                assert!(
                                    !fixture.0.join(name).exists(),
                                    "{}: the build publishes {name} only when it saves",
                                    case.label
                                );
                            }
                            let saved = domain.save();
                            assert!(
                                matches!(saved, Ok(true)),
                                "{}: an accepted save returns Ok(true), not {saved:?}",
                                case.label
                            );
                        }
                    }
                    (true, Err(issues)) => {
                        panic!("{}: an accepted case builds: {issues:?}", case.label)
                    }
                    (false, Ok(mut domain)) => {
                        let has_output = domain.output().is_some();
                        let save_failed =
                            matches!(domain.save(), Err(file_application::FailedBuild));
                        assert_eq!(
                            (has_output, save_failed),
                            (false, true),
                            "{}: a refused domain has no output and fails its save",
                            case.label
                        );
                    }
                    (false, Err(_)) => {}
                }
            }
        }
    }
    mod baseline_window_types {
        //! Baseline temporal operands retain their typed condition and authored input site.
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
        // The cases are host-fixture truth, which the Python and R suites
        // read too; no benchmark pins these typed baseline operands.
        #[test]
        fn baseline_types_preserve_conditions_and_reference_priority() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["baseline_window_types"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(crate::support::Scratch::new("baseline-window-types"));
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
                let identity = || crate::support::study::identity("baseline_window_types", path);
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
    mod case_item_order {
        //! EXPR-7 case item order is one validation finding at every expression site,
        //! with the same issue from check and domain in both shared host routes. Paths
        //! follow the schema validator: keyed list members are named by `name` or `id`.
        use std::fs;
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::{domain_entry::Request, json::Value};

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(body: &str) -> Self {
                let root = crate::support::Scratch::new("case-item-order");
                fs::write(
                    root.join("spec.yaml"),
                    format!(
                        "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nbase: DM\nkeys: [USUBJID]\ninput:\n  DM: {{path: dm.csv, types: {{AGE: int}}}}\n  EX: {{path: ex.csv, types: {{DOSE: float}}}}\noutput: {{path: out.csv, columns: [USUBJID, FL]}}\n{body}"
                    ),
                )
                .unwrap();
                fs::write(root.join("dm.csv"), "USUBJID,AGE\nA,34\nB,70\n").unwrap();
                fs::write(root.join("ex.csv"), "USUBJID,DOSE\nA,5\nB,10\n").unwrap();
                Self(root)
            }
            fn specification(&self) -> String {
                self.0.join("spec.yaml").to_str().unwrap().to_owned()
            }
        }

        fn identity<'a>(runtime: &'a str, specification: &'a str) -> Identity<'a> {
            Identity {
                runtime,
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "case-item-order",
                specification,
                base_directory: ".",
            }
        }

        fn rows(issues: &[Diagnostic]) -> Value {
            Value::Array(
                issues
                    .iter()
                    .map(crate::support::study::issue_row)
                    .collect(),
            )
        }

        fn issues(fixture: &Fixture) -> Vec<(String, Value)> {
            let specification = fixture.specification();
            let mut result = Vec::new();
            for runtime in ["python", "r"] {
                let request = || Request {
                    specification: &specification,
                    environment: None,
                };
                let checked = file_application::check(
                    crate::support::host(runtime),
                    request(),
                    identity(runtime, &specification),
                );
                let built = match crate::support::study::domain(
                    crate::support::host(runtime),
                    request(),
                    identity(runtime, &specification),
                ) {
                    Ok(domain) => domain.issues(),
                    Err(issues) => issues,
                };
                result.push((format!("{runtime}/check"), rows(&checked)));
                result.push((format!("{runtime}/domain"), rows(&built)));
            }
            result
        }

        /// The schema finding of a 1.x specification names the file the run
        /// loaded, as `source` and `entry` (`support::benchmarks::without_layer`),
        /// by its identity rather than the path the run was given.
        fn finding(fixture: &Fixture, path: &str, expected: &str) -> Value {
            let loaded = fixture.0.identity("spec.yaml");
            yamaa_adapters::json!({
                "phase": "validation", "condition": "invalid_field_type", "rule": "EXPR-7",
                "spec_paths": [path],
                "context": {"expected": expected, "source": loaded, "entry": loaded}
            })
        }

        const NO_BRANCH: &str = "at least one when/then item";
        const NOT_LAST: &str = "otherwise only as the last item";
        const ONLY: &str = "{case: [{otherwise: \"'N'\"}]}";

        fn column(derivation: &str) -> String {
            format!("columns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: FL, type: str, derivation: {derivation}}}\n")
        }

        #[test]
        fn case_item_order_reports_req_0339_at_each_expression_site() {
            // `negative-case-otherwise-only` states the first site's first
            // issue, but its `error.yaml` leaves out the layer the finding
            // names and that the issue is the only one. No benchmark pins the
            // other sites.
            let sites = [
                (column(ONLY), "columns.FL.derivation.case", NO_BRANCH),
                (column("{case: []}"), "columns.FL.derivation.case", NO_BRANCH),
                (
                    column("{case: [{otherwise: \"'N'\"}, {when: 'DM.AGE >= 65', then: \"'Y'\"}]}"),
                    "columns.FL.derivation.case[0]",
                    NOT_LAST,
                ),
                (
                    column("{case: [{when: 'DM.AGE >= 65', then: \"'Y'\"}, {otherwise: \"'N'\"}, {otherwise: \"'U'\"}]}"),
                    "columns.FL.derivation.case[1]",
                    NOT_LAST,
                ),
                (
                    column(&format!("{{case: [{{when: 'DM.AGE >= 65', then: {ONLY}}}, {{otherwise: \"'N'\"}}]}}")),
                    "columns.FL.derivation.case[0].then.case",
                    NO_BRANCH,
                ),
                (
                    format!("columns:\n  - {{name: USUBJID, type: str}}\n  - {{name: FL, type: str}}\nrows:\n  - {{id: subject, dataset: DM, derivations: {{USUBJID: DM.USUBJID, FL: {ONLY}}}}}\n"),
                    "rows.subject.derivations.FL.case",
                    NO_BRANCH,
                ),
                (
                    format!("intermediates:\n  - {{id: DOSED, dataset: EX, key: [USUBJID], derivations: {{GROUP: {ONLY}}}}}\n{}", column("DOSED.GROUP")),
                    "intermediates.DOSED.derivations.GROUP.case",
                    NO_BRANCH,
                ),
                (
                    column(&format!("{{aggregate: {{key: [USUBJID], filter: 'EX.DOSE > 0', derivations: {{LEVEL: {{type: str, derivation: {ONLY}}}}}, expr: 'MAX(LEVEL)'}}}}")),
                    "columns.FL.derivation.aggregate.derivations.LEVEL.derivation.case",
                    NO_BRANCH,
                ),
            ];
            for (body, path, expected) in sites {
                let fixture = Fixture::new(&body);
                for (route, actual) in issues(&fixture) {
                    assert_eq!(
                        actual,
                        yamaa_adapters::json!([finding(&fixture, path, expected)]),
                        "{route}: {body}"
                    );
                }
                assert!(!fixture.0.join("out.csv").exists());
            }
        }

        // `negative-case-otherwise-only` states a case of one `otherwise` item;
        // no benchmark pins a case of two, each misplaced one reported.
        #[test]
        fn case_with_only_otherwise_items_reports_the_case_and_each_misplaced_item() {
            let fixture = Fixture::new(&column(
                "{case: [{otherwise: \"'N'\"}, {otherwise: \"'U'\"}]}",
            ));
            for (route, actual) in issues(&fixture) {
                assert_eq!(
                    actual,
                    yamaa_adapters::json!([
                        finding(&fixture, "columns.FL.derivation.case", NO_BRANCH),
                        finding(&fixture, "columns.FL.derivation.case[0]", NOT_LAST),
                    ]),
                    "{route}"
                );
            }
        }

        // No benchmark runs `check` in both hosts: a positive benchmark with an
        // ordered case, such as `adam-adsl-age-group`, pins its build alone.
        #[test]
        fn ordered_case_items_still_build() {
            let fixture = Fixture::new(&column(
                "{case: [{when: 'DM.AGE >= 65', then: \"'Y'\"}, {otherwise: \"'N'\"}]}",
            ));
            for (route, actual) in issues(&fixture) {
                assert_eq!(actual, yamaa_adapters::json!([]), "{route}");
            }
        }
    }
    mod check_output_validation {
        //! `check` reports the validation findings that `domain` reaches without
        //! study data: output declarations (#2295), formula qualification and column
        //! order. The same first issue, in both host routes. Committed negative
        //! benchmarks and `expected/error.yaml` remain the oracle.
        use crate::support::benchmarks;
        use std::{fs, path::Path};
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                Self(crate::support::Scratch::new(&format!(
                    "check-output-{name}"
                )))
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

        fn identity<'a>(
            runtime: &'a str,
            example: &'a str,
            specification: &'a str,
        ) -> Identity<'a> {
            Identity {
                runtime,
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example,
                specification,
                base_directory: ".",
            }
        }

        /// Run `check` and `domain` over one staged specification and return the
        /// first issue each route reports.
        fn first_issues(
            specification: &str,
            example: &str,
            runtime: &str,
        ) -> (yamaa_core::json::Value, yamaa_core::json::Value) {
            let request = || Request {
                specification,
                environment: None,
            };
            let checked = file_application::check(
                crate::support::host(runtime),
                request(),
                identity(runtime, example, specification),
            );
            assert!(!checked.is_empty(), "{example}: check reported no issues");
            let built = crate::support::study::domain(
                crate::support::host(runtime),
                request(),
                identity(runtime, example, specification),
            );
            let built = match built {
                Ok(result) => {
                    let issues = result.issues().to_vec();
                    assert!(!issues.is_empty(), "{example}: domain reported no issues");
                    issues
                }
                Err(failure) => {
                    assert!(!failure.is_empty(), "{example}: domain reported no issues");
                    failure
                }
            };
            (issue(&checked[0]), issue(&built[0]))
        }

        /// Every committed negative benchmark whose first issue comes from `output.*`
        /// is an output-declaration validation case. The denominator is pinned so a
        /// benchmark that silently stops being selected fails this test.
        fn output_validation_benchmarks() -> Vec<String> {
            let root = benchmarks::path("");
            let mut selected = Vec::new();
            for entry in fs::read_dir(&root).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("negative-") {
                    continue;
                }
                let oracle = entry.path().join("expected/error.yaml");
                if !oracle.exists() {
                    continue;
                }
                let expected = benchmarks::yaml(&oracle);
                let from_output = expected["spec_paths"]
                    .as_array()
                    .and_then(|paths| paths.first())
                    .and_then(|path| path.as_str())
                    .is_some_and(|path| path.starts_with("output."));
                if from_output {
                    selected.push(name);
                }
            }
            selected.sort();
            selected
        }

        #[test]
        fn check_matches_domain_on_output_validation_benchmarks() {
            let selected = output_validation_benchmarks();
            assert_eq!(
                selected,
                [
                    "negative-order-repeated",
                    "negative-order-unknown-column",
                    "negative-output-transport",
                    "negative-report-same-file",
                ],
                "output-validation benchmark denominator changed"
            );
            let root = benchmarks::path("");
            for name in &selected {
                let original = root.join(name);
                let fixture = Fixture::new(name);
                copy(&original.join("input"), &fixture.0.join("input"));
                fs::copy(original.join("spec.yaml"), fixture.0.join("spec.yaml")).unwrap();
                let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let oracle = benchmarks::yaml(&original.join("expected/error.yaml"));
                for runtime in ["python", "r"] {
                    let (checked, built) = first_issues(&spec, name, runtime);
                    assert_eq!(
                        checked, built,
                        "{name} ({runtime}): check/domain first issue"
                    );
                    assert_eq!(
                        checked, oracle,
                        "{name} ({runtime}): first issue vs expected/error.yaml"
                    );
                }
            }
        }

        /// The three rows of #2295's table, with the issue's own minimal study: an
        /// undeclared output column, `decimals` on a parquet output, and a negative
        /// `decimals`. None is covered by a committed negative benchmark.
        #[test]
        fn check_matches_domain_on_the_issue_rows() {
            let rows: [(&str, &str, yamaa_core::json::Value); 3] = [
                (
                    "undeclared",
                    "{path: out.csv, columns: [USUBJID, N, ZZZ]}",
                    yamaa_adapters::json!({"phase":"validation","condition":"undeclared_column","rule":"OUTPUT-1","spec_paths":["output.columns[2]"],"context":{"column":"ZZZ"}}),
                ),
                (
                    "decimals-parquet",
                    "{path: out.parquet, columns: [USUBJID, N], decimals: 2}",
                    yamaa_adapters::json!({"phase":"validation","condition":"decimals_not_applicable","rule":"CSV-6","spec_paths":["output.decimals"],"context":{"path":"out.parquet","profile":"parquet"}}),
                ),
                (
                    "decimals-negative",
                    "{path: out.csv, columns: [USUBJID, N], decimals: -1}",
                    yamaa_adapters::json!({"phase":"validation","condition":"invalid_field_type","rule":"CSV-6","spec_paths":["output.decimals"],"context":{"expected":"an integer from 0 through 18","actual":-1}}),
                ),
            ];
            for (name, output, oracle) in rows {
                let fixture = Fixture::new(name);
                fs::create_dir(fixture.0.join("input")).unwrap();
                fs::write(fixture.0.join("input/dm.csv"), "USUBJID,AGE\nS-001,34\n").unwrap();
                let spec = format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input/dm.csv}}\noutput: {output}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: N, type: float, derivation: DM.AGE}}\n"
        );
                fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
                let spec_path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                for runtime in ["python", "r"] {
                    let (checked, built) = first_issues(&spec_path, name, runtime);
                    assert_eq!(
                        checked, built,
                        "{name} ({runtime}): check/domain first issue"
                    );
                    assert_eq!(checked, oracle, "{name} ({runtime}): first issue");
                }
            }
        }

        /// The partial oracle of `expected/error.yaml`: phase, condition and spec
        /// paths exactly, the rule when one is written, and every written
        /// context field.
        fn matches_oracle(
            oracle: &yamaa_core::json::Value,
            issue: &yamaa_core::json::Value,
        ) -> bool {
            fn subset(want: &yamaa_core::json::Value, got: &yamaa_core::json::Value) -> bool {
                match (want, got) {
                    (
                        yamaa_core::json::Value::Object(want),
                        yamaa_core::json::Value::Object(got),
                    ) => want
                        .iter()
                        .all(|(key, value)| got.get(key).is_some_and(|got| subset(value, got))),
                    _ => want == got,
                }
            }
            ["phase", "condition", "spec_paths"]
                .into_iter()
                .all(|field| oracle[field] == issue[field])
                && oracle.get("rule").is_none_or(|rule| *rule == issue["rule"])
                && oracle
                    .get("context")
                    .is_none_or(|context| subset(context, &issue["context"]))
        }

        /// Validation negatives whose first issue `check` does not report, with
        /// what decides it. `domain` reaches each of them once it reads or binds
        /// the inputs. A case joins only when the input itself decides its
        /// finding: the fields a file stores, whether it exists, or a value its
        /// records hold, which no declaration states. A case that `check`
        /// starts to report fails until it leaves this list.
        const NOT_REPORTED_BY_CHECK: &[(&str, &str)] = &[
            (
                "negative-linked-artifact-header",
                "the fields stored in the linked artifact",
            ),
            (
                "negative-lookup-inner-join-key-type",
                "a key value whose type the input's records decide",
            ),
            (
                "negative-odm-schema-ambiguous",
                "the fields stored in the ODM input",
            ),
            (
                "negative-odm-schema-missing",
                "the fields stored in the ODM input",
            ),
            ("negative-path-missing", "whether the input file exists"),
            (
                "negative-path-symlink",
                "whether the input file is a symbolic link",
            ),
            (
                "negative-review-unknown-date",
                "an unknown name in a flag condition",
            ),
            (
                "negative-source-ordinal-collision",
                "the fields stored in the input",
            ),
            (
                "negative-source-undeclared-field",
                "the fields stored in the input",
            ),
            (
                "negative-types-unknown-field",
                "the fields stored in the input",
            ),
        ];

        /// Validation negatives this harness does not run, with the reason.
        const NOT_BUILT_HERE: [(&str, &str); 17] = [
            (
                "negative-allowed-values-beside-codelist",
                "the bound codelist's items come from its host environment, which this harness does not pass",
            ),
            (
                "negative-codelist-extended-alias",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-codelist-extension-closed-list",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-codelist-items-and-dictionary",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-codelist-partial-decode",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-codelist-type-mismatch",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-codelist-unknown",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-codelist-value-twice",
                "the codelists are declared by the project file, which this harness does not pass",
            ),
            (
                "negative-donor-function-unkept",
                "this harness passes no project file, so its project function is unknown",
            ),
            (
                "negative-format-name-without-dollar",
                "the codelist is declared by the project file, which this harness does not pass",
            ),
            (
                "negative-function-contract",
                "domain activates its project functions only in a host",
            ),
            (
                "negative-function-name-twice",
                "the project file lists the function files, which this harness does not pass",
            ),
            (
                "negative-function-not-listed",
                "this harness passes no project file, so its project function is unknown",
            ),
            (
                "negative-function-test-mismatch",
                "domain activates its project functions only in a host",
            ),
            (
                "negative-numeric-format-name-with-dollar",
                "the codelist is declared by the project file, which this harness does not pass",
            ),
            (
                "negative-terminology-standard-unknown",
                "the terminology standard is declared by the project file, which this harness does not pass",
            ),
            (
                "negative-value-level-where-not-in-codelist",
                "the where column's codelist is declared by the project file, which this harness does not pass",
            ),
        ];

        /// Every committed negative benchmark that conformance runs, with the
        /// phase of its first issue. A Define benchmark fails in its study's
        /// composition, which `submission_transport` runs; its domain build
        /// succeeds.
        fn negative_benchmarks() -> Vec<(String, String)> {
            let root = benchmarks::path("");
            let mut selected = Vec::new();
            for entry in fs::read_dir(&root).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                let oracle = entry.path().join("expected/error.yaml");
                if !name.starts_with("negative-") || !oracle.exists() {
                    continue;
                }
                let metadata = benchmarks::yaml(&entry.path().join("benchmark.yaml"));
                if metadata["conformance"]["excluded"].is_null() && metadata["define"].is_null() {
                    let phase = benchmarks::yaml(&oracle)["phase"]
                        .as_str()
                        .unwrap()
                        .to_owned();
                    selected.push((name, phase));
                }
            }
            selected.sort();
            selected
        }

        /// `check` over every committed negative. One that fails after
        /// validation needs the study's data, so `check` reports nothing for
        /// it. Every validation negative has the same first issue in `check`
        /// and `domain`, in both host routes, matching `expected/error.yaml`,
        /// except the listed ones whose finding binding decides from the
        /// inputs. Benchmarks run in place, so parents, links and relative
        /// paths keep their authored meaning.
        #[test]
        fn check_matches_domain_on_negative_benchmarks() {
            let selected = negative_benchmarks();
            for (name, _) in NOT_REPORTED_BY_CHECK.iter().chain(&NOT_BUILT_HERE) {
                assert!(
                    selected
                        .iter()
                        .any(|(s, phase)| s == name && phase == "validation"),
                    "{name}: not selected"
                );
            }
            let root = benchmarks::path("");
            let mut compared = Vec::new();
            for (name, phase) in &selected {
                if NOT_BUILT_HERE.iter().any(|(n, _)| n == name) {
                    continue;
                }
                let case = root.join(name);
                let metadata = benchmarks::yaml(&case.join("benchmark.yaml"));
                let specification = |runtime: &str| {
                    // The primary entry is last in each host's declared list.
                    let filename = if metadata["entries"].is_null() {
                        "spec.yaml"
                    } else {
                        metadata["entries"][runtime]
                            .as_array()
                            .and_then(|entries| entries.last())
                            .expect("benchmark entries must name a primary specification")
                            ["specification"]
                            .as_str()
                            .expect("the primary specification must be a filename")
                    };
                    case.join(filename).to_str().unwrap().to_owned()
                };
                let oracle = benchmarks::yaml(&case.join("expected/error.yaml"));
                if phase != "validation" {
                    // This harness passes no project file. A 1.x
                    // specification that binds a codelist through its
                    // project is then refused for want of it (CT-13, D32),
                    // as a 0.5 one is not; nothing else may be reported.
                    let project = case.join("project.yaml").is_file();
                    for runtime in ["python", "r"] {
                        let spec = specification(runtime);
                        let checked = file_application::check(
                            crate::support::host(runtime),
                            Request {
                                specification: &spec,
                                environment: None,
                            },
                            identity(runtime, name, &spec),
                        );
                        assert!(
                            checked
                                .iter()
                                .all(|issue| project
                                    && issue.condition() == "project_binding_missing"),
                            "{name} ({runtime}): a {phase} negative, check reports {:?}",
                            checked.iter().map(issue).collect::<Vec<_>>()
                        );
                    }
                    continue;
                }
                let missed = NOT_REPORTED_BY_CHECK.iter().find(|(n, _)| n == name);
                for runtime in ["python", "r"] {
                    let spec = specification(runtime);
                    if let Some((_, reason)) = missed {
                        let checked = file_application::check(
                            crate::support::host(runtime),
                            Request {
                                specification: &spec,
                                environment: None,
                            },
                            identity(runtime, name, &spec),
                        );
                        assert!(
                            !checked
                                .first()
                                .is_some_and(|i| matches_oracle(&oracle, &issue(i))),
                            "{name} ({runtime}): check reports {reason}; remove the exclusion"
                        );
                        continue;
                    }
                    let (checked, built) = first_issues(&spec, name, runtime);
                    assert_eq!(
                        checked, built,
                        "{name} ({runtime}): check/domain first issue"
                    );
                    assert!(
                        matches_oracle(&oracle, &checked),
                        "{name} ({runtime}): first issue {checked:?} vs expected/error.yaml {oracle:?}"
                    );
                }
                if missed.is_none() {
                    compared.push(name.as_str());
                }
            }
            // Column order over every kind of derivation, in either row
            // construction, and what binding decides of an aggregate, a
            // mapping or cut, a lookup range and a source filter from the
            // declarations (#2844, owner decision 3); the phase boundaries
            // each row template's phase set decides (#2754); and the group
            // and driver scopes of what the rows read (#3229).
            for name in [
                "negative-adlb-computed-param",
                "negative-aggregate-key-bare-field",
                "negative-aggregate-key-other-template",
                "negative-cut-coded",
                "negative-direct-dose-read",
                "negative-dose-intensity",
                "negative-first-available-self",
                "negative-forward-key-reference",
                "negative-forward-reference",
                "negative-grouped-lookup-ungrouped-key",
                "negative-grouped-range-ungrouped-value",
                "negative-keys-missing",
                "negative-lookup-bad-range",
                "negative-lookup-derivation-scope",
                "negative-lookup-donor-range-driver",
                "negative-lookup-key-column-phase",
                "negative-lookup-key-lookup-qualified",
                "negative-lookup-key-other-template",
                "negative-lookup-match-column-phase",
                "negative-lookup-mismatched",
                "negative-lookup-range-bare-field",
                "negative-row-aggregate-keys",
                "negative-row-filter-lookup-scope",
                "negative-row-phase-flag",
                "negative-row-ungrouped-second-template",
                "negative-row-value-self",
                "negative-row-window-ungrouped",
                "negative-source-missing-field",
                "negative-source-range-other-input",
                "negative-source-trivial-filter",
                "negative-sum-non-numeric",
                "negative-verification-lookup-key",
            ] {
                assert!(compared.contains(&name), "{name}: not compared");
            }
        }

        /// Formula reads bind without a source schema, so when nothing else is wrong
        /// `check` reports exactly what `domain` reports: each qualified name that is
        /// not a record lookup, then column order over the formulas' reads. The
        /// benchmarks state first issues alone (`negative-direct-dose-read`,
        /// `negative-forward-reference`); no benchmark pins the complete list.
        #[test]
        fn check_reports_formula_qualification_and_column_order() {
            let qualified = |expr: &str, identifier: &str| yamaa_adapters::json!({"phase":"validation","condition":"qualified_identifier","rule":"FORMULA-9","spec_paths":["columns.A.derivation"],"context":{"expr":expr,"identifier":identifier}});
            let forward = |dependency: &str| yamaa_adapters::json!({"phase":"validation","condition":"forward_reference","rule":"COLUMN-5","spec_paths":["columns.A.derivation"],"context":{"column":"A","dependency":dependency}});
            let cycle = yamaa_adapters::json!({"phase":"validation","condition":"dependency_cycle","rule":"COLUMN-6","spec_paths":["columns.A.derivation","columns.B.derivation"],"context":{"cycle":["A","B","A"]}});
            let rows: [(&str, &str, &str, Vec<yamaa_core::json::Value>); 4] = [
                (
                    "qualified-then-forward",
                    "A, B",
                    "  - {name: A, type: float, derivation: 'B + DM.AGE + DM.WT'}\n  - {name: B, type: float, derivation: DM.AGE}\n",
                    vec![
                        qualified("B + DM.AGE + DM.WT", "DM.AGE"),
                        qualified("B + DM.AGE + DM.WT", "DM.WT"),
                        forward("B"),
                    ],
                ),
                (
                    "forward-in-written-order",
                    "A, B, C",
                    "  - {name: A, type: float, derivation: 'C - B'}\n  - {name: B, type: float, derivation: DM.AGE}\n  - {name: C, type: float, derivation: DM.WT}\n",
                    vec![forward("C"), forward("B")],
                ),
                (
                    "cycle",
                    "A, B",
                    "  - {name: A, type: float, derivation: 'B + 1'}\n  - {name: B, type: float, derivation: 'A + 1'}\n",
                    vec![cycle],
                ),
                (
                    "earlier-column",
                    "B, A",
                    "  - {name: B, type: float, derivation: DM.AGE}\n  - {name: A, type: float, derivation: 'B * 2'}\n",
                    vec![],
                ),
            ];
            for (name, outputs, columns, oracle) in rows {
                let fixture = Fixture::new(name);
                fs::create_dir(fixture.0.join("input")).unwrap();
                fs::write(
                    fixture.0.join("input/dm.csv"),
                    "USUBJID,AGE,WT\nU1,34,70.5\n",
                )
                .unwrap();
                fs::write(
                    fixture.0.join("spec.yaml"),
                    format!(
                        "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: {{path: input/dm.csv, types: {{AGE: float, WT: float}}}}}}\noutput: {{path: out.csv, columns: [USUBJID, {outputs}]}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n{columns}"
                    ),
                )
                .unwrap();
                let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let request = || Request {
                    specification: &spec,
                    environment: None,
                };
                for runtime in ["python", "r"] {
                    let checked = file_application::check(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, name, &spec),
                    );
                    let built = match crate::support::study::domain(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, name, &spec),
                    ) {
                        Ok(result) => result.issues().to_vec(),
                        Err(failure) => failure,
                    };
                    let checked = checked.iter().map(issue).collect::<Vec<_>>();
                    let built = built.iter().map(issue).collect::<Vec<_>>();
                    assert_eq!(checked, oracle, "{name} ({runtime}): check");
                    assert_eq!(built, oracle, "{name} ({runtime}): domain");
                }
            }
        }

        /// What the tree does not keep: a field written empty or `false`, read
        /// from the document as binding reads it. A driver intermediate refuses
        /// each prohibited field it writes, whatever its value (ROW-2); an
        /// `order_by` with no term still needs a `keep` (LOOKUP-10); an ODM
        /// input refuses a written `types` (NAME-8); and an empty `derivations`
        /// keeps an intermediate from being rename-only (LOOKUP-4), so it builds.
        /// `rows: []` builds no row template, as without `rows`, and an ODM read
        /// under `unconvertible` is named at binding's path. `check` and `domain`
        /// report the same issues in both host routes (#3225).
        /// `negative-lookup-empty-order` writes `order_by: []` beside `keep: last`;
        /// no benchmark pins `order_by: []` without `keep`, `derivations: {}`, an
        /// ODM input's `types: {}` or `rows: []`.
        #[test]
        fn written_empty_declarations_keep_their_outcomes_in_both_routes() {
            let header = "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\n";
            let driver = |field: &str| {
                format!(
                    "{header}keys: [ID]\ninput: {{SRC: input/src.csv}}\noutput: {{path: out.csv, columns: [ID]}}\nintermediates:\n  - {{id: DRV, dataset: SRC, filter: \"SRC.ID <> 'z'\", {field}}}\ncolumns:\n  - {{name: ID, type: str}}\nrows:\n  - {{id: all, dataset: DRV, derivations: {{ID: DRV.ID}}}}\n"
                )
            };
            let lookup = |fields: &str| {
                format!(
                    "{header}keys: [X]\ninput: {{SRC: input/src.csv, RIGHT: {{path: input/right.csv, key: [X]}}}}\nbase: SRC\noutput: {{path: out.csv, columns: [X, W]}}\nintermediates:\n  - {{id: LOOK, dataset: RIGHT, {fields}}}\ncolumns:\n  - {{name: X, type: str, derivation: SRC.X}}\n  - {{name: W, type: str, derivation: LOOK.K}}\n"
                )
            };
            let odm = |input: &str, age: &str| {
                format!(
                    "{header}keys: [STUDYID, USUBJID]\ninput: {{ODM: {input}}}\noutput: {{path: out.csv, columns: [STUDYID, USUBJID, AGE]}}\ncolumns:\n  - {{name: STUDYID, type: str, derivation: ODM.StudyOID}}\n  - {{name: USUBJID, type: str, derivation: ODM.SubjectKey}}\n  - {{name: AGE, type: int, derivation: {age}}}\n"
                )
            };
            let rows = |columns: &str, intermediates: &str| {
                format!(
                    "{header}keys: [ID]\ninput: {{SRC: input/src.csv}}\nbase: SRC\noutput: {{path: out.csv, columns: [ID, A]}}\n{intermediates}columns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: X, type: str, derivation: SRC.X}}\n{columns}rows: []\n"
                )
            };
            let invalid_driver = |field: &str| yamaa_adapters::json!({"phase":"validation","condition":"invalid_intermediate_driver","rule":"ROW-2","spec_paths":["rows.all.dataset"],"context":{"fields":[field],"intermediate":"DRV"}});
            let cases: [(&str, String, Option<yamaa_core::json::Value>); 8] = [
                (
                    "driver-require-match-false",
                    driver("require_match: false"),
                    Some(invalid_driver("require_match")),
                ),
                (
                    "driver-order-by-empty",
                    driver("order_by: []"),
                    Some(invalid_driver("order_by")),
                ),
                ("derivations-empty", lookup("key: [X], derivations: {}"), None),
                (
                    "order-by-empty",
                    lookup("key: [X], order_by: []"),
                    Some(
                        yamaa_adapters::json!({"phase":"validation","condition":"unpaired_fields","rule":"LOOKUP-10","spec_paths":["intermediates.LOOK"],"context":{"declared":["order_by"],"intermediate":"LOOK","missing":["keep"]}}),
                    ),
                ),
                (
                    "odm-types-empty",
                    odm("{path: input/odm.csv, types: {}}", "{odm: ODM.IT.DM.AGE}"),
                    Some(
                        yamaa_adapters::json!({"phase":"validation","condition":"odm_schema_field_type","rule":"NAME-8","spec_paths":["input.ODM.types"],"context":{"dataset":"ODM","declared":"types"}}),
                    ),
                ),
                (
                    "odm-under-unconvertible",
                    odm(
                        "input/odm.csv",
                        "{value: {odm: NOPE.IT.DM.AGE}, unconvertible: null}",
                    ),
                    Some(
                        yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","rule":"NAME-3","spec_paths":["columns.AGE.derivation.odm.item"],"context":{"identifier":"NOPE.IT.DM.AGE"}}),
                    ),
                ),
                (
                    "rows-empty-self",
                    rows(
                        "  - {name: A, type: str, derivation: PREV.ID}\n",
                        "intermediates:\n  - {id: PREV, dataset: SELF, key: [ID]}\n",
                    ),
                    Some(
                        yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","rule":"LOOKUP-7","spec_paths":["intermediates.PREV.dataset"],"context":{"identifier":"SELF","intermediate":"PREV"}}),
                    ),
                ),
                (
                    "rows-empty-rounding",
                    rows(
                        "  - {name: A, type: float, derivation: {round_half_away_from_zero: {source: X, digits: 1}}}\n",
                        "",
                    ),
                    Some(
                        yamaa_adapters::json!({"phase":"validation","condition":"incompatible_input_type","rule":"FORMULA-16","spec_paths":["columns.A.derivation.round_half_away_from_zero.source"],"context":{"actual":"str","expected":"numeric","source":"X"}}),
                    ),
                ),
            ];
            let odm_source = benchmarks::path("negative-odm-schema-type").join("input/odm.csv");
            for (name, specification, oracle) in cases {
                let fixture = Fixture::new(name);
                fs::create_dir(fixture.0.join("input")).unwrap();
                fs::write(
                    fixture.0.join("input/src.csv"),
                    "ID,X,V\na,one,1.0\nb,two,2.0\n",
                )
                .unwrap();
                fs::write(fixture.0.join("input/right.csv"), "X,K\none,k1\ntwo,k2\n").unwrap();
                fs::copy(&odm_source, fixture.0.join("input/odm.csv")).unwrap();
                fs::write(fixture.0.join("spec.yaml"), specification).unwrap();
                let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                for runtime in ["python", "r"] {
                    let Some(oracle) = &oracle else {
                        let request = || Request {
                            specification: &spec,
                            environment: None,
                        };
                        let checked = file_application::check(
                            crate::support::host(runtime),
                            request(),
                            identity(runtime, name, &spec),
                        );
                        assert!(checked.is_empty(), "{name} ({runtime}): check");
                        let mut built = crate::support::study::domain(
                            crate::support::host(runtime),
                            request(),
                            identity(runtime, name, &spec),
                        )
                        .unwrap_or_else(|_| panic!("{name} ({runtime}): domain refused"));
                        assert!(built.issues().is_empty(), "{name} ({runtime}): domain");
                        assert!(built.save().unwrap(), "{name} ({runtime}): save");
                        assert_eq!(
                            fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                            "X,W\none,k1\ntwo,k2\n",
                            "{name} ({runtime}): saved"
                        );
                        fs::remove_file(fixture.0.join("out.csv")).unwrap();
                        continue;
                    };
                    let (checked, built) = first_issues(&spec, name, runtime);
                    assert_eq!(checked, *oracle, "{name} ({runtime}): check");
                    assert_eq!(built, *oracle, "{name} ({runtime}): domain");
                }
            }
        }

        fn precision_specification(decimals: Option<&str>, kind: &str) -> String {
            let precision = decimals.map_or(String::new(), |n| format!(", decimals: {n}"));
            format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input.csv}}\noutput: {{path: out.csv, columns: [USUBJID, N]{precision}}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: N, type: {kind}, derivation: DM.VALUE}}\n"
    )
        }

        // The finding is the crate fixture `csv_precision_limit.json`; no
        // benchmark pins a precision outside the range.
        #[test]
        fn csv_precision_range_is_validated_for_empty_and_non_float_outputs_in_both_hosts() {
            let oracle: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/csv_precision_limit.json"
            ))
            .unwrap();
            let huge = format!("1{}", "0".repeat(400));
            for decimals in [
                "-1",
                "19",
                "400",
                "5000",
                "10000000",
                "9223372036854775807",
                "9223372036854775808",
                "18446744073709551616",
                huge.as_str(),
            ] {
                let mut expected = oracle.clone();
                expected["context"]["actual"] =
                    yamaa_adapters::json_wire::decode(decimals).unwrap();
                for (kind, input) in [
                    ("float", "USUBJID,VALUE\nS-001,0.125\n"),
                    ("float", "USUBJID,VALUE\n"),
                    ("int", "USUBJID,VALUE\nS-001,34\n"),
                ] {
                    let fixture = Fixture::new("precision-limit");
                    fs::write(
                        fixture.0.join("spec.yaml"),
                        precision_specification(Some(decimals), kind),
                    )
                    .unwrap();
                    fs::write(fixture.0.join("input.csv"), input).unwrap();
                    let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                    for runtime in ["python", "r"] {
                        let (checked, built) =
                            first_issues(&specification, "precision-limit", runtime);
                        assert_eq!(checked, expected, "check {runtime}: {decimals}/{kind}");
                        assert_eq!(built, expected, "domain {runtime}: {decimals}/{kind}");
                    }
                    assert!(!fixture.0.join("out.csv").exists());
                    // The static finding does not require study data to exist.
                    fs::remove_file(fixture.0.join("input.csv")).unwrap();
                    for runtime in ["python", "r"] {
                        let issues = file_application::check(
                            crate::support::host(runtime),
                            Request {
                                specification: &specification,
                                environment: None,
                            },
                            identity(runtime, "precision-limit", &specification),
                        );
                        assert_eq!(issues.len(), 1);
                        assert_eq!(issue(&issues[0]), expected);
                    }
                }
            }
        }

        // No benchmark pins the endpoints of the precision, 0 and 18:
        // `adam-adlb-reported-precision` pins four decimals, and every float
        // benchmark without `decimals` pins its omission.
        #[test]
        fn csv_precision_endpoints_and_omission_keep_exact_saved_bytes() {
            for (decimals, expected) in [
                (Some("0"), "USUBJID,N\nS-001,0\nS-002,0\n"),
                (
                    Some("18"),
                    "USUBJID,N\nS-001,0.125000000000000000\nS-002,-0.125000000000000000\n",
                ),
                (None, "USUBJID,N\nS-001,0.125\nS-002,-0.125\n"),
            ] {
                let fixture = Fixture::new("precision-endpoints");
                fs::write(
                    fixture.0.join("spec.yaml"),
                    precision_specification(decimals, "float"),
                )
                .unwrap();
                fs::write(
                    fixture.0.join("input.csv"),
                    "USUBJID,VALUE\nS-001,0.125\nS-002,-0.125\n",
                )
                .unwrap();
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                for runtime in ["python", "r"] {
                    let request = || Request {
                        specification: &specification,
                        environment: None,
                    };
                    assert!(file_application::check(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, "precision-endpoints", &specification)
                    )
                    .is_empty());
                    let mut domain = crate::support::study::domain(
                        crate::support::host(runtime),
                        request(),
                        identity(runtime, "precision-endpoints", &specification),
                    )
                    .unwrap();
                    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                    assert!(domain.save().unwrap());
                    assert_eq!(
                        fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                        expected
                    );
                }
            }
        }
    }
    mod compiled_model {
        //! Original YAML reaches core compilation; expectations are authored from the
        //! six unchanged documents, not emitted by a compiler or the legacy plan codec.
        use crate::support::table_schema as schema;
        use crate::support::Edit as _;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::{
            shipped_schema,
            specification_source::{PreparedDocument, Source},
        };
        use yamaa_core::inheritance::Source as Identity;
        use yamaa_core::resolver::{Captured, Failure, Name, Resolver};
        use yamaa_core::{
            bound_expression::{Binding, Read},
            dataset::{
                resolved::{self as slots, BindingOrigin, CheckScope, Grain, Op, RegionId},
                Intermediate, ResolvedPlan, WindowKind,
            },
            schema::DocumentNode as N,
            specification::{PrepareError, PreparedSpecification},
            table::TableSchema,
            value::ColumnType as T,
        };

        struct Parents {
            root: PathBuf,
            reads: Vec<String>,
            unsupported: bool,
        }
        impl Resolver for Parents {
            type Error = ();
            fn inspect(&mut self, name: Name<'_>) -> Result<Identity, ()> {
                let Name::Written { written, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                assert!(
                    written.ends_with(".yaml"),
                    "preparation acquired study authority"
                );
                Ok(Identity {
                    identity: written.into(),
                    display_path: written.into(),
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, ()> {
                let Name::Located { source, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                assert!(
                    source.identity.ends_with(".yaml"),
                    "compiler read study data"
                );
                self.reads.push(source.identity.clone());
                let mut bytes = fs::read(self.root.join(&source.identity)).unwrap();
                if self.unsupported && source.identity == "spec_organization.yaml" {
                    bytes = String::from_utf8(bytes)
                        .unwrap()
                        .edit("input/lb.csv", "input/lb.xlsx")
                        .into_bytes();
                }
                Ok(Captured::resource(bytes.into(), true))
            }
            fn failure(&self, _: Name<'_>, _: &()) -> Option<Failure> {
                None
            }
            fn rebase(&mut self, _: &Identity, _: &Identity, written: &str) -> Result<String, ()> {
                Ok(written.into())
            }
        }
        fn prepare(name: &str, unsupported: bool) -> (PreparedDocument, Parents) {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks")
                .join(name);
            let entry = if name == "schema-inheritance" {
                "spec_study.yaml"
            } else {
                "spec.yaml"
            };
            let mut parents = Parents {
                root,
                reads: vec![],
                unsupported,
            };
            let document = shipped_schema::prepare(
                Source {
                    identity: entry.into(),
                    bytes: fs::read(parents.root.join(entry)).unwrap(),
                },
                entry.into(),
                &mut parents,
            )
            .unwrap();
            (document, parents)
        }
        // Binding returns the resolved plan; its dataset region carries the
        // bound plan's output, bindings, relations and checks.
        fn output_schema(plan: &ResolvedPlan) -> &TableSchema {
            &plan.region(RegionId::DATASET).output
        }
        fn names(plan: &ResolvedPlan) -> Vec<&str> {
            output_schema(plan)
                .columns()
                .iter()
                .map(|column| column.name.as_str())
                .collect()
        }
        /// The column-phase bindings, one per binding of the bound plan.
        fn columns(plan: &ResolvedPlan) -> Vec<&slots::Binding> {
            let mut previous = None;
            plan.region_bindings(RegionId::DATASET)
                .map(|binding| plan.binding(binding))
                .filter(|binding| match binding.origin {
                    BindingOrigin::Column { assignment } => {
                        previous.replace(assignment) != Some(assignment)
                    }
                    BindingOrigin::Template { .. } => false,
                })
                .collect()
        }
        fn intermediates(plan: &ResolvedPlan) -> Vec<&Intermediate> {
            (0..)
                .map_while(|index| plan.named(RegionId::DATASET, index))
                .map(|relation| plan.relation(relation).named.as_deref().unwrap())
                .collect()
        }
        fn provenance(document: &PreparedDocument, plan: &ResolvedPlan, inherited: bool) {
            for binding in plan.region_bindings(RegionId::DATASET) {
                let declaring = document
                    .declaring_source(&plan.binding(binding).path)
                    .unwrap();
                assert_eq!(
                    declaring,
                    if inherited {
                        "spec_organization.yaml"
                    } else {
                        "spec.yaml"
                    }
                );
            }
            assert_eq!(
                document.raw().locations.len(),
                document.raw().document.nodes().len()
            );
            for parent in document.parents() {
                assert_eq!(
                    parent.raw().locations.len(),
                    parent.raw().document.nodes().len()
                );
                assert_eq!(
                    parent.source().bytes,
                    fs::read(
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../../../benchmarks/schema-inheritance")
                            .join(&parent.source().identity)
                    )
                    .unwrap()
                );
            }
        }

        type Case = (
            &'static str,
            Vec<TableSchema>,
            &'static [&'static str],
            &'static [usize],
            usize,
            usize,
        );
        /// Each benchmark, with the source schemas binding receives and the
        /// output columns, key indices, template count and dataset check
        /// count authored from its specification.
        fn cases() -> [Case; 6] {
            [
                (
                    "negative-zero-division",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("LBTESTCD", T::Str),
                        ("LBSTRESN", T::Str),
                        ("LBBLRESN", T::Str),
                    ])],
                    &["STUDYID", "USUBJID", "PARAMCD", "AVAL", "BASE", "PCHG"],
                    &[0, 1, 2],
                    1,
                    0,
                ),
                (
                    "negative-integer-overflow",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("CELLCNT", T::Str),
                        ("VOLUML", T::Str),
                    ])],
                    &["STUDYID", "USUBJID", "CELLCNT", "VOLUML", "CELLTOT"],
                    &[0, 1],
                    1,
                    0,
                ),
                (
                    "adam-adlb-ordered-sum",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("LBSEQ", T::Str),
                        ("LBTESTCD", T::Str),
                        ("LBTEST", T::Str),
                        ("VISIT", T::Str),
                        ("LBSTRESN", T::Float),
                    ])],
                    &[
                        "STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL", "DTYPE",
                    ],
                    &[0, 1, 3, 2],
                    2,
                    1,
                ),
                (
                    "schema-window-functions",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("VISITN", T::Int),
                        ("VISIT", T::Str),
                        ("VSDTC", T::Date),
                        ("TRTSDT", T::Date),
                        ("VSSTRESN", T::Float),
                        ("VSEVAL", T::Str),
                    ])],
                    &[
                        "STUDYID", "USUBJID", "VISITN", "VISIT", "VSDTC", "TRTSDT", "VSSTRESN",
                        "VSEVAL", "VSSEQ", "SEVRANKC", "SEVRANKD", "PREVRES", "NEXTRES",
                        "EVALPREV", "LASTRES", "CARRYRES", "BLFL",
                    ],
                    &[1, 2],
                    1,
                    0,
                ),
                (
                    "schema-lookup",
                    vec![
                        schema(&[("USUBJID", T::Str)]),
                        schema(&[
                            ("USUBJID", T::Str),
                            ("AEDECOD", T::Str),
                            ("AEOUT", T::Str),
                            ("AEDY", T::Str),
                        ]),
                        schema(&[("LLTNAME", T::Str), ("PTNAME", T::Str)]),
                    ],
                    &["USUBJID", "DTHDY", "DTHCAUS", "DTHPTERM"],
                    &[0],
                    1,
                    0,
                ),
                (
                    "schema-inheritance",
                    vec![schema(&[
                        ("USUBJID", T::Str),
                        ("LBTESTCD", T::Str),
                        ("LBSTRESN", T::Float),
                        ("LBSTRESU", T::Str),
                    ])],
                    &["USUBJID", "PARAMCD", "AVAL", "AVALU"],
                    &[0, 1],
                    1,
                    0,
                ),
            ]
        }
        /// The output column types authored from the specification of `name`.
        fn output_types(name: &str) -> Vec<T> {
            match name {
                "negative-zero-division" => {
                    vec![T::Str, T::Str, T::Str, T::Float, T::Float, T::Float]
                }
                "negative-integer-overflow" => vec![T::Str, T::Str, T::Int, T::Int, T::Int],
                "adam-adlb-ordered-sum" => {
                    vec![T::Str, T::Str, T::Str, T::Str, T::Str, T::Float, T::Str]
                }
                "schema-window-functions" => vec![
                    T::Str,
                    T::Str,
                    T::Int,
                    T::Str,
                    T::Date,
                    T::Date,
                    T::Float,
                    T::Str,
                    T::Int,
                    T::Int,
                    T::Int,
                    T::Float,
                    T::Float,
                    T::Float,
                    T::Float,
                    T::Float,
                    T::Str,
                ],
                "schema-lookup" => vec![T::Str, T::Int, T::Str, T::Str],
                "schema-inheritance" => vec![T::Str, T::Str, T::Float, T::Str],
                _ => unreachable!(),
            }
        }
        /// `negative-zero-division` and `negative-integer-overflow`: the last
        /// column computes its authored expression over the columns it names.
        fn numeric_derivation(name: &str, plan: &ResolvedPlan) {
            let last = columns(plan).last().unwrap().expr;
            let Op::Compute(numeric) = &plan.expr(last).op else {
                panic!("numeric derivation")
            };
            assert_eq!(
                numeric.expression().expression(),
                if name == "negative-zero-division" {
                    "100 * (AVAL - BASE) / BASE"
                } else {
                    "CELLCNT * VOLUML"
                }
            );
            let bindings = if name == "negative-zero-division" {
                vec![
                    Binding {
                        name: "AVAL".into(),
                        read: Read::Column(3),
                    },
                    Binding {
                        name: "BASE".into(),
                        read: Read::Column(4),
                    },
                ]
            } else {
                vec![
                    Binding {
                        name: "CELLCNT".into(),
                        read: Read::Column(2),
                    },
                    Binding {
                        name: "VOLUML".into(),
                        read: Read::Column(3),
                    },
                ]
            };
            assert_eq!(numeric.bindings(), bindings);
        }
        /// `adam-adlb-ordered-sum`: the grouped template's total is the
        /// authored SUM over the dataset's own input.
        fn ordered_sum(plan: &ResolvedPlan, template_ids: &[slots::TemplateId]) {
            // Both templates are driven by the dataset's own input.
            for &template in template_ids {
                let driver = &plan.template(template).driver;
                assert_eq!((driver.source.index(), driver.named), (0, None));
            }
            let grain = |index: usize| &plan.template(template_ids[index]).driver.grain;
            assert_eq!(grain(0), &Grain::Records);
            assert_eq!(grain(1), &Grain::Groups(vec![0, 1, 5]));
            let total = plan
                .row_bindings(template_ids[1])
                .map(|binding| plan.binding(binding))
                .find(|binding| binding.column.index() == 5)
                .unwrap();
            let Op::Aggregate(aggregate) = &plan.expr(total.expr).op else {
                panic!("the authored grouped SUM is a bound aggregate");
            };
            assert_eq!(
                aggregate.relation(),
                &yamaa_core::dataset::AggregateRelation::CurrentGroup
            );
            assert_eq!(aggregate.expression().expression(), "SUM(LB.LBSTRESN)");
            assert_eq!(
                aggregate.bindings(),
                [yamaa_core::dataset::AggregateBinding {
                    name: "LB.LBSTRESN".into(),
                    column: 6,
                }]
            );
            assert_eq!(aggregate.expression().reductions().len(), 1);
            let reduction = &aggregate.expression().reductions()[0];
            assert_eq!(reduction.reducer, yamaa_core::expression::Reducer::Sum);
            assert_eq!(
                reduction.operand,
                yamaa_core::expression::ReductionOperand::Value("LB.LBSTRESN".into())
            );
        }
        /// `schema-window-functions`: the window columns' kinds, in column
        /// order.
        fn window_kinds(plan: &ResolvedPlan) {
            let kinds = columns(plan)
                .into_iter()
                .filter_map(|binding| match &plan.expr(binding.expr).op {
                    Op::Window { window, .. } => Some(window.kind),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                kinds,
                vec![
                    WindowKind::RowNumber,
                    WindowKind::Competition,
                    WindowKind::Dense,
                    WindowKind::RowValue {
                        column: 6,
                        offset: -1
                    },
                    WindowKind::RowValue {
                        column: 6,
                        offset: 1
                    },
                    WindowKind::RowValue {
                        column: 6,
                        offset: -1
                    },
                    WindowKind::Locf {
                        column: 6,
                        include_current: false
                    },
                    WindowKind::Locf {
                        column: 6,
                        include_current: true
                    },
                    WindowKind::BaselineFlag {
                        date: 4,
                        reference_date: 5,
                        value: 6,
                        inclusive: true
                    }
                ]
            );
        }
        /// `schema-lookup`: the two intermediates, their keys, and the
        /// secondary sources they read.
        fn lookup_intermediates(plan: &ResolvedPlan) {
            let intermediates = intermediates(plan);
            assert_eq!(
                intermediates
                    .iter()
                    .map(|item| item.identifier.as_str())
                    .collect::<Vec<_>>(),
                ["DEATHEV", "MEDDRA_PT"]
            );
            assert_eq!(
                intermediates[0].keys,
                [yamaa_core::dataset::IntermediateKey {
                    source_column: 0,
                    path: "intermediates.DEATHEV.key".into(),
                    value: yamaa_core::dataset::MatchValue::Variable(
                        yamaa_core::dataset::MatchRead::Output {
                            column: 0,
                            identifier: "USUBJID".into()
                        }
                    ),
                }]
            );
            assert_eq!(
                intermediates[1].keys,
                [yamaa_core::dataset::IntermediateKey {
                    source_column: 0,
                    path: "intermediates.MEDDRA_PT.key".into(),
                    value: yamaa_core::dataset::MatchValue::Variable(
                        yamaa_core::dataset::MatchRead::Output {
                            column: 2,
                            identifier: "DTHCAUS".into()
                        }
                    ),
                }]
            );
            assert_eq!(
                plan.region(RegionId::DATASET)
                    .secondary
                    .iter()
                    .map(|source| source.name.as_str())
                    .collect::<Vec<_>>(),
                ["AE", "MEDDRA"]
            );
        }
        /// `schema-inheritance`: the one source, and the layer that declares
        /// a column's label and an input's types.
        fn inherited_layers(compiled: &PreparedSpecification, document: &PreparedDocument) {
            assert_eq!(
                compiled
                    .sources()
                    .iter()
                    .map(|source| source.name.as_str())
                    .collect::<Vec<_>>(),
                ["LB"]
            );
            assert_eq!(
                document.declaring_source("columns.AVAL.label"),
                Some("spec_study.yaml")
            );
            assert_eq!(
                document.declaring_source("input.LB.types"),
                Some("spec_compound.yaml")
            );
        }

        // A unit seam: the bound plans of six benchmarks' specifications, which
        // no benchmark's expected files pin.
        #[test]
        fn all_six_original_yaml_documents_compile_against_independent_model_expectations() {
            for (name, schemas, output, keys, templates, checks) in cases() {
                let (document, parents) = prepare(name, false);
                // Only YAML parent captures are possible. No study bytes are supplied;
                // binding receives independently enumerated immutable metadata. The
                // chain is captured from the entry upward: the compound level names
                // the organization level.
                assert_eq!(
                    parents.reads,
                    if name == "schema-inheritance" {
                        vec!["spec_compound.yaml", "spec_organization.yaml"]
                    } else {
                        vec![]
                    }
                );
                let compiled = crate::support::specification_run::compile(&document).unwrap();
                let plan = compiled
                    .bind_sources(&schemas.iter().collect::<Vec<_>>())
                    .unwrap();
                assert_eq!(names(&plan), output, "{name}");
                assert_eq!(
                    output_schema(&plan)
                        .columns()
                        .iter()
                        .map(|column| column.kind)
                        .collect::<Vec<_>>(),
                    output_types(name),
                    "{name}"
                );
                let projection = match name {
                    "negative-integer-overflow" => vec!["STUDYID", "USUBJID", "CELLTOT"],
                    "adam-adlb-ordered-sum" => vec![
                        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "DTYPE",
                    ],
                    _ => output.to_vec(),
                };
                assert_eq!(compiled.projection(), projection, "{name}");
                assert_eq!(
                    plan.keys()
                        .iter()
                        .map(|key| key.index())
                        .collect::<Vec<_>>(),
                    keys,
                    "{name}"
                );
                let template_ids: Vec<_> = plan.region_templates(RegionId::DATASET).collect();
                assert_eq!(template_ids.len(), templates, "{name}");
                assert_eq!(
                    plan.checks()
                        .iter()
                        .filter(|check| check.scope == CheckScope::Dataset)
                        .count(),
                    checks,
                    "{name}"
                );
                provenance(&document, &plan, name == "schema-inheritance");
                match name {
                    "negative-zero-division" | "negative-integer-overflow" => {
                        numeric_derivation(name, &plan);
                    }
                    "adam-adlb-ordered-sum" => ordered_sum(&plan, &template_ids),
                    "schema-window-functions" => window_kinds(&plan),
                    "schema-lookup" => lookup_intermediates(&plan),
                    "schema-inheritance" => inherited_layers(&compiled, &document),
                    _ => unreachable!(),
                }
            }
        }

        // A unit seam: where the finding of an edited layer of
        // `schema-inheritance` is located, which no benchmark pins.
        #[test]
        fn inherited_unknown_profile_retains_written_layer_and_location_before_study_effects() {
            let (document, parents) = prepare("schema-inheritance", true);
            let Err(PrepareError::Invalid(findings)) =
                crate::support::specification_run::compile(&document)
            else {
                panic!("unknown source profile")
            };
            assert_eq!(findings.len(), 1);
            let diagnostic = findings[0].diagnostic();
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().rule
                ),
                ("validation", "source_profile_unknown", Some("INPUT-1"))
            );
            assert_eq!(diagnostic.spec_paths, ["input.LB.path"]);
            assert_eq!(
                diagnostic.context,
                [
                    (
                        "dataset".into(),
                        yamaa_core::json::Value::String("LB".into())
                    ),
                    (
                        "path".into(),
                        yamaa_core::json::Value::String("input/lb.xlsx".into())
                    )
                ]
                .into()
            );
            assert_eq!(
                document.declaring_source(&diagnostic.spec_paths[0]),
                Some("spec_organization.yaml")
            );
            let parent = document
                .parents()
                .iter()
                .find(|parent| parent.source().identity == "spec_organization.yaml")
                .unwrap();
            let raw = &parent.raw().document;
            let input = raw.field(raw.root(), "input").unwrap();
            let node = raw.field(input, "LB").unwrap();
            assert_eq!(raw.nodes()[node], N::Text("input/lb.xlsx".into()));
            let position = parent.raw().locations[node];
            // The line the layer writes the input on, after its header.
            let line = std::str::from_utf8(&parent.source().bytes)
                .unwrap()
                .lines()
                .position(|line| line.trim() == "LB: input/lb.xlsx")
                .unwrap();
            assert_eq!(position.line, line + 1);
            assert!(position.offset < parent.source().bytes.len());
            assert_eq!(
                parents.reads,
                ["spec_compound.yaml", "spec_organization.yaml"]
            );
        }
    }
}
