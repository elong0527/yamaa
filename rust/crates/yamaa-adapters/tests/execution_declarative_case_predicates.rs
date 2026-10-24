#![cfg(feature = "yaml")]
//! Execution contracts: declarative case predicates, declarative flags,
//! declarative intermediate keys, declarative intermediate ranges,
//! declarative output ordering, declarative primary intermediates and
//! declarative row intermediates. The case predicate, correlated predicate,
//! single key and intermediate key cases that write files, run one
//! specification and compare what it reports are cases of
//! `tests/fixtures/diagnostics/declarative_case_predicates.json` (#3798); the
//! tests of those families here read benchmarks and shared truth files, which
//! the replay does not.
mod execution {
    mod declarative_case_predicates {
        //! Independently authored full case reports and unchanged original CSV truth.
        use std::fs;
        use yamaa_adapters::file_application;
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
        use yamaa_adapters::file_application;
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
        use yamaa_adapters::file_application;
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
        use crate::support::Edit as _;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::file_application;
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
        const BODY: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int, X: int}}
  D: {path: donor.csv, types: {ID: int, LOW: int, HIGH: int, ORD: int}}
output: {path: result.csv, columns: [ID, SEQ, X, A, B]}
intermediates:
  - id: P
    dataset: D
    key: [ID]
    filter: "D.ELIG = 'Y'"
    between: {value: X, lower: LOW, upper: HIGH}
    columns: [V, W, V]
    order_by: [D.ORD]
    keep: first
    absent: absent
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: X, type: int, derivation: SRC.X}
  - {name: A, type: str, derivation: P.V}
  - {name: B, type: str, derivation: P.W}
"#;
        fn populate(f: &Fixture) {
            fs::write(
                f.0.join("src.csv"),
                "ID,SEQ,X\n1,1,3\n1,2,4\n1,3,\n1,4,6\n2,1,3\n1,5,2\n",
            )
            .unwrap();
            fs::write(f.0.join("donor.csv"), "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,3,2,Y,edge,\n1,3,5,1,Y,later,paired\n1,,10,3,Y,missing-bound,hidden\n1,1,10,0,N,filtered,hidden\n").unwrap();
        }

        /// No benchmark derives a range bound as a date from a timestamp, so
        /// that a donor on the current date qualifies:
        /// `adam-adae-post-dose-window` derives a datetime bound, and
        /// `adam-adpc-relative-times` reads its lower bound as stored.
        #[test]
        fn derived_date_bound_keeps_the_last_record_on_the_current_date() {
            let f = Fixture::new();
            fs::write(
                f.0.join("src.csv"),
                "ID,TRTSDT\nS01,2030-01-10\nS02,2030-02-01\nS03,\n",
            )
            .unwrap();
            fs::write(
        f.0.join("donor.csv"),
        "ID,VSDTC,WEIGHT\nS01,2030-01-05T08:00,70\nS01,2030-01-10T07:30,71\nS01,2030-01-15T09:00,72\nS02,2030-02-02T08:00,80\nS03,2030-01-03,60\n",
    )
    .unwrap();
            let body = r#"yamaa: "1.0"
type: spec
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {TRTSDT: date}}
  VS: {path: donor.csv, types: {WEIGHT: float}}
output: {path: result.csv, columns: [ID, TRTSDT, WEIGHTBL]}
intermediates:
  - id: BL
    dataset: VS
    key: [ID]
    derivations:
      ADT: {to_date: {source: VSDTC}}
    between: {value: TRTSDT, lower: ADT}
    order_by: [VS.ADT]
    keep: last
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: TRTSDT, type: date, derivation: SRC.TRTSDT}
  - {name: WEIGHTBL, type: float, derivation: BL.WEIGHT}
"#;
            let mut result = f.run(body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(!f.0.join("result.csv").exists());
            assert!(result.save().unwrap());
            // S01's same-day timestamp qualifies after the bound is derived as a date.
            // S02 has only a later donor; S03's missing current value selects no donor.
            assert_eq!(
                fs::read(f.0.join("result.csv")).unwrap(),
                b"ID,TRTSDT,WEIGHTBL\nS01,2030-01-10,71\nS02,2030-02-01,\nS03,,\n"
            );
        }

        /// No benchmark projects a range intermediate's `columns`, one of them
        /// repeated, nor pins a range selection's handler ledger.
        #[test]
        fn inclusive_ranges_keep_one_record_choice_and_project_only_named_reads() {
            let f = Fixture::new();
            populate(&f);
            let mut result = f.run(BODY);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,absent,absent\n2,1,3,absent,absent\n1,5,2,edge,\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":3},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":3}
                ])
            );
        }
        /// No benchmark selects a named intermediate through a range without a
        /// lower bound.
        #[test]
        fn unstated_lower_bound_never_turns_a_missing_unused_donor_field_into_absence() {
            let f = Fixture::new();
            populate(&f);
            let mut result =
                f.run(&BODY.edit("value: X, lower: LOW, upper: HIGH", "value: X, upper: HIGH"));
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,missing-bound,hidden\n2,1,3,absent,absent\n1,5,2,later,paired\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
                    {"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":2},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
                    {"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":2}
                ])
            );
        }
        /// No benchmark refuses a read of a donor field that the
        /// intermediate's `columns` leaves out (LOOKUP-11).
        #[test]
        fn a_hidden_read_has_the_complete_original_reference_finding_and_no_output() {
            let f = Fixture::new();
            populate(&f);
            let mut result = f.run(&BODY.edit("derivation: P.W", "derivation: P.LOW"));
            assert_eq!(
                result
                    .issues()
                    .iter()
                    .map(|issue| {
                        let mut value = yamaa_adapters::report_storage::diagnostic_value(issue);
                        value["context"] = yamaa_core::json::Value::Object(issue.context.clone());
                        value
                    })
                    .collect::<Vec<_>>(),
                [
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","rule":"LOOKUP-11","spec_paths":["columns.B.derivation.source"],"context":{"identifier":"P.LOW"}})
                ]
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!f.0.join("result.csv").exists());
        }
        /// No benchmark answers an absence with the 64-bit minimum or a
        /// fraction, nor pins the ledger that counts it.
        #[test]
        fn typed_absence_replacements_keep_their_full_value_and_ledger() {
            for (case, replacement, kind, expected) in [
                (
                    "integer",
                    "-9223372036854775808",
                    "int",
                    b"ID,SEQ,X,A,B\n2,1,3,-9223372036854775808,-9223372036854775808\n".as_slice(),
                ),
                (
                    "float",
                    "0.5",
                    "float",
                    b"ID,SEQ,X,A,B\n2,1,3,0.5,0.5\n".as_slice(),
                ),
            ] {
                let f = Fixture::new();
                fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
                fs::write(
                    f.0.join("donor.csv"),
                    "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
                )
                .unwrap();
                let body = BODY
                    .edit("absent: absent", &format!("absent: {replacement}"))
                    .edit("name: A, type: str", &format!("name: A, type: {kind}"))
                    .edit("name: B, type: str", &format!("name: B, type: {kind}"));
                let mut result = f.run(&body);
                assert!(result.issues().is_empty(), "{case}: {:?}", result.issues());
                assert!(result.save().unwrap(), "{case}");
                assert_eq!(
                    fs::read(f.0.join("result.csv")).unwrap(),
                    expected,
                    "{case}"
                );
                assert_eq!(
                    result.observations()["handler_counts"],
                    yamaa_adapters::json!([
                        {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":0},
                        {"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":1},
                        {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":0},
                        {"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":1}
                    ]),
                    "{case}"
                );
            }
        }

        /// No benchmark reads a named range intermediate in a row template
        /// whose range value and key are column defaults:
        /// `schema-lookup-source-range-default` promotes the default of a
        /// structured source's range value. None pins that ledger either.
        #[test]
        fn range_current_outputs_and_scalar_keys_promote_their_defaults_before_row_reads() {
            let f = Fixture::new();
            populate(&f);
            let body = BODY
        .edit("\ncolumns:\n", "\nrows:\n  - id: selected\n    dataset: SRC\n    derivations: {A: P.V, B: P.W}\ncolumns:\n")
        .edit("name: A, type: str, derivation: P.V", "name: A, type: str")
        .edit("name: B, type: str, derivation: P.W", "name: B, type: str");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,absent,absent\n2,1,3,absent,absent\n1,5,2,edge,\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows.selected.derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows.selected.derivations.A.source.absent","handler":"absent","count":3},
                    {"spec_path":"rows.selected.derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows.selected.derivations.B.source.absent","handler":"absent","count":3}
                ])
            );
        }

        /// No benchmark answers an absence with a boolean.
        #[test]
        fn boolean_absence_replacement_reaches_a_null_test_without_conversion() {
            let f = Fixture::new();
            fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
            fs::write(
                f.0.join("donor.csv"),
                "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
            )
            .unwrap();
            let body = BODY.edit("absent: absent", "absent: true")
        .edit("derivation: P.V", "derivation: {flag: {condition: 'P.V IS NOT NULL', true_value: Y, false_value: N, missing: null}}")
        .edit("derivation: P.W", "derivation: {flag: {condition: 'P.W IS NOT NULL', true_value: Y, false_value: N, missing: null}}");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(f.0.join("result.csv")).unwrap(),
                b"ID,SEQ,X,A,B\n2,1,3,Y,Y\n"
            );
            // Predicate normalization retains the literal value and discards recovery
            // sites; each flag's UNKNOWN answer is registered and never reached.
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.flag.missing","handler":"missing","count":0},
                    {"spec_path":"columns.B.derivation.flag.missing","handler":"missing","count":0}
                ])
            );
        }

        /// No benchmark compares an integer beyond 2^53 with float range
        /// endpoints.
        #[test]
        fn integer_current_values_do_not_round_into_a_float_donor_endpoint() {
            let f = Fixture::new();
            fs::write(
                f.0.join("src.csv"),
                "ID,SEQ,X\n1,1,9007199254740993\n1,2,9007199254740992\n",
            )
            .unwrap();
            fs::write(
                f.0.join("donor.csv"),
                "ID,LOW,HIGH,ORD,ELIG,V,W\n1,9007199254740992,9007199254740992,1,Y,edge,paired\n",
            )
            .unwrap();
            let body = BODY.edit("LOW: int, HIGH: int", "LOW: float, HIGH: float");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
        fs::read(f.0.join("result.csv")).unwrap(),
        b"ID,SEQ,X,A,B\n1,1,9007199254740993,absent,absent\n1,2,9007199254740992,edge,paired\n"
    );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":0},
                    {"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":1},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":0},
                    {"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":1}
                ])
            );
        }

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
        use yamaa_adapters::file_application;
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
        fn authored(spec: &str, input: &[u8]) -> Fixture {
            let f = Fixture::new("schema-ordering");
            fs::write(f.0.join("spec.yaml"), spec).unwrap();
            fs::write(f.0.join("source.csv"), input).unwrap();
            f
        }
        /// The benchmark states the bytes its ordering saves.
        #[test]
        fn original_ordering_artifact_matches_immutable_csv_truth() {
            crate::support::benchmarks::reproduce("schema-ordering");
        }
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
        /// `negative-csv-empty-string-present` refuses the declaration with its
        /// input present; no benchmark shows the refusal comes before the
        /// input is read, here with no input file.
        #[test]
        fn csv_present_empty_is_rejected_before_study_bytes_are_requested() {
            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: absent.csv, empty_string: present}}
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
                b"unused\n",
            );
            let issues = fixture.run().err().expect("static declaration");
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"empty_string_present_unsupported","rule":"INPUT-9","spec_paths":["input.SRC.empty_string"],"context":{"dataset":"SRC"}}),
                ],
                "empty_string: present on a delimited input",
            );
            assert!(!fixture.0.join("result.csv").exists());
        }

        /// Root predicates gate raw base records before key conversion and keep first key appearance.
        /// No benchmark's root filter reads a key field whose stored text
        /// (`02`) differs from its converted key.
        #[test]
        fn root_filter_reads_raw_driver_values_before_key_derivation() {
            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID, V]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: V, type: str, derivation: SRC.V}
"#,
                b"ID,V,KEEP\nbad,excluded,N\n02,seven,Y\n2,conflict,\n1,eight,Y\n2,seven,Y\n",
            );
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,V\n2,seven\n1,eight\n"
            );
            // A raw string comparison would fail if the filter read the completed integer key.
            let raw = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "SRC.ID = '02'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: int, derivation: SRC.ID}]
"#,
                b"ID\n02\n3\n",
            );
            let mut result = raw.run().unwrap();
            assert!(result.issues().is_empty());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(raw.0.join("result.csv")).unwrap(), b"ID\n2\n");
        }
        /// No benchmark's root filter reads a base declared after another
        /// input: `adam-adrs-response-records` and `adam-adtte-dor` declare
        /// theirs first.
        #[test]
        fn root_filter_uses_explicit_base_independently_of_input_declaration_order() {
            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {OTHER: other.csv, SRC: source.csv}
base: SRC
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
                b"ID,KEEP\na,N\nb,Y\n",
            );
            fs::write(fixture.0.join("other.csv"), b"ID,KEEP\nwrong,Y\n").unwrap();
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID\nb\n");
        }
        /// No benchmark's root filter reads a column the specification
        /// constructs. `negative-root-filter-with-rows` refuses a root filter
        /// beside row templates, and its `error.yaml` states the first issue
        /// in part; what no benchmark pins is that the refusal is the run's
        /// only issue, before any input is read.
        #[test]
        fn root_filter_forbids_constructed_columns_and_conflicting_row_templates() {
            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "ID = 'a'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
                b"ID\na\n",
            );
            let result = fixture.run().unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            let issue = &result.issues()[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "phase_boundary", Some("ROW-9"))
            );
            assert_eq!(issue.spec_paths, ["filter"]);
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"identifier":"ID", "available_phase":"column_derivation", "required_phase":"row_filter"})
            );
            let mut spec = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            spec.push_str("rows: [{id: records, dataset: SRC}]\n");
            fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
            fs::remove_file(fixture.0.join("source.csv")).unwrap();
            let issues = fixture
                .run()
                .err()
                .expect("static conflicting row construction");
            assert_eq!(issues.len(), 1);
            assert_eq!(
                (issues[0].condition(), issues[0].rule()),
                ("conflicting_row_construction", Some("ROW-9"))
            );
        }

        /// `schema-source-ordinal` keeps each record's position through a row
        /// filter; no benchmark's ordinal input holds a record on two lines,
        /// or no record at all.
        #[test]
        fn source_ordinals_precede_filtering_and_count_multiline_records_once() {
            let spec = r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, ordinal: REC}}
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#;
            for (input, expected) in [
                (
                    b"ID,KEEP,NOTE\na,N,first\nb,Y,\"two\nlines\"\nc,Y,last\n".as_slice(),
                    b"ID,POSITION\nb,2\nc,3\n".as_slice(),
                ),
                (b"ID,KEEP,NOTE\n", b"ID,POSITION\n"),
            ] {
                let fixture = authored(spec, input);
                let mut result = fixture.run().unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
            }
        }

        /// The first case is the benchmark's own specification, compared with
        /// the issue its `error.yaml` states; what no benchmark pins is that it
        /// is the run's only issue, and an ordinal on the fixed ODM schema
        /// (INPUT-4).
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

            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: absent.parquet, ordinal: REC}}
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: {odm: SRC.ITEM}}]
"#,
                b"unused\n",
            );
            let issues = fixture
                .run()
                .err()
                .expect("fixed schema rejects ordinal before capture");
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"odm_schema_field_type","rule":"INPUT-4","spec_paths":["input.SRC.ordinal"],"context":{"dataset":"SRC","declared":"ordinal"}}),
                ],
                "an ordinal on the fixed ODM schema",
            );
        }

        /// No benchmark declares an ordinal beside a column's `metadata.odm`.
        #[test]
        fn metadata_odm_spelling_does_not_turn_an_ordinal_source_into_odm() {
            let fixture = authored(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, ordinal: REC}}
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: int, metadata: {odm: SRC.ITEM}, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#,
                b"ID\n3\n1\n",
            );
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,POSITION\n3,1\n1,2\n"
            );
        }

        /// No benchmark declares an ordinal on an input that a nested ODM
        /// derivation reads.
        #[test]
        fn nested_column_and_row_odm_derivations_still_reject_ordinals_before_capture() {
            for declarations in [
        "columns:\n  - name: ID\n    type: str\n    derivation:\n      case:\n        - when: \"1 = 1\"\n          then: {odm: SRC.ITEM}\n        - otherwise: \"'x'\"\n",
        "columns: [{name: ID, type: str, derivation: SRC.ID}]\nrows:\n  - id: records\n    derivations:\n      ID: {odm: SRC.ITEM}\n",
    ] {
        let fixture = authored(
            &format!("yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: absent.parquet, ordinal: REC}}}}\noutput: {{path: result.csv, columns: [ID]}}\n{declarations}"),
            b"unused\n",
        );
        let issues = fixture.run().err().expect("ODM ordinal admission before capture");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition(), "odm_schema_field_type");
        assert_eq!(issues[0].spec_paths, ["input.SRC.ordinal"]);
    }
        }

        /// No benchmark declares an ordinal on a Parquet input.
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
    mod declarative_primary_intermediates {
        //! Same-input donor selection retains one captured relation across driver scopes.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("primary-intermediate");
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
                let specification = self.0.join("spec.yaml");
                fs::write(&specification, body).unwrap();
                let specification = specification.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification,
                        environment: None,
                    },
                    crate::support::study::identity(
                        "independent-primary-intermediate",
                        specification,
                    ),
                )
            }
        }
        const BODY: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, SEQ, A, B]}
intermediates:
  - id: P
    dataset: SRC
    key: [ID]
    filter: "SRC.ELIG = 'Y'"
    order_by: [SRC.SEQ]
    keep: last
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
        fn populate(fixture: &Fixture) {
            fs::write(
                fixture.0.join("src.csv"),
                "ID,SEQ,ELIG,A,B\n1,1,Y,early,x\n1,2,Y,last,\n2,1,N,hidden,hidden\n",
            )
            .unwrap();
        }
        /// No benchmark pins the handler ledger of a selection whose donor is
        /// the driver's own input.
        #[test]
        fn driver_and_donor_share_the_input_but_selection_stays_at_the_donor_grain() {
            let fixture = Fixture::new();
            populate(&fixture);
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SEQ,A,B\n1,1,last,\n1,2,last,\n2,1,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":1},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":1},
                ])
            );
        }
        /// No benchmark pins the handler ledger of a row template over another
        /// input that reads the base's records as donors.
        #[test]
        fn a_different_template_driver_still_reads_the_original_primary_donors() {
            let fixture = Fixture::new();
            populate(&fixture);
            fs::write(fixture.0.join("alt.csv"), "ID\n1\n3\n2\n").unwrap();
            let body = BODY.edit("keys: [ID, SEQ]", "keys: [ID]")
        .edit("  SRC: {path: src.csv, types: {ID: int, SEQ: int}}", "  SRC: {path: src.csv, types: {ID: int, SEQ: int}}\n  ALT: {path: alt.csv, types: {ID: int}}")
        .edit("output: {path: result.csv, columns: [ID, SEQ, A, B]}", "output: {path: result.csv, columns: [ID, A, B]}")
        .edit("columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: A, type: str, derivation: P.A}\n  - {name: B, type: str, derivation: P.B}", "rows:\n  - id: alternate\n    dataset: ALT\n    derivations: {ID: ALT.ID, A: P.A, B: P.B}\ncolumns:\n  - {name: ID, type: int}\n  - {name: A, type: str}\n  - {name: B, type: str}");
            let mut result = fixture.run(&body).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,A,B\n1,last,\n3,,\n2,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows.alternate.derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows.alternate.derivations.A.source.absent","handler":"absent","count":2},
                    {"spec_path":"rows.alternate.derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows.alternate.derivations.B.source.absent","handler":"absent","count":2},
                ])
            );
        }
    }
    mod declarative_row_intermediates {
        //! Unchanged public YAML and independent plain-reference outputs/findings.
        use std::fs;
        use yamaa_adapters::file_application;
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
