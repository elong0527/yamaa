#![cfg(feature = "yaml")]
//! Native specification run assertions.
mod execution {
    mod specification_run {
        use crate::support::ports::NoFunctions;
        use crate::support::specification_run::*;
        use crate::support::Edit as _;
        use yamaa_core::application_observer::Scope;

        /// Seam: a scripted capture port's classified failures, payload identity and call log.
        #[test]
        fn classified_capture_failures_retain_complete_reports_without_publication() {
            use yamaa_adapters::json;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::{PortError, PreparedRun},
            };
            use yamaa_core::resource::ResourceFailure;
            struct Payload {
                cause: Option<ResourceFailure>,
                identity: Arc<()>,
            }
            struct Port {
                bytes: Arc<[u8]>,
                calls: Vec<String>,
                fail_at: usize,
                reads: usize,
                cached: bool,
                cause: Option<ResourceFailure>,
                identity: Arc<()>,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Port {
                type Error = Payload;
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
                    let index = self.calls.len();
                    self.calls.push(source.name.clone());
                    if index == self.fail_at {
                        return Err(Payload {
                            cause: self.cause,
                            identity: Arc::clone(&self.identity),
                        });
                    }
                    assert_eq!(source.name, "DM");
                    self.reads += usize::from(!self.cached);
                    Ok(resolver::Captured::resource(
                        Arc::clone(&self.bytes),
                        self.reads != before,
                    ))
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    error: &Self::Error,
                ) -> Option<resolver::Failure> {
                    error.cause.map(resolver::Failure::Resource)
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
            let run =
                PreparedRun::prepare(prepare(&std::fs::read(case.join("spec.yaml")).unwrap()))
                    .unwrap();
            let bytes: Arc<[u8]> = std::fs::read(case.join("input/dm.csv")).unwrap().into();
            let metadata = || Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "schema-lookup",
                specification: "spec.yaml",
                base_directory: ".",
            };
            for (cause, condition) in [
                (ResourceFailure::Missing, "resource_path_missing"),
                (
                    ResourceFailure::NotRegularFile,
                    "resource_path_not_regular_file",
                ),
            ] {
                for fail_at in [0, 1] {
                    for cached in [false, true] {
                        let identity = Arc::new(());
                        let mut port = Port {
                            bytes: Arc::clone(&bytes),
                            calls: vec![],
                            fail_at,
                            reads: 0,
                            cached,
                            cause: Some(cause),
                            identity: Arc::clone(&identity),
                        };
                        let mut attempt =
                            run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                        assert_eq!(
                            port.calls,
                            if fail_at == 0 {
                                vec!["DM"]
                            } else {
                                vec!["DM", "AE"]
                            }
                        );
                        let Err(PortError::Capture(error)) = &attempt.dataset.result else {
                            panic!("capture failure missing")
                        };
                        assert!(Arc::ptr_eq(&identity, &error.identity));
                        if fail_at == 1 {
                            assert!(attempt.dataset.sources[0].table.is_some());
                        }
                        let (dataset, path) = if fail_at == 0 {
                            ("DM", "input/dm.csv")
                        } else {
                            ("AE", "input/ae.csv")
                        };
                        let diagnostics = json!([{"phase":"validation","condition":condition,"rule":"FILE-7","spec_paths":[format!("input.{dataset}.path")],"context":{"dataset":dataset,"path":path}}]);
                        let mut reads = vec![];
                        if fail_at == 1 {
                            reads.push(json!({"base_directory":".","path":"input/dm.csv","outcome":"captured","condition":null,"snapshots_created":usize::from(!cached)}));
                        }
                        reads.push(json!({"base_directory":".","path":path,"outcome":"failure","condition":condition,"snapshots_created":0}));
                        let expected = json!({"report_version":"0.3.0-draft","runtime":"python","backend":"rust","runtime_version":"fixture-runtime","engine_version":"fixture-engine","example":"schema-lookup","outcome":"failure","artifacts":[],"diagnostics":diagnostics,"unsupported":[],"handler_counts":[],"nodes":[{"specification":"spec.yaml","outcome":"failure","diagnostics":diagnostics,"unsupported":[],"handler_counts":[]}],"tables":[],"verifications":[],"callbacks":[],"source_reads":reads,"error":null});
                        let kind = match cause {
                            ResourceFailure::Missing => "missing",
                            ResourceFailure::NotRegularFile => "not_regular_file",
                            ResourceFailure::Boundary(_)
                            | ResourceFailure::UriScheme
                            | ResourceFailure::Backslash
                            | ResourceFailure::EmptySegment
                            | ResourceFailure::ContentChanged
                            | ResourceFailure::Symlink => {
                                unreachable!(
                                    "this retained fixture selects missing and nonregular causes"
                                )
                            }
                        };
                        let row = include_str!("fixtures/source_capture.tsv")
                            .lines()
                            .skip(1)
                            .map(|line| line.split('\t').collect::<Vec<_>>())
                            .find(|row| {
                                row[0] == kind
                                    && row[1] == fail_at.to_string()
                                    && row[2] == usize::from(cached).to_string()
                            })
                            .expect("independent capture truth missing");
                        assert_eq!(expected, yamaa_adapters::json_wire::decode(row[3]).unwrap());
                        let result =
                            specification_report::build_result(&run, &attempt, metadata(), &[])
                                .unwrap();
                        assert_eq!(result.observations(), expected);
                        assert!(result.output().is_none());
                        assert!(result.files().is_empty());
                        // Failed accounting and opaque host errors cannot borrow a known
                        // finding left in the source ledger to become semantic failures.
                        attempt
                            .dataset
                            .sources
                            .last_mut()
                            .unwrap()
                            .read
                            .snapshots_created = None;
                        assert!(specification_report::failure(&run, &attempt, metadata()).is_err());
                        attempt
                            .dataset
                            .sources
                            .last_mut()
                            .unwrap()
                            .read
                            .snapshots_created = Some(0);
                        attempt.dataset.sources.last_mut().unwrap().read.failure = None;
                        assert!(specification_report::failure(&run, &attempt, metadata()).is_err());
                    }
                }
            }
            let identity = Arc::new(());
            let mut port = Port {
                bytes,
                calls: vec![],
                fail_at: 0,
                reads: 0,
                cached: false,
                cause: None,
                identity: Arc::clone(&identity),
            };
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
            let Err(PortError::Capture(error)) = &attempt.dataset.result else {
                panic!("opaque failure missing")
            };
            assert!(Arc::ptr_eq(&identity, &error.identity));
            assert_eq!(attempt.dataset.sources[0].read.failure, None);
            assert!(specification_report::build_result(&run, &attempt, metadata(), &[]).is_err());
        }

        /// Seam: a scripted resolver's rebase of an inherited parent, and the written paths its
        /// findings keep.
        #[test]
        fn resource_findings_use_written_inherited_paths_not_rebased_locations() {
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
                specification_source::Source,
            };
            use yamaa_core::inheritance::Source as FileIdentity;
            use yamaa_core::resource::ResourceFailure;
            struct Files {
                parent: Vec<u8>,
            }
            impl resolver::Resolver for Files {
                type Error = ();
                fn inspect(&mut self, name: resolver::Name<'_>) -> Result<FileIdentity, ()> {
                    let resolver::Name::Written { written, .. } = name else {
                        panic!("unexpected request {name:?}")
                    };
                    assert_eq!(written, "parent.yaml");
                    Ok(FileIdentity {
                        identity: written.into(),
                        display_path: written.into(),
                    })
                }
                fn capture(&mut self, _: resolver::Name<'_>) -> Result<resolver::Captured, ()> {
                    Ok(resolver::Captured::resource(
                        self.parent.clone().into(),
                        true,
                    ))
                }
                fn failure(&self, _: resolver::Name<'_>, _: &()) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    layer: &FileIdentity,
                    _: &FileIdentity,
                    written: &str,
                ) -> Result<String, ()> {
                    Ok(if layer.identity == "parent.yaml" {
                        format!("internal-location/{written}")
                    } else {
                        written.into()
                    })
                }
            }
            struct Missing;
            use yamaa_core::resolver;
            impl resolver::Resolver for Missing {
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
                    let resolver::Name::Input(_) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Err(())
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    _: &Self::Error,
                ) -> Option<resolver::Failure> {
                    Some(resolver::Failure::Resource(ResourceFailure::Missing))
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
            for declaration in [
                "SRC: nested/data.csv",
                "SRC: {path: nested/data.csv, types: {ID: int}}",
            ] {
                for child_path in [None, Some("child.csv")] {
                    let mut entry = String::from("yamaa: '1.0'\ntype: spec\nparent: parent.yaml\ndomain: TEST\nkeys: [ID]\noutput: {path: result.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n");
                    if let Some(path) = child_path {
                        entry.push_str(&format!("input: {{SRC: {path}}}\n"));
                    }
                    let parent = format!("yamaa: '1.0'\ntype: spec\ninput: {{{declaration}}}\n");
                    let document = yamaa_adapters::shipped_schema::prepare(
                        Source {
                            identity: "entry.yaml".into(),
                            bytes: entry.into_bytes(),
                        },
                        "entry.yaml".into(),
                        &mut Files {
                            parent: parent.into_bytes(),
                        },
                    )
                    .unwrap();
                    let written = child_path.unwrap_or("nested/data.csv");
                    assert_eq!(document.written_source_path("SRC"), Some(written));
                    assert_eq!(document.written_source_path("ABSENT"), None);
                    let run = PreparedRun::prepare(document).unwrap();
                    if child_path.is_none() {
                        assert_eq!(run.source().path, "internal-location/nested/data.csv");
                    }
                    let attempt =
                        run.execute_with_ports(&mut NoFunctions, &mut Missing, Scope::default());
                    let report = specification_report::failure(
                        &run,
                        &attempt,
                        Identity {
                            runtime: "r",
                            runtime_version: "test",
                            engine_version: "test",
                            example: "inheritance",
                            specification: "entry.yaml",
                            base_directory: ".",
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        report["diagnostics"],
                        yamaa_adapters::json!([{"phase":"validation","condition":"resource_path_missing","rule":"FILE-7","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":written}}])
                    );
                }
            }
        }

        /// Already data: the authored reports of `fixtures/column_literals.json`.
        #[test]
        fn original_column_literals_match_complete_reference_reports_and_csv() {
            independent_scalar_reports(include_str!("fixtures/column_literals.json"), "literal", 4);
        }

        /// Already data: the authored reports of `fixtures/original_conversion_handlers.json`.
        #[test]
        fn original_conversion_handlers_match_complete_reference_reports_and_csv() {
            independent_scalar_reports(
                include_str!("fixtures/original_conversion_handlers.json"),
                "handler",
                5,
            );
        }

        /// Already data: the authored reports of `fixtures/original_row_conversion_handlers.json`.
        #[test]
        fn original_row_conversion_handlers_match_complete_reference_reports_and_csv() {
            independent_scalar_reports(
                include_str!("fixtures/original_row_conversion_handlers.json"),
                "row-handler",
                8,
            );
        }

        /// Seam: a scripted port's inspection failures and the inputs it inspected, against
        /// `fixtures/source_inspection.tsv`.
        #[test]
        fn metadata_inspection_reports_all_known_failures_before_any_study_read() {
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::{PortError, PreparedRun},
            };
            use yamaa_core::resource::ResourceFailure;
            struct Port {
                failures: yamaa_core::json::Value,
                inspected: Vec<String>,
            }
            use yamaa_core::resolver;
            impl resolver::Resolver for Port {
                type Error = ResourceFailure;
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.inspected.push(source.name.clone());
                    match self.failures[&source.name].as_str() {
                        Some("missing") => Err(ResourceFailure::Missing),
                        Some("not_regular_file") => Err(ResourceFailure::NotRegularFile),
                        None => Ok(resolver::Located {
                            identity: source.path.clone(),
                            display_path: source.path.clone(),
                        }),
                        _ => panic!("unknown independent inspection cause"),
                    }
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(_) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    panic!("metadata failure reached study capture");
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    error: &Self::Error,
                ) -> Option<resolver::Failure> {
                    Some(resolver::Failure::Resource(*error))
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
            let run = PreparedRun::prepare(prepare(
                &std::fs::read(root.join("benchmarks/schema-lookup/spec.yaml")).unwrap(),
            ))
            .unwrap();
            let metadata = || Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: "schema-lookup",
                specification: "spec.yaml",
                base_directory: ".",
            };
            for line in include_str!("fixtures/source_inspection.tsv")
                .lines()
                .skip(1)
            {
                let fields = line.split('\t').collect::<Vec<_>>();
                let mut port = Port {
                    failures: yamaa_adapters::json_wire::decode(fields[1]).unwrap(),
                    inspected: vec![],
                };
                let mut attempt =
                    run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                assert_eq!(port.inspected, ["DM", "AE", "MEDDRA"]);
                assert!(attempt.dataset.sources.is_empty());
                let expected = yamaa_adapters::json_wire::decode(fields[2]).unwrap();
                let result =
                    specification_report::build_result(&run, &attempt, metadata(), &[]).unwrap();
                assert_eq!(result.observations(), expected, "{}", fields[0]);
                assert!(result.output().is_none());
                // An inspection failure has no file to save.
                assert!(result.files().is_empty(), "{}", fields[0]);
                let Err(PortError::Inspect(ref mut failures)) = attempt.dataset.result else {
                    panic!("inspection findings");
                };
                // A cause cannot be attached to a forged declaration, repeated source or
                // opaque host failure merely because the rest of the ledger is valid.
                let original = failures[0].source.clone();
                failures[0].source.path = "forged.csv".into();
                assert!(
                    specification_report::build_result(&run, &attempt, metadata(), &[]).is_err()
                );
                let Err(PortError::Inspect(ref mut failures)) = attempt.dataset.result else {
                    unreachable!();
                };
                failures[0].source = original;
                if failures.len() > 1 {
                    failures.swap(0, 1);
                    assert!(
                        specification_report::build_result(&run, &attempt, metadata(), &[])
                            .is_err()
                    );
                    let Err(PortError::Inspect(ref mut failures)) = attempt.dataset.result else {
                        unreachable!();
                    };
                    failures.swap(0, 1);
                    failures[1].source = failures[0].source.clone();
                    assert!(
                        specification_report::build_result(&run, &attempt, metadata(), &[])
                            .is_err()
                    );
                }
                let Err(PortError::Inspect(ref mut failures)) = attempt.dataset.result else {
                    unreachable!();
                };
                failures[0].failure = None;
                assert!(
                    specification_report::build_result(&run, &attempt, metadata(), &[]).is_err()
                );
            }
        }

        /// Already data: the authored reports of `fixtures/source_filters.json`.
        #[test]
        fn original_source_filters_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/source_filters.json"),
                "source-filter",
                13,
            );
        }

        /// Already data: the authored reports of `fixtures/first_available.json`.
        #[test]
        fn original_first_available_matches_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/first_available.json"),
                "first-available",
                11,
            );
        }

        /// Already data: the authored reports of `fixtures/original_assertions.json`.
        #[test]
        fn original_assertions_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_assertions.json"),
                "assert",
                17,
            );
        }

        /// Already data: the authored reports of `fixtures/original_all_or_none.json`.
        #[test]
        fn original_all_or_none_matches_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_all_or_none.json"),
                "all-or-none",
                11,
            );
        }

        /// Already data: the authored reports of `fixtures/original_not_missing.json`.
        #[test]
        fn original_column_not_missing_matches_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_not_missing.json"),
                "column-check",
                9,
            );
        }

        /// Already data: the authored reports of `fixtures/row_filter_admission.json`.
        #[test]
        fn row_filter_carrier_failures_match_complete_normative_reports() {
            replay_source_selection_truth(
                include_str!("fixtures/row_filter_admission.json"),
                "predicate-carrier",
                2,
            );
        }

        /// Seam: the observation record's verification records, the first check's identity and the
        /// failure's log context, and the scripted port's read count, which `report.check` rows do
        /// not state whole.
        #[test]
        fn exact_wide_count_bounds_preserve_empty_counts_and_completed_check_prefix() {
            use crate::support::ports::Sources;
            use yamaa_adapters::json;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            let prefix = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: source.csv}\noutput: {path: result.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            for input in [b"ID\n".as_slice(), b"ID\n1\n2\n".as_slice()] {
                for bound in [
                    "min: 999999999999999999999999999999",
                    "max: -999999999999999999999999999999",
                ] {
                    let source = format!("{prefix}verifications:\n  - row_count: {{id: first, min: -999999999999999999999999999999, max: 999999999999999999999999999999}}\n  - row_count: {{{bound}}}\n");
                    let run = PreparedRun::prepare(prepare(source.as_bytes())).unwrap();
                    let mut host = Sources::any(input);
                    let attempt =
                        run.execute_with_ports(&mut NoFunctions, &mut host, Scope::default());
                    let result = specification_report::build_result(
                        &run,
                        &attempt,
                        Identity {
                            runtime: "python",
                            runtime_version: "fixture-runtime",
                            engine_version: "fixture-engine",
                            example: "wide-count",
                            specification: "spec.yaml",
                            base_directory: ".",
                        },
                        &[],
                    )
                    .unwrap();
                    let report = result.observations();
                    assert_eq!(host.reads(), 1);
                    assert_eq!(report["outcome"], "failure");
                    assert_eq!(report["artifacts"], json!([]));
                    assert_eq!(report["verifications"].as_array().unwrap().len(), 2);
                    assert_eq!(report["verifications"][0]["verification_id"], "first");
                    assert_eq!(report["verifications"][0]["failure"], json!(null));
                    let count = if input.len() == 3 { 0 } else { 2 };
                    assert_eq!(
                        report["diagnostics"],
                        json!([{"phase":"verification", "condition":"row_count_failed", "rule":"CHECK-13", "spec_paths":["verifications[1].row_count"], "context":{"count":count,"failure_count":1,"keys":[{}]}}])
                    );
                    assert_eq!(
                        report["verifications"][1]["failure"]["log_context"]["count"],
                        count
                    );
                }
            }
        }

        /// Already data: the authored reports of `fixtures/row_filters.json`.
        #[test]
        fn row_filters_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/row_filters.json"),
                "row-filter",
                27,
            );
        }

        /// Already data: the authored reports of `fixtures/row_reductions.json`.
        #[test]
        fn row_reductions_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/row_reductions.json"),
                "row-reduction",
                25,
            );
        }

        /// Already data: the authored reports of `fixtures/correlated_aggregates.json`.
        #[test]
        fn aggregate_relations_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/correlated_aggregates.json"),
                "correlated-aggregate",
                16,
            );
        }

        /// Already data: the authored reports of `fixtures/odm_reads.json`.
        #[test]
        fn odm_reads_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(include_str!("fixtures/odm_reads.json"), "odm", 14);
        }

        /// Already data: the authored reports of `fixtures/odm_deferred.json`.
        #[test]
        fn deferred_odm_defaults_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/odm_deferred.json"),
                "odm-deferred",
                7,
            );
        }

        /// Already data: the authored findings of `fixtures/odm_preflight.json`.
        #[test]
        fn odm_preflight_matches_complete_independent_findings_before_source_authority() {
            let mut count = 0;
            for row in fixture_rows(include_str!("fixtures/odm_preflight.json")) {
                let raw = fixture_bytes(&row["source"]);
                let error = yamaa_adapters::specification_run::PreparedRun::prepare(prepare(&raw))
                    .unwrap_err();
                let expected: yamaa_core::json::Value = row["expected"].clone();
                assert_eq!(expected["outcome"]["status"], "invalid");
                assert_eq!(
                    crate::support::findings(yamaa_adapters::specification_diagnostics::failure(
                        &error, None
                    )),
                    expected["outcome"]["diagnostics"],
                    "{}",
                    row["case"].as_str().unwrap()
                );
                count += 1;
            }
            assert_eq!(count, 8);
        }

        /// Already data: the authored reports of `fixtures/row_column_checks.json`.
        #[test]
        fn row_column_checks_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/row_column_checks.json"),
                "row-column",
                47,
            );
        }

        /// Already data: the authored reports of `fixtures/original_column_values.json`.
        #[test]
        fn original_column_values_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_column_values.json"),
                "column-value",
                20,
            );
        }

        /// Already data: the authored reports of `fixtures/original_column_matches.json`.
        #[test]
        fn original_column_matches_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_column_matches.json"),
                "column-matches",
                16,
            );
        }

        /// Seam: a preparation that admits a specification whose invalid patterns stay deferred,
        /// whose issues no test authored; only its `missing_report` refusal names a row.
        #[test]
        fn deferred_column_findings_keep_vocabulary_and_report_admission() {
            use yamaa_adapters::specification_run::{Error, PreparedRun};
            use yamaa_core::specification::PrepareError;
            let raw = |first: &str, severity: &str| {
                format!(
        "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: unread.csv}}\noutput: {{path: result.csv, columns: [ID, V]}}\ncolumns:\n  - {{name: ID, type: str, label: ID, derivation: SRC.ID}}\n  - name: V\n    type: str\n    label: V\n    derivation: SRC.V\n    verifications:\n      - {first}\n      - matches: {{pattern: 'a{{1000001}}', severity: {severity}}}\n"
    )
            };
            for first in ["matches: {pattern: '('}", "allowed_values: {values: []}"] {
                let document = prepare(raw(first, "error").as_bytes());
                PreparedRun::prepare(document).expect("the earlier finding remains deferred");
                let document = prepare(raw(first, "warning").as_bytes());
                assert!(matches!(PreparedRun::prepare(document),
            Err(Error::Prepare(PrepareError::Invalid(items)))
                if items.iter().any(|item| item.diagnostic().definition().condition == "missing_report")));
                let with_report = raw(first, "warning")
                    .edit("columns: [ID, V]}", "columns: [ID, V], report: report.csv}");
                PreparedRun::prepare(prepare(with_report.as_bytes()))
                    .expect("the earlier finding remains deferred with a declared report file");
            }
        }

        /// Already data: the authored findings of `fixtures/static_verification_checks.json`.
        #[test]
        fn static_verification_checks_match_complete_issues_without_study_authority() {
            let rows = fixture_rows(include_str!("fixtures/static_verification_checks.json"));
            assert_eq!(rows.len(), 20);
            for row in &rows {
                let document = prepare(&fixture_bytes(&row["source"]));
                let run =
                    yamaa_adapters::specification_run::PreparedRun::prepare(document).unwrap();
                for _ in 0..2 {
                    assert_eq!(
                        yamaa_core::json::Value::Array(crate::support::rows(
                            &yamaa_adapters::specification_check::issues(&run)
                        )),
                        row["expected"],
                        "{}",
                        row["case"].as_str().unwrap()
                    );
                }
            }
        }

        /// Already data: the authored reports of `fixtures/original_source_bindings.json`.
        #[test]
        fn original_source_bindings_match_complete_reports_and_exact_repeated_csv() {
            use crate::support::ports::Sources;
            use std::collections::BTreeMap;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            let mut count = 0;
            for row in fixture_rows(include_str!("fixtures/original_source_bindings.json")) {
                let case = row["case"].as_str().unwrap();
                let run = PreparedRun::prepare(prepare(&fixture_bytes(&row["source"]))).unwrap();
                let inputs: BTreeMap<String, String> = row["inputs"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
                    .collect();
                let expected: yamaa_core::json::Value = row["expected"].clone();
                // Each input is served at its written path, its lowercase name.
                let paths: Vec<(String, &str)> = inputs
                    .iter()
                    .map(|(name, bytes)| (format!("{}.csv", name.to_lowercase()), bytes.as_str()))
                    .collect();
                let artifact = fixture_bytes(&row["artifact"]);
                // A second attempt must neither inherit choices nor accumulate handler counts.
                for _ in 0..2 {
                    let mut port = Sources::by_path(
                        paths
                            .iter()
                            .map(|(path, bytes)| (path.as_str(), bytes.as_bytes())),
                    );
                    let attempt =
                        run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                    let example = format!("source-binding-{}", case);
                    let result = specification_report::build_result(
                        &run,
                        &attempt,
                        Identity {
                            runtime: "python",
                            runtime_version: "fixture-runtime",
                            engine_version: "fixture-engine",
                            example: &example,
                            specification: "spec.yaml",
                            base_directory: ".",
                        },
                        &[],
                    )
                    .unwrap();
                    let mut unsaved = expected.clone();
                    unsaved["artifacts"] = yamaa_adapters::json!([]);
                    assert_eq!(result.observations(), unsaved, "{}", case);
                    if artifact.is_empty() {
                        assert!(result.output().is_none());
                        assert!(result.files().is_empty(), "{}", case);
                    } else {
                        assert_eq!(result.files(), [("result.csv", &artifact[..])], "{}", case);
                        assert_eq!(result.saved_observations().unwrap(), expected, "{}", case);
                    }
                    assert_eq!(
                        port.captured(),
                        if inputs.contains_key("DONOR") {
                            vec!["SRC", "DONOR"]
                        } else {
                            vec!["SRC"]
                        }
                    );
                }
                count += 1;
            }
            assert_eq!(count, 12);
        }

        /// Already data: the authored reports of `fixtures/original_mapping_cut.json`.
        #[test]
        fn original_mapping_cut_match_complete_reports_and_exact_repeated_csv() {
            use crate::support::ports::Sources;
            use std::collections::BTreeMap;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            let mut count = 0;
            for row in fixture_rows(include_str!("fixtures/original_mapping_cut.json")) {
                let case = row["case"].as_str().unwrap();
                assert_eq!(
                    yamaa_adapters::json_wire::decode_slice(&fixture_bytes(&row["expected_wire"]))
                        .unwrap(),
                    row["expected"]
                );
                let run = PreparedRun::prepare(prepare(&fixture_bytes(&row["source"]))).unwrap();
                let inputs: BTreeMap<String, String> = row["inputs"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
                    .collect();
                let expected: yamaa_core::json::Value = row["expected"].clone();
                // Each input is served at its written path, its lowercase name.
                let paths: Vec<(String, &str)> = inputs
                    .iter()
                    .map(|(name, bytes)| (format!("{}.csv", name.to_lowercase()), bytes.as_str()))
                    .collect();
                let artifact = fixture_bytes(&row["artifact"]);
                // A second attempt must neither inherit choices nor accumulate handler counts.
                for _ in 0..2 {
                    let mut port = Sources::by_path(
                        paths
                            .iter()
                            .map(|(path, bytes)| (path.as_str(), bytes.as_bytes())),
                    );
                    let attempt =
                        run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                    let example = format!("mapping-cut-{}", case);
                    let result = specification_report::build_result(
                        &run,
                        &attempt,
                        Identity {
                            runtime: "python",
                            runtime_version: "fixture-runtime",
                            engine_version: "fixture-engine",
                            example: &example,
                            specification: "spec.yaml",
                            base_directory: ".",
                        },
                        &[],
                    )
                    .unwrap();
                    let mut unsaved = expected.clone();
                    unsaved["artifacts"] = yamaa_adapters::json!([]);
                    assert_eq!(result.observations(), unsaved, "{}", case);
                    if artifact.is_empty() {
                        assert!(result.output().is_none());
                        assert!(result.files().is_empty(), "{}", case);
                    } else {
                        assert_eq!(result.files(), [("result.csv", &artifact[..])], "{}", case);
                        assert_eq!(result.saved_observations().unwrap(), expected, "{}", case);
                    }
                    assert_eq!(
                        port.captured(),
                        if inputs.contains_key("DONOR") {
                            vec!["SRC", "DONOR"]
                        } else {
                            vec!["SRC"]
                        }
                    );
                }
                count += 1;
            }
            assert_eq!(count, 20);
        }

        /// Already data: the authored findings of `fixtures/mapping_cut_admission.json`.
        #[test]
        fn original_mapping_cut_admission_has_complete_findings_before_study_ports() {
            for row in fixture_rows(include_str!("fixtures/mapping_cut_admission.json")) {
                let bytes = fixture_bytes(&row["source"]);
                let error =
                    yamaa_adapters::specification_run::PreparedRun::prepare(prepare(&bytes))
                        .unwrap_err();
                let expected: yamaa_core::json::Value = row["expected"].clone();
                assert_eq!(
                    spec_findings(&error, None).unwrap(),
                    expected["outcome"]["diagnostics"]
                        .as_array()
                        .unwrap()
                        .clone(),
                    "{}",
                    row["case"].as_str().unwrap()
                );
            }
        }
    }
}
