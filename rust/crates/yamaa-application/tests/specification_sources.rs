#![cfg(feature = "yaml")]
//! Native specification run assertions.
mod execution {
    mod specification_run {
        // A specification run whose findings a case can state through the public
        // build is a case of `fixtures/diagnostics/specification_runs.json`. These
        // tests keep the seams no case states and replay authored fixtures.
        use crate::support::ports::NoFunctions;
        use crate::support::specification_run::*;
        use crate::support::Edit as _;
        use yamaa_core::application_observer::Scope;

        /// Seam: the scripted `Sources` port's read count and the captured source table's cells.
        #[test]
        #[cfg(feature = "parquet")]
        fn parquet_input_policy_and_owned_source_observations_use_the_shared_build_path() {
            use crate::support::ports::Sources;
            use yamaa_application::specification_run::PreparedRun;
            use yamaa_core::table::ValueRef;
            for (policy, first) in [
                ("missing", ValueRef::Missing),
                ("present", ValueRef::Str("")),
            ] {
                let raw = format!(
                    r#"yamaa: "1.0"
type: spec
domain: TEST
input:
  SRC: {{path: input.PARQUET, empty_string: {policy}}}
keys: [I]
columns:
  - {{name: I, type: int, derivation: SRC.I}}
  - {{name: S, type: str, derivation: SRC.S}}
output: {{path: output.csv, columns: [I, S]}}
"#
                );
                let run = PreparedRun::prepare(prepare(raw.as_bytes())).unwrap();
                let content = crate::support::parquet_files::file("text");
                let mut port = Sources::by_path([("input.PARQUET", content.as_slice())]);
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                assert_eq!(port.reads(), 1);
                drop(port);
                drop(run);
                let source = attempt.dataset.sources[0].table.as_ref().unwrap();
                assert_eq!(source.cell(0, 1).unwrap(), first);
                assert_eq!(source.cell(1, 1).unwrap(), ValueRef::Str("X"));
                assert!(attempt.dataset.sources[0].read.captured);
                assert_eq!(attempt.dataset.sources[0].read.snapshots_created, Some(1));
                assert!(attempt.dataset.result.unwrap().result.is_ok());
            }
        }

        /// Seam: the core plan's `ExecutionError::Numeric` over an in-memory table and the failing
        /// record's key values, against each benchmark's own `expected/error.yaml`.
        #[test]
        fn original_yaml_to_checked_numeric_failure_without_python_plan() {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            for (name, keys) in [
                ("negative-zero-division", vec!["PILOT7", "P7-742", "ALT"]),
                (
                    "negative-integer-overflow",
                    vec!["YAMAA-01", "YAMAA-01-102"],
                ),
            ] {
                // The failure each benchmark states, in the language it is
                // written in (#3422).
                let oracle = crate::support::benchmarks::error(name);
                let (condition, rule, path) = (
                    oracle["condition"].as_str().unwrap(),
                    oracle["rule"].as_str().unwrap(),
                    oracle["spec_paths"][0].as_str().unwrap(),
                );
                let case = root.join("benchmarks").join(name);
                let spec = prepare(&std::fs::read(case.join("spec.yaml")).unwrap());
                let source = csv_source::parse_text_table(
                    &std::fs::read(case.join("input/lb.csv")).unwrap(),
                )
                .unwrap();
                let plan = lower(&spec, source.schema());
                drop(spec);
                let attempt = plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                    &|| false,
                );
                assert!(attempt.handler_counts.is_empty());
                let error = attempt.result.unwrap_err();
                let dataset::ExecutionError::Numeric { error, identity } = *error else {
                    panic!("unexpected {error:?}")
                };
                assert_eq!(error.spec_path, path);
                let EvaluationErrorKind::Condition(actual) = error.kind;
                assert_eq!(actual.condition(), condition);
                assert_eq!(actual.rule(), rule);
                assert_eq!(
                    identity.unwrap().values,
                    keys.into_iter()
                        .map(|s| Value::Str(s.into()))
                        .collect::<Vec<_>>()
                );
                println!("{name}: {actual:?}");
            }
        }

        /// Seam: the core plan's rows over an in-memory table and its
        /// `ExecutionError::MultipleValues`.
        #[test]
        fn repeated_keys_collect_raw_readings_before_conversion() {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let spec = prepare(
                &std::fs::read(root.join("benchmarks/negative-zero-division/spec.yaml")).unwrap(),
            );
            let same =
                b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,12,10\nS,P,ALT,12,10\n";
            let table = csv_source::parse_text_table(same).unwrap();
            let plan = lower(&spec, table.schema());
            let result = plan
                .execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                    &|| false,
                )
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[vec![
                    Value::Str("S".into()),
                    Value::Str("P".into()),
                    Value::Str("ALT".into()),
                    Value::float(12.0),
                    Value::float(10.0),
                    Value::float(20.0)
                ]]
            );
            let different =
                b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,12,10\nS,P,ALT,12.0,10\n";
            let table = csv_source::parse_text_table(different).unwrap();
            let attempt = plan.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
                &|| false,
            );
            assert!(attempt.handler_counts.is_empty());
            let dataset::ExecutionError::MultipleValues {
                path,
                identifier,
                value_count,
                identity,
            } = *attempt.result.unwrap_err()
            else {
                panic!("expected raw distinct readings")
            };
            assert_eq!(path, "columns.AVAL.derivation.source");
            assert_eq!(identifier, "LB.LBSTRESN");
            assert_eq!(value_count, 2);
            assert_eq!(
                identity.unwrap().values,
                vec![
                    Value::Str("S".into()),
                    Value::Str("P".into()),
                    Value::Str("ALT".into())
                ]
            );
        }

        /// Seam: the internal `PrepareError` and `BindError` variants of compiling and binding.
        #[test]
        fn unsupported_operations_handlers_and_metadata_are_refused_before_binding() {
            use yamaa_core::specification::PrepareError;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let raw =
                std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml"))
                    .unwrap();
            let written = format!("{raw}\nmetadata:\n  label: reserved\n");
            let captured = prepare(written.as_bytes());
            let error = compile(&captured).unwrap_err();
            let PrepareError::Invalid(findings) = error else {
                panic!("{error:?}")
            };
            assert!(
                matches!(&findings[0], yamaa_core::specification::PreflightFinding::Scalar(diagnostic) if diagnostic.code == yamaa_core::diagnostic::ConditionCode::SubmissionReservedMetadata && diagnostic.spec_paths == ["metadata.label"])
            );
            let written = raw.edit("100 * (AVAL - BASE) / BASE", "LB.LBSTRESN + 1");
            let captured = prepare(written.as_bytes());
            let prepared = compile(&captured).unwrap();
            let schema = TableSchema::new(vec![]).unwrap();
            assert!(matches!(
                prepared.bind(&schema),
                Err(yamaa_core::specification::BindError::Invalid(_))
            ));
        }

        /// Seam: the internal `BindFinding` and `reference_binding::Diagnostic` variants of
        /// binding.
        #[test]
        fn binding_uses_shared_reference_and_dependency_findings() {
            use yamaa_core::reference_binding::Diagnostic;
            use yamaa_core::specification::{BindError, BindFinding};
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let raw =
                std::fs::read_to_string(root.join("benchmarks/negative-zero-division/spec.yaml"))
                    .unwrap();
            let table =
                csv_source::parse_text_table(b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\n")
                    .unwrap();
            for (expression, expected_suggestion) in
                [("MISSING + 1", false), ("LBSTRESN + 1", true)]
            {
                let written = raw.edit("100 * (AVAL - BASE) / BASE", expression);
                let captured = prepare(written.as_bytes());
                let prepared = compile(&captured).unwrap();
                let error = prepared.bind(table.schema()).unwrap_err();
                let BindError::Invalid(findings) = &error else {
                    panic!("{error:?}")
                };
                let [BindFinding::OutputReference { finding, .. }] = findings.as_slice() else {
                    panic!("{error:?}")
                };
                assert_eq!(
                    *finding,
                    if expected_suggestion {
                        Diagnostic::UnresolvableName { dataset: 0 }
                    } else {
                        Diagnostic::UnknownField
                    }
                );
            }
            let written = raw.edit("100 * (AVAL - BASE) / BASE", "PCHG + 1");
            let captured = prepare(written.as_bytes());
            assert!(matches!(
                compile(&captured)
                    .unwrap()
                    .bind(table.schema()),
                Err(BindError::Invalid(findings)) if matches!(findings.as_slice(), [BindFinding::Dependencies { .. }])
            ));
        }

        /// Seam: the prepared run's source and the condition outcome's failing record (its
        /// position, key values, partition and matched key) beside its portable issue row.
        #[test]
        fn original_yaml_run_returns_complete_independent_portable_failures() {
            use yamaa_adapters::json;
            use yamaa_application::specification_run::PreparedRun;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            for (name, condition, rule, expr, keys) in [
                (
                    "negative-zero-division",
                    "division_by_zero",
                    "FORMULA-15",
                    "100 * (AVAL - BASE) / BASE",
                    vec!["PILOT7", "P7-742", "ALT"],
                ),
                (
                    "negative-integer-overflow",
                    "integer_overflow",
                    "VALUE-5",
                    "CELLCNT * VOLUML",
                    vec!["YAMAA-01", "YAMAA-01-102"],
                ),
            ] {
                let case = root.join("benchmarks").join(name);
                let captured = prepare(&std::fs::read(case.join("spec.yaml")).unwrap());
                let run = PreparedRun::prepare(captured).unwrap();
                assert_eq!(run.source().name, "LB");
                assert_eq!(run.source().path, "input/lb.csv");
                let mut context = json!({"expr":expr});
                if condition == "integer_overflow" {
                    context["minimum"] = json!(i64::MIN);
                    context["maximum"] = json!(i64::MAX);
                    context["value"] = json!("9223372036854776832");
                }
                // The portable diagnostic now carries the geometry the core
                // recorded for the failing expression instead of discarding it.
                let (span, route): ((usize, usize), Vec<String>) = match condition {
                    "division_by_zero" => ((0, 26), Vec::new()),
                    _ => ((0, 16), Vec::new()),
                };
                // The path each benchmark states, in the language it is
                // written in (#3422).
                let expected = json!({"phase":"derivation","condition":condition,"rule":rule,
                    "spec_paths":crate::support::benchmarks::error(name)["spec_paths"].clone(),"context":context,
                    "source_span":{"start":span.0,"end":span.1},"operand_route":route});
                let row = keys
                    .iter()
                    .map(|key| yamaa_core::value::Value::Str((*key).into()))
                    .collect::<Vec<_>>();
                for _ in 0..2 {
                    let response = run
                        .execute_csv(&std::fs::read(case.join("input/lb.csv")).unwrap())
                        .unwrap();
                    assert!(response.table.is_none());
                    let yamaa_adapters::dataset_result::Outcome::Condition(found) =
                        &response.outcome
                    else {
                        panic!("an arithmetic failure is a condition outcome");
                    };
                    assert_eq!(
                        yamaa_adapters::report_storage::diagnostic_value(&found.diagnostic),
                        expected
                    );
                    let identity = found.identity.as_ref().unwrap();
                    assert_eq!((identity.position, &identity.values), (1, &row));
                    assert!(found.partition.is_none() && found.matched_key.is_none());
                }
            }
        }

        /// Seam: the internal `Error::Source` and `BindFinding` variants of a run over raw bytes.
        #[test]
        fn source_ingestion_failure_precedes_deferred_formula_diagnostics() {
            use yamaa_application::specification_run::{Error, PreparedRun};
            use yamaa_core::specification::{BindError, BindFinding};
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/negative-zero-division");
            // The benchmark as 1.0 writes it, since the test edits its text
            // (#3422); its input is the benchmark's own.
            let raw = crate::support::benchmarks::converted("negative-zero-division");
            for (expression, qualified) in [("AVAL +", false), ("LB.LBSTRESN + 1", true)] {
                let written = raw.edit("100 * (AVAL - BASE) / BASE", expression);
                let run = PreparedRun::prepare(prepare(written.as_bytes())).unwrap();
                assert!(matches!(
                    run.execute_csv(b"A\n\"bad"),
                    Err(Error::Source(_))
                ));
                let error = run
                    .execute_csv(&std::fs::read(case.join("input/lb.csv")).unwrap())
                    .err()
                    .unwrap();
                let Error::Bind(BindError::Invalid(findings)) = &error else {
                    panic!("{error:?}")
                };
                let path = match findings.as_slice() {
                    [BindFinding::Numeric {
                        path,
                        expression: actual,
                        ..
                    }] if !qualified => {
                        assert_eq!(actual, expression);
                        path
                    }
                    [BindFinding::QualifiedNumericReference {
                        path,
                        expression: actual,
                        ..
                    }] if qualified => {
                        assert_eq!(actual, expression);
                        path
                    }
                    _ => panic!("wrong phase or condition: {error:?}"),
                };
                assert_eq!(path, "columns.PCHG.derivation");
            }
        }

        /// Already data: the authored findings of `fixtures/preflight.tsv`.
        #[test]
        fn core_preflight_matches_independent_complete_findings() {
            use yamaa_adapters::specification_diagnostics;
            use yamaa_application::specification_run::PreparedRun;
            use yamaa_core::json::Value;
            let mut count = 0;
            for row in include_str!("fixtures/preflight.tsv").lines().skip(1) {
                let fields = row.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 3);
                let error = PreparedRun::prepare(prepare(fields[1].as_bytes())).unwrap_err();
                let expected: Value = yamaa_adapters::json_wire::decode(fields[2]).unwrap();
                assert_eq!(expected["outcome"]["status"], "invalid", "{}", fields[0]);
                assert_eq!(
                    crate::support::findings(specification_diagnostics::failure(&error, None)),
                    expected["outcome"]["diagnostics"],
                    "{}",
                    fields[0]
                );
                count += 1;
            }
            assert_eq!(count, 5);
        }

        /// Already data: the authored reports of `fixtures/output_declarations.tsv`.
        #[test]
        fn core_output_declarations_match_independent_complete_failed_reports() {
            independent_failed_reports(
                include_str!("fixtures/output_declarations.tsv"),
                "output",
                (5, 8),
                b"ID\n1\n",
            );
        }

        /// Already data: the authored reports of `fixtures/grammar_diagnostics.tsv`.
        #[test]
        fn core_grammar_matches_independent_complete_failed_reports() {
            independent_failed_reports(
                include_str!("fixtures/grammar_diagnostics.tsv"),
                "grammar",
                (7, 7),
                b"ID\n1\n",
            );
        }

        /// Already data: the authored reports of `fixtures/binding_diagnostics.tsv`.
        #[test]
        fn core_binding_matches_independent_complete_failed_reports() {
            independent_failed_reports(
                include_str!("fixtures/binding_diagnostics.tsv"),
                "binding",
                (10, 11),
                b"ID,V\n1,2\n",
            );
        }

        /// Already data: the authored reports of the adapters'
        /// `fixtures/csv_profile_diagnostics.json`, whose codec projections
        /// their own suite compares.
        #[test]
        fn core_csv_profile_matches_independent_complete_failed_reports() {
            independent_failed_reports(
                include_str!("../../yamaa-adapters/tests/fixtures/csv_profile_diagnostics.json"),
                "csv",
                (13, 13),
                b"",
            );
        }

        /// Already data: the authored findings of `fixtures/window_diagnostics.tsv`.
        #[test]
        fn core_window_findings_match_independent_complete_failed_reports() {
            independent_failed_reports(
                include_str!("fixtures/window_diagnostics.tsv"),
                "window",
                (2, 2),
                b"ID,V\n1,2\n",
            );
        }

        /// Already data: the authored reports of `fixtures/predicate_diagnostics.tsv`.
        #[test]
        fn core_predicate_matches_independent_complete_failed_report() {
            independent_failed_reports(
                include_str!("fixtures/predicate_diagnostics.tsv"),
                "predicate",
                (1, 1),
                b"ID,V\n1,2\n",
            );
        }

        /// Already data: the authored reports of the adapters'
        /// `fixtures/source_typing.json`, whose codec projections their own
        /// suite compares.
        #[test]
        fn core_source_typing_matches_independent_complete_failed_reports() {
            independent_failed_reports(
                include_str!("../../yamaa-adapters/tests/fixtures/source_typing.json"),
                "typing",
                (4, 4),
                b"",
            );
        }

        /// Seam: a scripted resolver port's request and read counts, snapshot identity and the
        /// `PortError` variants of a failed capture.
        #[test]
        fn actual_capture_observations_survive_errors_and_distinguish_cached_reads() {
            use yamaa_application::specification_run::{Error, PortError, PreparedRun};
            use yamaa_core::specification_run::RunError;
            struct Port {
                content: Arc<[u8]>,
                reads: usize,
                requests: usize,
                fail: bool,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Port {
                type Error = &'static str;
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Ok(resolver::Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    assert_eq!((&*source.name, &*source.path), ("LB", "input/lb.csv"));
                    self.requests += 1;
                    if self.fail {
                        return Err("resource_path_missing");
                    }
                    // The first request takes the snapshot; later ones reuse it.
                    let created = self.requests == 1;
                    self.reads += usize::from(created);
                    Ok(resolver::Captured::resource(
                        Arc::clone(&self.content),
                        created,
                    ))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &resolver::Located,
                    _: &resolver::Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    panic!("an input is never rebased")
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/negative-zero-division");
            let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
            let run = PreparedRun::prepare(prepare(raw.as_bytes())).unwrap();
            let content: Arc<[u8]> = std::fs::read(case.join("input/lb.csv")).unwrap().into();
            let mut port = Port {
                content: Arc::clone(&content),
                reads: 0,
                requests: 0,
                fail: false,
            };
            for created in [1, 0] {
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                let sources = &attempt.dataset.sources;
                assert!(sources[0].read.captured);
                assert_eq!(sources[0].read.snapshots_created, Some(created));
                assert!(Arc::ptr_eq(sources[0].snapshot.as_ref().unwrap(), &content));
                assert_eq!(sources[0].table.as_ref().unwrap().row_count(), 2);
                let execution = attempt.dataset.result.as_ref().unwrap();
                assert!(execution.result.is_err());
                assert!(matches!(
                    yamaa_adapters::dataset_result::outcome(execution).unwrap(),
                    yamaa_adapters::dataset_result::Outcome::Condition(found)
                        if found.diagnostic.condition() == "division_by_zero"
                ));
            }
            assert_eq!(port.requests, 2);
            let invalid = raw.edit("100 * (AVAL - BASE) / BASE", "AVAL +");
            let run = PreparedRun::prepare(prepare(invalid.as_bytes())).unwrap();
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
            assert!(attempt.dataset.sources[0].table.is_some());
            assert!(matches!(
                attempt.dataset.result,
                Err(PortError::Run(RunError::Bind(_)))
            ));
            port.content = Arc::from(b"A\n\"bad".as_slice());
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
            assert!(attempt.dataset.sources[0].snapshot.is_some());
            assert!(attempt.dataset.sources[0].table.is_none());
            assert!(matches!(
                &attempt.dataset.result,
                Err(PortError::Run(RunError::Sources(errors)))
                    if matches!(errors[..], [(0, Error::Source(_))])
            ));
            port.fail = true;
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
            let sources = &attempt.dataset.sources;
            assert!(!sources[0].read.captured);
            assert_eq!(sources[0].read.snapshots_created, Some(0));
            assert!(sources[0].snapshot.is_none() && sources[0].table.is_none());
            assert!(matches!(
                attempt.dataset.result,
                Err(PortError::Capture("resource_path_missing"))
            ));
        }

        /// Already data: the authored observation records of `fixtures/specs/<benchmark>.json`,
        /// read through a capturing port that counts its reads.
        #[test]
        fn whole_failure_reports_match_reference_observations_from_actual_capture() {
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            struct Files {
                base: std::path::PathBuf,
                reads: usize,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Files {
                type Error = std::io::Error;
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Ok(resolver::Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    std::fs::File::open(self.base.join(&source.path))?
                        .take((yamaa_core::limits::Limits::SOURCE_BYTES + 1) as u64)
                        .read_to_end(&mut bytes)?;
                    self.reads += 1;
                    Ok(resolver::Captured::resource(bytes.into(), true))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &resolver::Located,
                    _: &resolver::Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    panic!("an input is never rebased")
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            for name in [
                "negative-zero-division",
                "negative-integer-overflow",
                "negative-formula-flag",
                "negative-row-aggregate",
                "negative-row-no-prior",
                "negative-source-missing-field",
                "negative-source-trivial-filter",
                "negative-paired-dates",
                "negative-not-missing-age",
                "negative-implausible-age",
                "negative-invalid-sex",
            ] {
                let case = root.join("benchmarks").join(name);
                let run =
                    PreparedRun::prepare(prepare(&std::fs::read(case.join("spec.yaml")).unwrap()))
                        .unwrap();
                let mut port = Files {
                    base: case,
                    reads: 0,
                };
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                assert_eq!(port.reads, 1);
                // Independently authored complete failure truth; benchmark source and
                // diagnostic declarations also receive a separate reference comparison.
                let mut expected: yamaa_core::json::Value =
                    yamaa_adapters::json_wire::decode_slice(
                        &std::fs::read(
                            Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("tests/fixtures/specs")
                                .join(format!("{name}.json")),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let actual = specification_report::failure(
                    &run,
                    &attempt,
                    Identity {
                        runtime: "python",
                        runtime_version: expected["runtime_version"].as_str().unwrap(),
                        engine_version: expected["engine_version"].as_str().unwrap(),
                        example: name,
                        specification: "spec.yaml",
                        base_directory: ".",
                    },
                )
                .unwrap();
                expected["backend"] = yamaa_adapters::json!("rust");
                assert_eq!(actual.observations_value(false), expected, "{name}");
            }
        }

        /// Seam: the prepared run's declared source types (`run.source().types`), beside the two
        /// issue rows its runs report.
        #[test]
        fn declared_source_types_preserve_ingestion_diagnostics_before_formula_errors() {
            use yamaa_adapters::json;
            use yamaa_application::specification_run::PreparedRun;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/negative-zero-division");
            let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
            let written = raw
                .edit(
                    "LB: input/lb.csv",
                    "LB: {path: input/lb.csv, types: {LBSTRESN: int}}",
                )
                .edit("100 * (AVAL - BASE) / BASE", "AVAL +");
            assert_ne!(written, raw);
            let run = PreparedRun::prepare(prepare(written.as_bytes())).unwrap();
            assert_eq!(run.source().types.len(), 1);
            let error = run
                .execute_csv(b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,42.5,0")
                .err()
                .unwrap();
            assert_eq!(
                spec_findings(&error, Some(run.source())).unwrap(),
                vec![
                    json!({"phase":"ingest","condition":"field_parse_failed","rule":"INPUT-7","spec_paths":["input.LB.types.LBSTRESN"],"context":{"dataset":"LB","field":"LBSTRESN","type":"int","value":"42.5"}})
                ]
            );
            let written = written.edit("{LBSTRESN: int}", "{LBSTRESN: int, ZZ: float, AA: int}");
            let run = PreparedRun::prepare(prepare(written.as_bytes())).unwrap();
            let error = run
                .execute_csv(b"STUDYID,USUBJID,LBTESTCD,LBSTRESN,LBBLRESN\nS,P,ALT,42.5,0")
                .err()
                .unwrap();
            assert_eq!(
                spec_findings(&error, Some(run.source())).unwrap(),
                vec![
                    json!({"phase":"validation","condition":"unknown_field","rule":"INPUT-6","spec_paths":["input.LB.types.ZZ"],"context":{"dataset":"LB","field":"ZZ","identifier":"ZZ"}})
                ]
            );
        }

        /// Seam: the core plan's typed rows and exact float bits, against the benchmark's
        /// `expected/adlb.csv`.
        #[test]
        fn original_ordered_sum_yaml_executes_all_seventeen_rows_with_exact_float_bits() {
            use yamaa_adapters::typed_csv;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/adam-adlb-ordered-sum");
            let document = prepare(&std::fs::read(case.join("spec.yaml")).unwrap());
            let compiled = compile(&document).unwrap();
            let source = typed_csv::parse(
                &std::fs::read(case.join("input/lb.csv")).unwrap(),
                &compiled.source().types,
            )
            .unwrap();
            let plan = compiled.bind(source.schema()).unwrap();
            let result = plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                    &|| false,
                )
                .result
                .unwrap();
            assert_eq!(result.dataset.rows().len(), 17);
            let projection = compiled
                .projection()
                .iter()
                .map(|name| {
                    result
                        .dataset
                        .schema()
                        .columns()
                        .iter()
                        .position(|c| &c.name == name)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let bytes =
                yamaa_adapters::csv_artifact::render(&result.dataset, &projection, None).unwrap();
            assert_eq!(
                bytes,
                std::fs::read(case.join("expected/adlb.csv")).unwrap()
            );
            let expected = typed_csv::parse(
                &std::fs::read(case.join("expected/adlb.csv")).unwrap(),
                &[("AVAL".into(), yamaa_core::value::ColumnType::Float)],
            )
            .unwrap();
            for (column, name) in compiled.projection().iter().enumerate() {
                assert_eq!(&expected.schema().columns()[column].name, name);
                let actual_column = result
                    .dataset
                    .schema()
                    .columns()
                    .iter()
                    .position(|c| &c.name == name)
                    .unwrap();
                assert_eq!(
                    result.dataset.schema().columns()[actual_column].kind,
                    expected.schema().columns()[column].kind
                );
                for row in 0..17 {
                    assert_eq!(
                        result.dataset.cell(row, actual_column).unwrap(),
                        expected.cell(row, column).unwrap(),
                        "row {row} column {name}"
                    );
                }
            }
            let values = result
                .dataset
                .schema()
                .columns()
                .iter()
                .position(|c| c.name == "AVAL")
                .unwrap();
            let Value::Float(first) = result.dataset.rows()[12][values] else {
                panic!()
            };
            let Value::Float(second) = result.dataset.rows()[13][values] else {
                panic!()
            };
            assert_eq!(first.get().to_bits(), 0.6000000000000001f64.to_bits());
            assert_eq!(second.get().to_bits(), 0.6f64.to_bits());
            assert_eq!(result.dataset.rows()[16][values], Value::Missing);
            assert_eq!(result.verifications.len(), 1);
            assert!(result.verifications.iter().all(|v| v.failed_count == 0));
        }

        /// Seam: the observation record a build returns (`saved_observations`), against
        /// `fixtures/specs/adam-adlb-ordered-sum.json`.
        #[test]
        fn original_success_report_describes_its_saved_file_and_output_failures_save_nothing() {
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            struct Captured {
                bytes: Arc<[u8]>,
                reads: usize,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Captured {
                type Error = std::convert::Infallible;
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Ok(resolver::Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    let before = self.reads;
                    assert_eq!(source.path, "input/lb.csv");
                    self.reads = 1;
                    Ok(resolver::Captured::resource(
                        self.bytes.clone(),
                        self.reads != before,
                    ))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &resolver::Located,
                    _: &resolver::Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    panic!("an input is never rebased")
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/adam-adlb-ordered-sum");
            let raw = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
            let mut port = Captured {
                bytes: std::fs::read(case.join("input/lb.csv")).unwrap().into(),
                reads: 0,
            };
            let id = || Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "adam-adlb-ordered-sum",
                specification: "spec.yaml",
                base_directory: ".",
            };
            let run = PreparedRun::prepare(prepare(raw.as_bytes())).unwrap();
            let expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/specs/adam-adlb-ordered-sum.json"
            ))
            .unwrap();
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
            let result = specification_report::build_result(&run, &attempt, id(), &[]).unwrap();
            let artifact = std::fs::read(case.join("expected/adlb.csv")).unwrap();
            assert_eq!(result.files(), [("adlb.csv", &artifact[..])]);
            assert_eq!(result.saved_observations().unwrap(), expected);
            // An output failure has no file to save.
            let saved = |run: &PreparedRun, port: &mut Captured| {
                let attempt = run.execute_with_ports(&mut NoFunctions, port, Scope::default());
                let result = specification_report::build_result(run, &attempt, id(), &[]).unwrap();
                assert!(result.files().is_empty());
                result.saved_observations().unwrap()
            };
            let run = PreparedRun::prepare(prepare(
                raw.edit("path: adlb.csv", "path: adlb.bad").as_bytes(),
            ))
            .unwrap();
            let report = saved(&run, &mut port);
            let mut expected = expected;
            expected["outcome"] = json!("failure");
            expected["artifacts"] = json!([]);
            expected["tables"].as_array_mut().unwrap().pop();
            expected["nodes"][0]["outcome"] = json!("failure");
            expected["diagnostics"] = json!([{"phase":"validation","condition":"unknown_artifact_profile","rule":"OUTPUT-5","spec_paths":["output.path"],"context":{"path":"adlb.bad","permitted":[".csv",".parquet"]}}]);
            expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
            expected["source_reads"][0]["snapshots_created"] = json!(0);
            assert_eq!(report, expected);

            let run = PreparedRun::prepare(prepare(
                raw.edit("min: 17\n      max: 17", "min: 18\n      max: 18")
                    .as_bytes(),
            ))
            .unwrap();
            let actual = saved(&run, &mut port);
            let diagnostic = json!({"phase":"verification","condition":"row_count_failed","rule":"CHECK-13","spec_paths":["verifications[0].row_count"],"context":{"count":17,"failure_count":1,"keys":[{}]}});
            expected["diagnostics"] = json!([diagnostic.clone()]);
            expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
            let mut failure = diagnostic;
            failure["severity"] = json!("error");
            failure["offending_keys"] = json!([{}]);
            failure["log_context"] = failure["context"].clone();
            expected["verifications"][0]["failure"] = failure;
            assert_eq!(actual, expected);
            // All checks execute despite an earlier data failure; output diagnostics
            // remain later than verification and no derived/accepted artifact is exposed.
            // A `unique` over a non-key column comes first and fails on repeats.
            let written = raw
                .edit(
                    "verifications:\n",
                    "verifications:\n  - unique: [USUBJID]\n",
                )
                .edit("path: adlb.csv", "path: adlb.bad");
            let run = PreparedRun::prepare(prepare(written.as_bytes())).unwrap();
            let actual = saved(&run, &mut port);
            let original: Json = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/specs/adam-adlb-ordered-sum.json"
            ))
            .unwrap();
            let rows = original["tables"][1]["rows"].as_array().unwrap();
            let keys=[0,1,2,12,3,4,5,13,6,14,7,8,9,15,10,11,16].into_iter().map(|row|json!({"STUDYID":rows[row][0]["value"],"USUBJID":rows[row][1]["value"],"PARAMCD":rows[row][3]["value"],"AVISIT":rows[row][2]["value"]})).collect::<Vec<_>>();
            let diagnostic = json!({"phase":"verification","condition":"unique_failed","rule":"CHECK-10","spec_paths":["verifications[0].unique"],"context":{"columns":["USUBJID"],"failure_count":5,"keys":&keys[..5]}});
            expected["diagnostics"] = json!([diagnostic.clone()]);
            expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
            let mut failure = diagnostic;
            failure["severity"] = json!("error");
            failure["offending_keys"] = json!(keys);
            failure["log_context"] = failure["context"].clone();
            failure["log_context"]["keys"] = failure["offending_keys"].clone();
            let mut count = expected["verifications"][0].clone();
            count["spec_path"] = json!("verifications[1].row_count");
            count["failure"] = Json::Null;
            let mut unique = count.clone();
            unique["check"] = json!("unique");
            unique["rule"] = json!("CHECK-10");
            unique["spec_path"] = json!("verifications[0].unique");
            unique["evaluated_count"] = json!(5);
            unique["failure"] = failure;
            expected["verifications"] = json!([unique, count]);
            assert_eq!(actual, expected);
        }

        /// Seam: the observation record's verification records, each failure's context and log
        /// context, and the scripted port's read count, which `report.check` rows do not state
        /// whole.
        #[test]
        fn verification_declarations_preserve_completed_prefix_and_phase_precedence() {
            use crate::support::ports::Sources;
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/adam-adlb-ordered-sum");
            // The benchmark as 1.0 writes it, since the test edits its text
            // (#3422); its input is the benchmark's own.
            let raw = crate::support::benchmarks::converted("adam-adlb-ordered-sum");
            let prefix = raw.split("verifications:").next().unwrap();
            let bytes = std::fs::read(case.join("input/lb.csv")).unwrap();
            let id = || Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "adam-adlb-ordered-sum",
                specification: "spec.yaml",
                base_directory: ".",
            };
            // Each valid first check records its identity and failure before the later
            // declaration aborts. Earlier data failures are kept in the ledger, while
            // the later declaration is the execution diagnostic (CHECK-22).
            for (declaration, condition, rule, suffix, reason) in [
                (
                    "unique: []",
                    "invalid_declaration",
                    "CHECK-4",
                    "unique",
                    "columns names at least one column",
                ),
                (
                    "unique: [ABSENT]",
                    "unknown_field",
                    "CHECK-4",
                    "unique.columns",
                    "unknown column 'ABSENT'",
                ),
                (
                    "row_count: {}",
                    "invalid_declaration",
                    "CHECK-4",
                    "row_count",
                    "row_count requires one bound",
                ),
                (
                    "row_count: {min: 19, max: 18}",
                    "invalid_declaration",
                    "CHECK-4",
                    "row_count",
                    "row_count min exceeds max",
                ),
                (
                    "row_count: {min: 999999999999999999999, max: 999999999999999999998}",
                    "invalid_declaration",
                    "CHECK-4",
                    "row_count",
                    "row_count min exceeds max",
                ),
                (
                    "row_count: {id: \"a'b\", min: 0}",
                    "duplicate_identifier",
                    "CHECK-3",
                    "row_count",
                    "verification id \"a'b\" repeats verifications[0].row_count",
                ),
            ] {
                for minimum in [0, 18] {
                    let written = format!("{prefix}verifications:\n  - row_count: {{id: \"a'b\", min: {minimum}}}\n  - {declaration}\n  - row_count: {{min: 0}}\n");
                    let run = PreparedRun::prepare(prepare(written.as_bytes())).unwrap();
                    let mut host = Sources::any(&bytes);
                    let attempt =
                        run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
                    let result =
                        specification_report::build_result(&run, &attempt, id(), &[]).unwrap();
                    // An invalid verification has no file to save.
                    assert!(result.files().is_empty());
                    let actual = result.saved_observations().unwrap();
                    assert_eq!(host.reads(), 1);
                    assert_eq!(actual["outcome"], "failure");
                    assert_eq!(actual["artifacts"], json!([]));
                    assert_eq!(actual["tables"].as_array().unwrap().len(), 1);
                    assert_eq!(actual["verifications"].as_array().unwrap().len(), 1);
                    let record = &actual["verifications"][0];
                    assert_eq!(record["verification_id"], "a'b");
                    assert_eq!(record["evaluated_count"], 1);
                    if minimum == 0 {
                        assert_eq!(record["failure"], Json::Null);
                    } else {
                        assert_eq!(record["failure"]["condition"], "row_count_failed");
                        assert_eq!(record["failure"]["context"]["verification_id"], "a'b");
                        assert_eq!(record["failure"]["log_context"]["verification_id"], "a'b");
                    }
                    // An unknown column and a repeated id also name the identifier.
                    let mut context = json!({"reason": reason});
                    match condition {
                        "unknown_field" => context["identifier"] = json!("ABSENT"),
                        "duplicate_identifier" => context["identifier"] = json!("a'b"),
                        _ => {}
                    }
                    assert_eq!(
                        actual["diagnostics"],
                        json!([{"phase":"validation","condition":condition,"rule":rule,"spec_paths":[format!("verifications[1].{suffix}")],"context":context}])
                    );
                }
            }
            let written = format!("{prefix}verifications:\n  - row_count: {{}}\n");
            // Derivation grammar is deferred until ingestion but is still before checks.
            let invalid_formula = written.edit("SUM(LB.LBSTRESN)", "SUM(");
            let run = PreparedRun::prepare(prepare(invalid_formula.as_bytes())).unwrap();
            let mut host = Sources::any(&bytes);
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
            let actual = specification_report::build_result(&run, &attempt, id(), &[])
                .unwrap()
                .saved_observations()
                .unwrap();
            assert_eq!(actual["verifications"], json!([]));
            assert_eq!(
                actual["diagnostics"][0]["spec_paths"],
                json!(["rows.total.derivations.AVAL.aggregate"])
            );
            // A missing output key is reached after derivation and before declaration checks.
            let run = PreparedRun::prepare(prepare(
                written
                    .edit("derivation: LB.STUDYID", "derivation: null")
                    .as_bytes(),
            ))
            .unwrap();
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
            let actual = specification_report::build_result(&run, &attempt, id(), &[])
                .unwrap()
                .saved_observations()
                .unwrap();
            assert_eq!(actual["verifications"], json!([]));
            assert_eq!(actual["diagnostics"][0]["condition"], "missing_key");
            // Report admission scans all declarations before granting study ports,
            // even when an earlier payload finding remains deferred.
            let missing_report = format!("{written}  - row_count: {{min: 1, severity: warning}}\n");
            assert!(matches!(
                PreparedRun::prepare(prepare(missing_report.as_bytes())),
                Err(yamaa_application::specification_run::Error::Prepare(
                    yamaa_core::specification::PrepareError::Invalid(findings)
                )) if findings.iter().any(|finding| finding.diagnostic().definition().condition == "missing_report")
            ));
        }

        /// Seam: the observation record a build returns and the scripted port's read count.
        #[test]
        fn original_sum_reports_nonnumeric_values_but_publishes_all_missing_groups() {
            use crate::support::ports::Sources;
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/adam-adlb-ordered-sum");
            // The benchmark as 1.0 writes it, since the test edits its text
            // (#3422); its input and expected output are the benchmark's own.
            let raw = crate::support::benchmarks::converted("adam-adlb-ordered-sum")
                .edit("LBSTRESN: float", "LBSTRESN: str");
            let input = std::fs::read_to_string(case.join("input/lb.csv")).unwrap();
            let id = || Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "adam-adlb-ordered-sum",
                specification: "spec.yaml",
                base_directory: ".",
            };
            for all_missing in [false, true] {
                let mut expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                    "fixtures/specs/adam-adlb-ordered-sum.json"
                ))
                .unwrap();
                expected["tables"][0]["types"][6] = json!("str");
                let mut source = String::new();
                for (i, line) in input.lines().enumerate() {
                    if i == 0 {
                        source.push_str(line);
                        source.push('\n');
                        continue;
                    }
                    let (fields, value) = line.rsplit_once(',').unwrap();
                    let value = if all_missing { "" } else { value };
                    source.push_str(&format!("{fields},{value}\n"));
                    expected["tables"][0]["rows"][i - 1][6] = if value.is_empty() {
                        json!({"type":"missing","value":null})
                    } else {
                        json!({"type":"str","value":value})
                    };
                }
                let mut host = Sources::any(source.as_bytes());
                let run = PreparedRun::prepare(prepare(raw.as_bytes())).unwrap();
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
                let result = specification_report::build_result(&run, &attempt, id(), &[]).unwrap();
                let actual = result.saved_observations().unwrap();
                if all_missing {
                    for row in expected["tables"][1]["rows"].as_array_mut().unwrap() {
                        row[5] = json!({"type":"missing","value":null});
                    }
                    let original = std::fs::read_to_string(case.join("expected/adlb.csv")).unwrap();
                    let records = original
                        .lines()
                        .enumerate()
                        .map(|(i, line)| {
                            if i == 0 {
                                line.to_owned()
                            } else {
                                let mut cells = line.split(',').collect::<Vec<_>>();
                                cells[5] = "";
                                cells.join(",")
                            }
                        })
                        .collect::<Vec<_>>();
                    let content = format!("{}\n", records.join("\n"));
                    expected["artifacts"][0]["records"] = json!(records);
                    expected["artifacts"][0]["content"] = json!(content);
                    expected["artifacts"][0]["byte_length"] = json!(content.len());
                    assert_eq!(result.files(), [("adlb.csv", content.as_bytes())]);
                } else {
                    expected["outcome"] = json!("failure");
                    expected["artifacts"] = json!([]);
                    expected["verifications"] = json!([]);
                    expected["tables"].as_array_mut().unwrap().pop();
                    expected["diagnostics"] = json!([{"phase":"validation","condition":"incompatible_input_type","rule":"FORMULA-23","spec_paths":["rows.total.derivations.AVAL.aggregate"],"context":{"expr":"SUM(LB.LBSTRESN)","reducer":"SUM","source":"LB.LBSTRESN","expected":"numeric","actual":"str"}}]);
                    expected["nodes"][0]["outcome"] = json!("failure");
                    expected["nodes"][0]["diagnostics"] = expected["diagnostics"].clone();
                    assert!(result.files().is_empty());
                }
                assert_eq!(host.reads(), 1);
                assert_eq!(actual, expected);
            }
        }

        /// Seam: the observation record a build returns and the scripted port's read count, beside
        /// the findings of its edited specifications.
        #[test]
        fn row_defaults_are_inherited_per_template_and_coverage_errors_precede_io() {
            use crate::support::ports::Sources;
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/adam-adlb-ordered-sum");
            // The benchmark as 1.0 writes it, since the test edits its text
            // (#3422); its input is the benchmark's own.
            let raw = crate::support::benchmarks::converted("adam-adlb-ordered-sum");
            let written = raw
                .edit(
                    "label: Parameter\n",
                    "label: Parameter\n    derivation: LB.LBTEST\n",
                )
                .edit("      PARAM: LB.LBTEST\n", "")
                .edit(
                    "label: Derivation Type\n",
                    "label: Derivation Type\n    derivation: null\n",
                )
                .edit("      DTYPE: null\n", "");
            let bytes = std::fs::read(case.join("input/lb.csv")).unwrap();
            let mut host = Sources::any(&bytes);
            let run = PreparedRun::prepare(prepare(written.as_bytes())).unwrap();
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
            let result = specification_report::build_result(
                &run,
                &attempt,
                Identity {
                    runtime: "python",
                    runtime_version: "fixture-runtime",
                    engine_version: "fixture-engine",
                    example: "adam-adlb-ordered-sum",
                    specification: "spec.yaml",
                    base_directory: ".",
                },
                &[],
            )
            .unwrap();
            let actual = result.saved_observations().unwrap();
            let expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/specs/adam-adlb-ordered-sum.json"
            ))
            .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(host.reads(), 1);
            let artifact = std::fs::read(case.join("expected/adlb.csv")).unwrap();
            assert_eq!(result.files(), [("adlb.csv", &artifact[..])]);
            // The inherited declaration retains its column path, although it executes
            // at row construction for the template without an override.
            let bad = written.edit("derivation: LB.LBTEST", "derivation: LB.ABSENT");
            let run = PreparedRun::prepare(prepare(bad.as_bytes())).unwrap();
            let error = run.execute_csv(&bytes).err().unwrap();
            assert_eq!(
                spec_findings(&error, Some(run.source())).unwrap(),
                vec![
                    json!({"phase":"validation","condition":"unknown_field","rule":"NAME-3","spec_paths":["columns.PARAM.derivation.source"],"context":{"identifier":"LB.ABSENT"}})
                ]
            );
            let bad = raw.edit(
                "label: Analysis Value\n",
                "label: Analysis Value\n    derivation: {aggregate: 'SUM(LB.LBSTRESN)'}\n",
            );
            let error = PreparedRun::prepare(prepare(bad.as_bytes()))
                .expect_err("dataset aggregate is not a row default");
            assert_eq!(
                spec_findings(&error, None).unwrap(),
                vec![
                    json!({"phase":"validation","condition":"duplicate_derivation","rule":"SPEC-6","spec_paths":["columns.AVAL.derivation"],"context":{"column":"AVAL","identifier":"AVAL","rows":["collected","total"],"not_row_local":"aggregate"}})
                ]
            );
            // Missing coverage is reported in declared column order, then the
            // independently conflicting root filter. The ABSENT row derivation
            // names an undeclared column, so pruning (LAYER-11) discards it
            // with the dead declaration instead of reporting it.
            let bad = raw
                .edit("domain: ADLB", "domain: ADLB\nfilter: 'TRUE'")
                .edit("      PARAM: LB.LBTEST\n", "")
                .replacen(
                    "    derivations:\n",
                    "    derivations:\n      ABSENT: \"'ignored'\"\n",
                    1,
                );
            let error = PreparedRun::prepare(prepare(bad.as_bytes()))
                .expect_err("preflight without source");
            assert_eq!(
                spec_findings(&error, None).unwrap(),
                vec![
                    json!({"phase":"validation","condition":"missing_derivation","rule":"SPEC-5","spec_paths":["columns.PARAM.derivation"],"context":{"column":"PARAM","rows":["collected"]}}),
                    json!({"phase":"validation","condition":"conflicting_row_construction","rule":"ROW-9","spec_paths":["filter","rows"],"context":{"rows":["collected","total"]}}),
                ]
            );
        }

        /// Seam: the core plan's binding of several schemas (`SourceCount`, `bind_sources`) and its
        /// handler counts.
        #[test]
        fn original_lookup_compiles_multiple_schemas_and_executes_named_selections() {
            use yamaa_core::specification::BindError;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-lookup");
            let original = std::fs::read_to_string(case.join("spec.yaml")).unwrap();
            // Driver position never changes authored capture order or secondary indices.
            for raw in [
                original.clone(),
                original.edit(
                    "  DM: input/dm.csv\n  AE: input/ae.csv",
                    "  AE: input/ae.csv\n  DM: input/dm.csv",
                ),
            ] {
                let document = prepare(raw.as_bytes());
                let prepared = compile(&document).unwrap();
                assert_eq!(prepared.sources().len(), 3);
                assert_eq!(prepared.source().name, "DM");
                let tables = prepared
                    .sources()
                    .iter()
                    .map(|source| {
                        csv_source::parse_text_table(
                            &std::fs::read(case.join(&source.path)).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>();
                assert!(matches!(
                    prepared.bind(tables[prepared.driver_index()].schema()),
                    Err(BindError::SourceCount)
                ));
                let schemas = tables.iter().map(TableAccess::schema).collect::<Vec<_>>();
                let plan = prepared.bind_sources(&schemas).unwrap();
                let secondary = tables
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != prepared.driver_index())
                    .map(|(_, table)| table as &dyn TableAccess<Error = _>)
                    .collect::<Vec<_>>();
                for _ in 0..2 {
                    let attempt = plan.execute(
                        &tables[prepared.driver_index()],
                        &secondary,
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                        &|| false,
                    );
                    let execution = attempt.result.unwrap();
                    let bytes = yamaa_adapters::csv_artifact::render(
                        &execution.dataset,
                        &[0, 1, 2, 3],
                        None,
                    )
                    .unwrap();
                    assert_eq!(
                        bytes,
                        std::fs::read(case.join("expected/adsl.csv")).unwrap()
                    );
                    assert_eq!(bytes.len(), 138);
                    assert!(execution.verifications.is_empty());
                    let actual = attempt
                        .handler_counts
                        .iter()
                        .map(|entry| (entry.spec_path.as_str(), entry.handler.name(), entry.count))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        actual,
                        [
                            (
                                "columns.DTHDY.derivation.source.multiple_matches",
                                "multiple_matches",
                                1
                            ),
                            ("columns.DTHDY.derivation.source.absent", "absent", 2),
                            (
                                "columns.DTHCAUS.derivation.source.multiple_matches",
                                "multiple_matches",
                                1
                            ),
                            ("columns.DTHCAUS.derivation.source.absent", "absent", 2),
                            ("columns.DTHPTERM.derivation.source.absent", "absent", 2),
                        ]
                    );
                }
            }
        }

        /// Seam: the observation record a build returns and a scripted host's capture requests.
        #[test]
        fn original_lookup_complete_report_uses_all_captured_sources_and_actual_handlers() {
            use std::collections::BTreeMap;
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            struct Host {
                content: BTreeMap<String, Arc<[u8]>>,
                seen: Vec<String>,
                reads: usize,
                requests: Vec<String>,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Host {
                type Error = ();
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Ok(resolver::Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.requests.push(source.name.clone());
                    // An input's first capture takes its snapshot; a later one
                    // reuses it.
                    let created = !self.seen.contains(&source.name);
                    if created {
                        self.seen.push(source.name.clone());
                        self.reads += 1;
                    }
                    let bytes = Arc::clone(&self.content[&source.path]);
                    Ok(resolver::Captured::resource(bytes, created))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &resolver::Located,
                    _: &resolver::Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    panic!("an input is never rebased")
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-lookup");
            let prepared =
                PreparedRun::prepare(prepare(&std::fs::read(case.join("spec.yaml")).unwrap()))
                    .unwrap();
            let content = prepared
                .compiled()
                .sources()
                .iter()
                .map(|source| {
                    (
                        source.path.clone(),
                        Arc::from(std::fs::read(case.join(&source.path)).unwrap()),
                    )
                })
                .collect();
            let mut host = Host {
                content,
                seen: Vec::new(),
                reads: 0,
                requests: Vec::new(),
            };
            let artifact = std::fs::read(case.join("expected/adsl.csv")).unwrap();
            for created in [1, 0] {
                let attempt =
                    prepared.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
                assert_eq!(attempt.dataset.sources.len(), 3);
                let id = Identity {
                    runtime: "python",
                    runtime_version: "fixture-runtime",
                    engine_version: "fixture-engine",
                    example: "schema-lookup",
                    specification: "spec.yaml",
                    base_directory: ".",
                };
                let result =
                    specification_report::build_result(&prepared, &attempt, id, &[]).unwrap();
                assert_eq!(result.files(), [("adsl.csv", &artifact[..])]);
                let actual = result.saved_observations().unwrap();
                let mut expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                    "fixtures/specs/schema-lookup.json"
                ))
                .unwrap();
                for source in expected["source_reads"].as_array_mut().unwrap() {
                    source["snapshots_created"] = json!(created);
                }
                assert_eq!(actual, expected);
            }
            assert_eq!(host.requests, ["DM", "AE", "MEDDRA", "DM", "AE", "MEDDRA"]);
            assert_eq!(host.reads, 3);
        }

        /// Seam: the observation record of each authored variant of
        /// `fixtures/specs/lookup-failures.json` and a scripted host's capture requests.
        #[test]
        fn original_lookup_failures_match_independent_diagnostics_and_completed_observations() {
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            struct Host {
                case: std::path::PathBuf,
                requests: Vec<String>,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Host {
                type Error = ();
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Ok(resolver::Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.requests.push(source.name.clone());
                    Ok(resolver::Captured::resource(
                        Arc::from(std::fs::read(self.case.join(&source.path)).unwrap()),
                        true,
                    ))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &resolver::Located,
                    _: &resolver::Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    panic!("an input is never rebased")
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-lookup");
            // The benchmark as 1.0 writes it, since each variant edits its
            // text (#3422); its inputs are the benchmark's own.
            let original = crate::support::benchmarks::converted("schema-lookup");
            let cases: Vec<Json> = yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/specs/lookup-failures.json"
            ))
            .unwrap();
            for variant in cases {
                let raw = original.replace(
                    variant["before"].as_str().unwrap(),
                    variant["after"].as_str().unwrap(),
                );
                let run = PreparedRun::prepare(prepare(raw.as_bytes())).unwrap();
                let mut host = Host {
                    case: case.clone(),
                    requests: vec![],
                };
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
                let result = specification_report::build_result(
                    &run,
                    &attempt,
                    Identity {
                        runtime: "python",
                        runtime_version: "fixture-runtime",
                        engine_version: "fixture-engine",
                        example: "schema-lookup",
                        specification: "spec.yaml",
                        base_directory: ".",
                    },
                    &[],
                )
                .unwrap();
                // A failed lookup has no file to save.
                assert!(result.files().is_empty());
                let actual = result.saved_observations().unwrap();
                let mut expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                    "fixtures/specs/schema-lookup.json"
                ))
                .unwrap();
                expected["outcome"] = json!("failure");
                expected["nodes"][0]["outcome"] = json!("failure");
                for field in ["diagnostics", "handler_counts"] {
                    expected[field] = variant[field].clone();
                    expected["nodes"][0][field] = variant[field].clone();
                }
                expected["artifacts"] = json!([]);
                expected["verifications"] = json!([]);
                expected["tables"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|table| table["stage"] == "source");
                assert_eq!(
                    actual,
                    expected,
                    "{}",
                    yamaa_adapters::json_wire::encode(&variant["name"])
                );
                assert_eq!(host.requests, ["DM", "AE", "MEDDRA"]);
            }
        }

        /// Seam: the internal `Rejection::Fault` of a run given one buffer for three sources.
        #[test]
        fn single_buffer_execution_reports_source_count_without_an_internal_failure() {
            use yamaa_adapters::specification_diagnostics;
            use yamaa_application::specification_run::PreparedRun;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-lookup");
            let run =
                PreparedRun::prepare(prepare(&std::fs::read(case.join("spec.yaml")).unwrap()))
                    .unwrap();
            let error = run
                .execute_csv(&std::fs::read(case.join("input/dm.csv")).unwrap())
                .err()
                .unwrap();
            assert_eq!(
                specification_diagnostics::failure(&error, Some(run.source())),
                specification_diagnostics::Rejection::Fault(
                    yamaa_core::application_issue::rejected(
                        yamaa_core::application_issue::Step::Bind,
                        yamaa_core::application_issue::Fault::SourceCount,
                    )
                )
            );
        }

        /// Seam: the observation record a build returns and a scripted host's read and request
        /// counts.
        #[test]
        fn original_window_yaml_compiles_and_publishes_complete_independent_report() {
            use yamaa_adapters::json;
            use yamaa_application::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            struct Host {
                bytes: Arc<[u8]>,
                reads: usize,
                requests: usize,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Host {
                type Error = ();
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Ok(resolver::Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    let before = self.reads;
                    assert_eq!((&*source.name, &*source.path), ("VS", "input/vs.csv"));
                    self.requests += 1;
                    self.reads = 1;
                    Ok(resolver::Captured::resource(
                        self.bytes.clone(),
                        self.reads != before,
                    ))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &resolver::Located,
                    _: &resolver::Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    panic!("an input is never rebased")
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-window-functions");
            let prepared = prepare(&std::fs::read(case.join("spec.yaml")).unwrap());
            let run = PreparedRun::prepare(prepared).unwrap();
            let mut host = Host {
                bytes: Arc::from(std::fs::read(case.join("input/vs.csv")).unwrap()),
                reads: 0,
                requests: 0,
            };
            let artifact = std::fs::read(case.join("expected/advs.csv")).unwrap();
            for created in [1, 0] {
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
                let result = specification_report::build_result(
                    &run,
                    &attempt,
                    Identity {
                        runtime: "python",
                        runtime_version: "fixture-runtime",
                        engine_version: "fixture-engine",
                        example: "schema-window-functions",
                        specification: "spec.yaml",
                        base_directory: ".",
                    },
                    &[],
                )
                .unwrap();
                assert_eq!(result.files(), [("advs.csv", &artifact[..])]);
                let report = result.saved_observations().unwrap();
                let mut expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                    "fixtures/specs/schema-window-functions.json"
                ))
                .unwrap();
                expected["source_reads"][0]["snapshots_created"] = json!(created);
                assert_eq!(report, expected);
            }
            assert_eq!((host.reads, host.requests), (1, 2));
        }
    }
}
