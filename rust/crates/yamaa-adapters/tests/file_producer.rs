#![cfg(feature = "yaml")]
//! File contracts.
#[allow(dead_code)]
mod support;
mod file {
    mod file_producer_build {
        #[test]
        fn study_relative_schema_links_select_each_producer_wrapper_and_its_output_base() {
            use yamaa_adapters::{file_workflow, producer_result::prepare_owned_classified};
            let study = Study::new();
            study.write("shared/producer.yaml", "schema_version: '1.0'\ndomain: PRODUCER\nkeys: [ID]\ninput: {RAW: {path: input/raw.csv, relative_to: entry, types: {ID: int}}}\ncolumns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\noutput: {path: output/produced.csv, relative_to: entry, columns: [ID]}\n");
            study.write("shared/consumer.yaml", "schema_version: '1.0'\ndomain: CONSUMER\nkeys: [ID]\ninput: {P: {path: output/produced.csv, schema: producer.yaml, relative_to: entry}}\ncolumns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]\noutput: {path: output/final.csv, relative_to: entry, columns: [ID]}\n");
            let mut results = Vec::new();
            for (name, bytes) in [("first", b"ID\n1\n"), ("second", b"ID\n2\n")] {
                study.write(&format!("{name}/input/raw.csv"), bytes);
                study.write(
                    &format!("{name}/spec.yaml"),
                    "schema_version: '1.0'\nparents: ../shared/consumer.yaml\n",
                );
                study.write(
                    &format!("{name}/producer.yaml"),
                    "schema_version: '1.0'\nparents: ../shared/producer.yaml\n",
                );
                fs::create_dir(study.0.join(name).join("output")).unwrap();
                let path = study.path(&format!("{name}/spec.yaml"));
                let yamaa_core::build::Prepared::Producer(graph) = file_workflow::select_request(
                    yamaa_core::domain_entry::Request {
                        specification: &path,
                        environment: None,
                    },
                    Language::Python,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{:?}",
                        yamaa_adapters::project_application::selection_failure(
                            &error,
                            Language::Python
                        )
                    )
                }) else {
                    panic!("selected producer workflow")
                };
                let consumer = &graph.graph().nodes()[0].document;
                let schema = consumer.written_input_origin("P", "schema").unwrap();
                assert_eq!(schema.declaring_source, study.path("shared/consumer.yaml"));
                assert_eq!(schema.base_source, path);
                let output = graph.graph().nodes()[1]
                    .document
                    .written_output_origin()
                    .unwrap();
                assert_eq!(output.declaring_source, study.path("shared/producer.yaml"));
                assert_eq!(
                    output.base_source,
                    study.path(&format!("{name}/producer.yaml"))
                );
                let mut build = graph.into_build();
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                assert!(owned.accepted());
                assert!(activation.trace.is_empty());
                let result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                assert!(result.issues().is_empty());
                results.push((name, bytes, result));
            }
            // A direct link to the shared producer is its own entry. It cannot borrow
            // an unrelated outer study's directory to repair an artifact mismatch.
            study.write("third/spec.yaml", "schema_version: '1.0'\nparents: ../shared/consumer.yaml\ninput: {P: {schema: ../shared/producer.yaml}}\n");
            let path = study.path("third/spec.yaml");
            let issues = yamaa_adapters::file_application::check(
                yamaa_core::domain_entry::Request {
                    specification: &path,
                    environment: None,
                },
                report_id(),
            );
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.condition == "producer_output_path_mismatch"),
                "{issues:?}"
            );
            fs::remove_dir_all(study.0.join("shared")).unwrap();
            for (name, bytes, result) in &mut results {
                fs::remove_dir_all(study.0.join(*name).join("input")).unwrap();
                for _ in 0..2 {
                    assert!(result.save().unwrap());
                    for output in ["output/produced.csv", "output/final.csv"] {
                        assert_eq!(fs::read(study.0.join(*name).join(output)).unwrap(), *bytes);
                    }
                }
            }
        }

        use std::{
            fs,
            path::PathBuf,
            rc::Rc,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
        };
        use yamaa_adapters::{
            file_graph::FileGraph,
            file_producer_build::{Attempt, BoundaryFailure, FileBuild},
            file_resources::{Error as ResourceError, Resources},
            project_source::{self, Kind, OwnedEnvironment, ProjectCapturePort, Reply, Request},
            shipped_schema,
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::{
            dataset::{Execution, ExecutionError},
            function_invocation::{Argument, FailureKind, HostError},
            producer_build::{CaptureError, Failure, ReportPort},
            project_activation::{ActivationPort, Failure as ActivationFailure},
            specification_output::{CompleteError, OutputReport},
            specification_run::PortError,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            producer_graph::Node,
            project_environment::{LockKind, LockReference},
            project_function::Language,
            specification::OutputFinding,
            table::{TableAccess, ValueRef},
            value::Value,
        };

        fn scope() -> yamaa_core::application_observer::Scope<'static> {
            yamaa_core::application_observer::Scope::new(&yamaa_core::application_observer::Noop)
        }

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let root = std::env::temp_dir().join(format!(
                    "yamaa-producer-build-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&root).unwrap();
                Self(fs::canonicalize(root).unwrap())
            }
            fn path(&self, written: &str) -> String {
                let path = self.0.join(written).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path
                    .strip_prefix(r"\\?\")
                    .unwrap_or(&path)
                    .replace('\\', "/");
                path
            }
            fn write(&self, written: &str, bytes: impl AsRef<[u8]>) {
                let path = self.0.join(written);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            fn build(&self, language: Language) -> FileBuild {
                FileGraph::prepare(
                    Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
                    "consumer/root.yaml",
                    environment(language),
                    shipped_schema::capture().unwrap(),
                )
                .unwrap_or_else(|error| panic!("graph fixture: {:?}", error.error()))
                .into_build()
            }
            fn rounded(&self) {
                self.write("producer/layers/base.yaml", PRODUCER);
                self.write(
                    "producer/p.yaml",
                    "schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: PRODUCER\n",
                );
                self.write("consumer/root.yaml", CONSUMER);
                self.write("raw/raw.csv", b"ID,VALUE\n1,1.234\n2,2.345\n");
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                if self.0.exists() {
                    fs::remove_dir_all(&self.0).unwrap();
                }
            }
        }
        const PRODUCER: &str = "schema_version: '1.0'\ndomain: TEST\nbase: RAW\nkeys: [ID]\ninput: {RAW: {path: ../../raw/raw.csv, types: {ID: int, VALUE: float}}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: float, label: Reported value, derivation: {function: {name: id_float, args: {x: RAW.VALUE}}}}\noutput: {path: ../../generated/producer.csv, columns: [ID, VALUE], decimals: 2}\n";
        const CONSUMER: &str = "schema_version: '1.0'\ndomain: TEST\nbase: FIRST\nkeys: [ID]\ninput:\n  FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml}\n  SECOND: {path: ../generated/producer.csv, schema: ../producer/./p.yaml}\nintermediates: [{id: SECOND_REFERENCE, dataset: SECOND, key: [ID], no_match: null}]\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: FIRST.ID}\n  - {name: FIRST_VALUE, type: float, label: First value, derivation: FIRST.VALUE}\n  - {name: SECOND_VALUE, type: float, label: Second value, derivation: SECOND_REFERENCE.VALUE}\n  - {name: VALUE, type: float, label: Reported value, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}}\noutput: {path: ../generated/root.csv, columns: [ID, VALUE], decimals: 2}\n";

        #[test]
        fn native_uneven_csv_and_parquet_graphs_keep_each_tables_actual_rows() {
            for format in [
                "csv",
                #[cfg(feature = "parquet")]
                "parquet",
            ] {
                let study = Study::new();
                study.write(
                    "raw/raw.csv",
                    "ID,VALUE\n1,1\n1,2\n1,3\n1,4\n1,5\n1,6\n1,7\n",
                );
                study.write("producer/p.yaml", format!("schema_version: '1.0'\ndomain: COUNTED\nkeys: [ID]\ninput: {{RAW: {{path: ../raw/raw.csv, types: {{ID: int, VALUE: int}}}}}}\nrows:\n  - id: counted\n    dataset: RAW\n    group_by: [RAW.ID]\n    derivations: {{ID: RAW.ID, VALUE: {{aggregate: 'COUNT(RAW.*)'}}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier}}\n  - {{name: VALUE, type: int, label: Count}}\noutput: {{path: ../generated/producer.{format}, columns: [ID, VALUE]}}\n"));
                study.write("consumer/root.yaml", format!("schema_version: '1.0'\ndomain: ROOT\nkeys: [ID]\ninput: {{FIRST: {{path: ../generated/producer.{format}, schema: ../producer/p.yaml}}}}\nrows:\n  - id: counted\n    dataset: FIRST\n    group_by: [FIRST.ID, FIRST.VALUE]\n    derivations: {{ID: FIRST.ID, SOURCE_COUNT: FIRST.VALUE, VALUE: {{aggregate: 'COUNT(FIRST.*)'}}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier}}\n  - {{name: SOURCE_COUNT, type: int, label: Original source count}}\n  - {{name: VALUE, type: int, label: Count}}\noutput: {{path: ../generated/root.csv, columns: [ID, SOURCE_COUNT, VALUE]}}\n"));
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, Mode::Lock);
                let attempt = build.build(&mut activation, &mut Report::default(), scope());
                assert!(attempt.accepted(), "{format}: {:?}", attempt.graph.outcome);
                assert!(activation.trace.is_empty());
                assert_eq!(artifact(&attempt, 1), b"ID,SOURCE_COUNT,VALUE\n1,7,1\n");
                let producer = attempt.graph.nodes[0]
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap();
                assert_eq!(producer.dataset.row_count(), 1);
                assert_eq!(producer.dataset.cell(0, 1).unwrap(), ValueRef::Int(7));
                assert_eq!(
                    attempt.graph.nodes[0].dataset.sources[0]
                        .table
                        .as_ref()
                        .unwrap()
                        .row_count(),
                    7
                );
                assert_eq!(
                    attempt.graph.nodes[1].dataset.sources[0]
                        .table
                        .as_ref()
                        .unwrap()
                        .row_count(),
                    1
                );
            }
        }

        #[test]
        fn producer_link_reduces_daily_doses_then_counts_days_before_any_publication() {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            for language in [Language::Python, Language::R] {
                let study = Study::new();
                study.write(
            "raw/ex.csv",
            "USUBJID,EXTRT,EXSTDTC,EXDOSE\nA,X,2030-01-01,100\nA,X,2030-01-01,100\nA,X,2030-01-02,200\nA,X,2030-01-03,200\nB,X,2030-01-01,100\nB,X,2030-01-02,100\nB,X,2030-01-02,100\n",
        );
                study.write(
                    "producer/daily.yaml",
                    r#"schema_version: '1.0'
domain: ADEXDAY
keys: [USUBJID, EXTRT, EXSTDTC]
input:
  EX: {path: ../raw/ex.csv, types: {EXDOSE: int}}
output: {path: ../generated/daily.csv, columns: [USUBJID, EXTRT, EXSTDTC, ADOSE]}
rows:
  - id: day
    dataset: EX
    group_by: [EX.USUBJID, EX.EXTRT, EX.EXSTDTC]
    derivations:
      USUBJID: EX.USUBJID
      EXTRT: EX.EXTRT
      EXSTDTC: EX.EXSTDTC
      ADOSE: {aggregate: "SUM(EX.EXDOSE)"}
columns:
  - {name: USUBJID, type: str, label: Subject}
  - {name: EXTRT, type: str, label: Treatment}
  - {name: EXSTDTC, type: str, label: Dosing date}
  - {name: ADOSE, type: int, label: Daily total dose}
"#,
                );
                study.write(
                    "consumer/root.yaml",
                    r#"schema_version: '1.0'
domain: ADEXSUM
keys: [USUBJID, EXTRT, ADOSE]
input:
  DAILY: {path: ../generated/daily.csv, schema: ../producer/daily.yaml}
output: {path: ../generated/adexsum.csv, columns: [USUBJID, EXTRT, ADOSE, TRTDUR]}
rows:
  - id: summary
    dataset: DAILY
    group_by: [DAILY.USUBJID, DAILY.EXTRT, DAILY.ADOSE]
    derivations:
      USUBJID: DAILY.USUBJID
      EXTRT: DAILY.EXTRT
      ADOSE: DAILY.ADOSE
      TRTDUR: {aggregate: "COUNT(DAILY.*)"}
columns:
  - {name: USUBJID, type: str}
  - {name: EXTRT, type: str}
  - {name: ADOSE, type: int}
  - {name: TRTDUR, type: int}
"#,
                );
                let mut build = study.build(language);
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.output().is_some());
                assert!(!study.0.join("generated/daily.csv").exists());
                assert!(!study.0.join("generated/adexsum.csv").exists());
                fs::create_dir(study.0.join("generated")).unwrap();
                assert!(result.save().unwrap());
                // The two day-1 doses combine before the consumer counts daily records.
                assert_eq!(
            fs::read(study.0.join("generated/daily.csv")).unwrap(),
            b"USUBJID,EXTRT,EXSTDTC,ADOSE\nA,X,2030-01-01,200\nA,X,2030-01-02,200\nA,X,2030-01-03,200\nB,X,2030-01-01,100\nB,X,2030-01-02,200\n"
        );
                assert_eq!(
                    fs::read(study.0.join("generated/adexsum.csv")).unwrap(),
                    b"USUBJID,EXTRT,ADOSE,TRTDUR\nA,X,200,3\nB,X,100,1\nB,X,200,1\n"
                );
            }
        }

        #[test]
        fn linked_producer_ordinal_is_local_to_consumers_and_keeps_the_stored_contract() {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            let study = Study::new();
            study.rounded();
            study.write("consumer/root.yaml", CONSUMER.replace("schema: ../producer/p.yaml}", "schema: ../producer/p.yaml, ordinal: REC}").replace("output: {path:", "  - {name: POSITION, type: int, label: Position, derivation: FIRST.REC}\noutput: {path:").replace("columns: [ID, VALUE], decimals", "columns: [ID, VALUE, POSITION], decimals"));
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let owned = build.build_owned_reported(&mut activation, report_id(), scope());
            let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            fs::create_dir(study.0.join("generated")).unwrap();
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(study.0.join("generated/root.csv")).unwrap(),
                b"ID,VALUE,POSITION\n1,2.46,1\n2,4.70,2\n"
            );

            for raw in [b"ID,VALUE\n1,1.234\n".as_slice(), b"ID,VALUE\n"] {
                study.write("raw/raw.csv", raw);
                study.write(
                    "consumer/root.yaml",
                    CONSUMER.replace(
                        "schema: ../producer/p.yaml}",
                        "schema: ../producer/p.yaml, ordinal: ID}",
                    ),
                );
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                let result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                assert!(result.output().is_none());
                assert_eq!(result.issues().len(), 1);
                let issue = &result.issues()[0];
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    ("validation", "ordinal_field_collision", Some("REQ-1294"))
                );
                assert_eq!(issue.spec_paths, ["input.FIRST.ordinal"]);
                assert_eq!(
                    yamaa_adapters::json_wire::decode(&issue.context).unwrap(),
                    yamaa_adapters::json!({"dataset":"FIRST", "field":"ID"})
                );
            }
        }

        #[test]
        fn sealed_native_save_keeps_original_held_bytes_after_sources_change_and_other_owners_drop()
        {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            for language in [Language::Python, Language::R] {
                let study = Study::new();
                study.rounded();
                fs::create_dir(study.0.join("generated")).unwrap();
                let mut build = study.build(language);
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                let before = result.observations().clone();
                let output = result.output().unwrap().to_vec();
                let events = activation.trace.clone();
                assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
                study.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n");
                fs::remove_dir_all(study.0.join("producer")).unwrap();
                fs::remove_dir_all(study.0.join("consumer")).unwrap();
                fs::remove_dir_all(study.0.join("raw")).unwrap();
                drop(build);
                for _ in 0..2 {
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read(study.0.join("generated/producer.csv")).unwrap(),
                        b"ID,VALUE\n1,1.23\n2,2.35\n"
                    );
                    assert_eq!(
                        fs::read(study.0.join("generated/root.csv")).unwrap(),
                        b"ID,VALUE\n1,2.46\n2,4.70\n"
                    );
                    assert_eq!(
                        result.observations()["publication"],
                        yamaa_adapters::json!({"status":"success","entered":[{"node":1,"published":true},{"node":0,"published":true}],"error":null})
                    );
                    assert_eq!(result.output().unwrap(), output);
                    assert!(result.issues().is_empty());
                    let mut report = result.observations().clone();
                    report.as_object_mut().unwrap().remove("publication");
                    assert_eq!(report, before);
                    assert_eq!(activation.trace, events);
                }
                assert!(owned.accepted());
                assert_eq!(owned.graph.nodes.len(), 2);
            }
        }
        #[test]
        fn authored_public_success_evidence_and_failed_owned_graph_never_obtain_save_authority() {
            use yamaa_adapters::{
                producer_report::NativeAttempt,
                producer_result::{prepare_classified, prepare_owned_classified},
            };
            let study = Study::new();
            study.rounded();
            fs::create_dir(study.0.join("generated")).unwrap();
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let actual = build.build_reported(&mut activation, report_id(), scope());
            // Every field is deliberately public evidence: a caller can author this
            // success, but that construction cannot create the private OwnedAttempt.
            let authored = NativeAttempt {
                provenance: Arc::clone(&actual.provenance),
                boundary: Ok(()),
                graph: actual.graph,
                resources: actual.resources,
            };
            let mut result = prepare_classified(&authored, report_id(), &[]).unwrap();
            assert!(result.output().is_some());
            assert!(result.save().is_err());
            activation.mode = Mode::Lock;
            let failed = build.build_owned_reported(&mut activation, report_id(), scope());
            assert!(!failed.accepted());
            assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
            assert!(prepare_owned_classified(&failed, report_id(), &[]).is_err());
        }
        #[test]
        fn whole_set_invalid_target_and_duplicate_normalized_alias_refuse_every_write() {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            for duplicate in [false, true] {
                let study = Study::new();
                study.rounded();
                fs::create_dir(study.0.join("generated")).unwrap();
                if duplicate {
                    study.write(
                        "consumer/root.yaml",
                        CONSUMER
                            .replace("../generated/root.csv", "../generated/dir/../producer.csv"),
                    );
                } else {
                    fs::create_dir(study.0.join("generated/root.csv")).unwrap();
                }
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                assert!(owned.accepted());
                let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                assert!(!result.save().unwrap());
                assert!(!study.0.join("generated/producer.csv").exists());
                assert!(result
                    .publication_attempt()
                    .unwrap()
                    .entered
                    .entered
                    .is_empty());
                assert_eq!(result.observations()["outcome"], "success");
                assert!(result.output().is_some());
                assert_eq!(
                    result.observations()["publication"]["entered"],
                    yamaa_adapters::json!([])
                );
            }
        }

        #[cfg(windows)]
        #[test]
        fn absent_windows_case_alias_targets_refuse_before_every_write() {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            let study = Study::new();
            study.rounded();
            study.write(
                "consumer/root.yaml",
                CONSUMER.replace("../generated/root.csv", "../generated/PRODUCER.csv"),
            );
            fs::create_dir(study.0.join("generated")).unwrap();
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let owned = build.build_owned_reported(&mut activation, report_id(), scope());
            assert!(owned.accepted());
            let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(
                result.observations()["publication"]["error"]["code"],
                "target_admission"
            );
            assert!(result
                .publication_attempt()
                .unwrap()
                .entered
                .entered
                .is_empty());
            assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
            assert!(result.output().is_some());
        }
        #[cfg(target_os = "macos")]
        #[test]
        fn macos_absent_case_alias_and_unproven_unicode_targets_preflight_before_every_write() {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            let study = Study::new();
            study.rounded();
            fs::create_dir(study.0.join("generated")).unwrap();
            study.write("generated/case-probe", b"native case witness");
            let sensitive = !study.0.join("generated/CASE-PROBE").exists();
            fs::remove_file(study.0.join("generated/case-probe")).unwrap();
            for name in ["PRODUCER.csv", r"\u00e9.csv"] {
                study.write(
                    "consumer/root.yaml",
                    CONSUMER.replace("../generated/root.csv", &format!("\"../generated/{name}\"")),
                );
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                assert!(owned.accepted());
                let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                if name == "PRODUCER.csv" && sensitive {
                    assert!(result.save().unwrap());
                    fs::remove_file(study.0.join("generated/producer.csv")).unwrap();
                    fs::remove_file(study.0.join("generated/PRODUCER.csv")).unwrap();
                } else {
                    assert!(!result.save().unwrap());
                    assert!(result
                        .publication_attempt()
                        .unwrap()
                        .entered
                        .entered
                        .is_empty());
                    assert_eq!(
                        result.observations()["publication"]["error"]["code"],
                        "target_preflight"
                    );
                }
                assert_eq!(fs::read_dir(study.0.join("generated")).unwrap().count(), 0);
                assert!(result.output().is_some());
            }
        }
        #[cfg(unix)]
        #[test]
        fn actual_late_consumer_publication_failure_keeps_producer_bytes_original_io_and_completed_build(
        ) {
            use rustix::fs::{self as unix, FlockOperation, Mode as OpenMode, OFlags};
            use yamaa_adapters::producer_result::prepare_owned_classified;
            use yamaa_core::producer_publication::Failure as SaveFailure;
            let study = Study::new();
            study.rounded();
            study.write(
                "consumer/root.yaml",
                CONSUMER.replace("../generated/root.csv", "../published/root.csv"),
            );
            fs::create_dir(study.0.join("generated")).unwrap();
            study.write("published/root.csv", b"previous consumer bytes");
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let owned = build.build_owned_reported(&mut activation, report_id(), scope());
            let events = activation.trace.clone();
            let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
            let before = result.observations().clone();
            let lock = unix::open(
                study.0.join("published"),
                OFlags::RDONLY | OFlags::DIRECTORY,
                OpenMode::empty(),
            )
            .unwrap();
            unix::flock(&lock, FlockOperation::NonBlockingLockExclusive).unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(
                fs::read(study.0.join("generated/producer.csv")).unwrap(),
                b"ID,VALUE\n1,1.23\n2,2.35\n"
            );
            assert_eq!(
                fs::read(study.0.join("published/root.csv")).unwrap(),
                b"previous consumer bytes"
            );
            let saved = result.publication_attempt().unwrap();
            assert_eq!(
                saved
                    .entered
                    .entered
                    .iter()
                    .map(|n| (n.node, n.published))
                    .collect::<Vec<_>>(),
                [(1, true), (0, false)]
            );
            assert!(
                matches!(&saved.entered.outcome,Err(SaveFailure::Publish {node:0,error}) if matches!(&error.error,yamaa_adapters::file_publication::Error::Io(e) if e.kind()==std::io::ErrorKind::WouldBlock))
            );
            assert!(result
                .issues()
                .iter()
                .any(|i| i.condition == "publication_failed"
                    && i.requirement.as_deref() == Some("REQ-0764")));
            assert!(owned.accepted());
            assert_eq!(owned.graph.nodes.len(), 2);
            assert!(result.output().is_some());
            unix::flock(&lock, FlockOperation::Unlock).unwrap();
            assert!(result.save().unwrap());
            assert!(result.issues().is_empty());
            let mut report = result.observations().clone();
            report.as_object_mut().unwrap().remove("publication");
            assert_eq!(report, before);
            assert_eq!(activation.trace, events);
        }

        #[test]
        fn whole_graph_report_views_keep_original_document_and_plan_together() {
            use yamaa_adapters::specification_run_view::RunView;
            let study = Study::new();
            study.rounded();
            let build = study.build(Language::Python);
            let provenance = Arc::clone(build.provenance());
            assert_eq!(provenance.metadata().order(), [1, 0]);
            drop(build);
            fs::remove_dir_all(&study.0).unwrap();

            let views = provenance.report_views().collect::<Vec<_>>();
            assert_eq!(views.len(), 2);
            for (index, view) in views.iter().enumerate() {
                assert!(std::ptr::eq(
                    view.document(),
                    provenance.documents()[index].as_ref()
                ));
                assert_eq!(view.document().source().identity, view.node().identity());
                assert!(view.check_diagnostics().is_empty());
                assert_eq!(view.compiled().output_decimals(), Some("2"));
            }
            assert!(views[0].node().identity().ends_with("/consumer/root.yaml"));
            assert!(views[1].node().identity().ends_with("/producer/p.yaml"));
            assert_eq!(views[0].compiled().source().name, "FIRST");
            assert_eq!(views[1].compiled().source().name, "RAW");
            assert_eq!(views[0].document().parents().len(), 0);
            assert_eq!(views[1].document().parents().len(), 1);
            assert_eq!(
                views[1].document().parents()[0].source().bytes,
                PRODUCER.as_bytes()
            );
        }

        struct LockCapture {
            language: Language,
        }
        impl ProjectCapturePort for LockCapture {
            type Error = ();
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, ()> {
                assert_eq!(request.kind, Kind::Lock);
                Ok(Reply::Lock {
                    source: Source {
                        identity: "held/lock".into(),
                        bytes: b"held original lock".to_vec(),
                    },
                    kind: if self.language == Language::Python {
                        LockKind::Uv
                    } else {
                        LockKind::Renv
                    },
                })
            }
        }
        fn environment(language: Language) -> OwnedEnvironment {
            let (name, call, lock) = if language == Language::Python {
                ("python", "program.id_float", "uv.lock")
            } else {
                ("r", "program::id_float", "renv.lock")
            };
            let text = format!("schema_version: '1.0'\nlanguage: {name}\nlock: {lock}\nfunctions:\n  id_float:\n    function: {call}\n    description: Float identity.\n    params: [{{name: x, type: float}}]\n    returns: float\n    tests:\n      - {{id: normal, covers: [normal, numeric-comparison], args: {{x: 7.0}}, result: 7.0}}\n      - {{id: boundary, covers: [boundary], args: {{x: 0.0}}, result: 0.0}}\n      - {{id: missing, covers: ['short-circuit-missing:x'], args: {{x: null}}, result: null}}\n");
            project_source::prepare(
                shipped_schema::capture_environment().unwrap(),
                Source {
                    identity: "held/environment.yaml".into(),
                    bytes: text.into_bytes(),
                },
                language,
                &mut LockCapture { language },
            )
            .unwrap_or_else(|error| match error {
                project_source::Failure::Rejected(error) => {
                    panic!("environment fixture: {:?}", error.admission)
                }
                _ => panic!("environment fixture capture failed"),
            })
            .into_owned()
        }
        #[derive(Debug)]
        struct Payload {
            original: Rc<()>,
            interrupt: bool,
        }
        #[derive(Clone, Copy)]
        enum Mode {
            Pass,
            Lock,
            BindingFailure,
            CaseFailure,
            CaseMismatch,
            CaseInterrupt,
            LiveFailure,
            LiveInterrupt,
            Unwind,
            ChangeAfterActivation,
        }
        struct Activation {
            study: PathBuf,
            trace: Vec<String>,
            original: Rc<()>,
            unwind: Arc<()>,
            mode: Mode,
        }
        impl Activation {
            fn new(study: &Study, mode: Mode) -> Self {
                Self {
                    study: study.0.clone(),
                    trace: vec![],
                    original: Rc::new(()),
                    unwind: Arc::new(()),
                    mode,
                }
            }
            fn payload(&self, interrupt: bool) -> Payload {
                Payload {
                    original: Rc::clone(&self.original),
                    interrupt,
                }
            }
        }
        impl ActivationPort for Activation {
            type Error = Payload;
            type Handle = ();
            fn verify_lock(
                &mut self,
                language: Language,
                lock: &LockReference,
                calls: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "id_float");
                assert_eq!(
                    lock.kind,
                    if language == Language::Python {
                        LockKind::Uv
                    } else {
                        LockKind::Renv
                    }
                );
                self.trace.push("lock".into());
                if matches!(self.mode, Mode::Lock) {
                    Err(self.payload(false))
                } else {
                    Ok(())
                }
            }
            fn bind(
                &mut self,
                call: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Payload> {
                assert_eq!(call.name, "id_float");
                self.trace.push("bind".into());
                if matches!(self.mode, Mode::BindingFailure) {
                    return Err(self.payload(false));
                }
                Ok(())
            }
            fn is_interrupt(&self, error: &Payload) -> bool {
                error.interrupt
            }
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
                let [Argument {
                    name: "x",
                    value: ValueRef::Float(value),
                }] = args
                else {
                    panic!("exact float argument")
                };
                self.trace.push(format!("invoke:{}", value.get()));
                if value.get() == 0.0 {
                    if matches!(self.mode, Mode::CaseFailure) {
                        return Err(HostError::Raised(self.payload(false)));
                    }
                    if matches!(self.mode, Mode::CaseMismatch) {
                        return Ok(Value::Int(19));
                    }
                    if matches!(self.mode, Mode::CaseInterrupt) {
                        return Err(HostError::Raised(self.payload(true)));
                    }
                    if matches!(self.mode, Mode::ChangeAfterActivation) {
                        // Native study capture must happen after all conformance cases.
                        fs::write(
                            self.study.join("raw/raw.csv"),
                            b"ID,VALUE\n1,1.234\n2,2.345\n",
                        )
                        .unwrap();
                    }
                }
                if value.get() == 1.234 {
                    match self.mode {
                        Mode::LiveFailure => return Err(HostError::Raised(self.payload(false))),
                        Mode::LiveInterrupt => return Err(HostError::Raised(self.payload(true))),
                        Mode::Unwind => std::panic::panic_any(Arc::clone(&self.unwind)),
                        _ => {}
                    }
                }
                Ok(Value::Float(*value))
            }
        }
        #[derive(Default)]
        struct Report {
            entered: Vec<String>,
            fail: bool,
            final_failure: bool,
        }
        impl ReportPort for Report {
            fn select<C, D, T: TableAccess>(
                &mut self,
                node: &Node,
                _attempt: &yamaa_core::specification_run::CapturedAttempt<C, D, T>,
            ) -> Result<(), Self::Error> {
                self.entered.push(node.identity().into());
                if self.fail {
                    Err("original report refusal")
                } else {
                    Ok(())
                }
            }
        }
        impl OutputReport for Report {
            type Error = &'static str;
            type Report = String;
            fn failure(&mut self) -> Result<String, Self::Error> {
                Ok("failed".into())
            }
            fn begin(&mut self, _: &Execution) -> Result<(), Self::Error> {
                Ok(())
            }
            fn rejected(&mut self, _: &[OutputFinding]) -> Result<String, Self::Error> {
                Ok("rejected".into())
            }
            fn success(
                &mut self,
                _: &Execution,
                _: &[usize],
                _: &[u8],
            ) -> Result<String, Self::Error> {
                if self.final_failure {
                    Err("original report rendering refusal")
                } else {
                    Ok("accepted".into())
                }
            }
        }
        fn artifact<E, R, F: std::fmt::Debug>(attempt: &Attempt<E, R, F>, node: usize) -> &[u8] {
            attempt.graph.nodes[node]
                .output
                .as_ref()
                .ok()
                .unwrap()
                .as_ref()
                .unwrap()
                .artifact()
                .unwrap()
                .bytes()
        }

        fn passthrough(input: &str, base: &str, output: &str, kind: &str) -> String {
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: {kind}, label: Value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE]}}\n")
        }

        #[test]
        #[cfg(feature = "parquet")]
        fn native_parquet_round_trip_uses_held_artifact_and_a_functionless_graph_grants_no_activation(
        ) {
            let study = Study::new();
            study.write("raw/raw.csv", b"ID,VALUE\n1,13\n2,17\n");
            study.write(
                "producer/p.yaml",
                passthrough(
                    "{RAW: {path: ../raw/raw.csv, types: {ID: int, VALUE: int}}}",
                    "RAW",
                    "../generated/producer.parquet",
                    "int",
                ),
            );
            study.write("consumer/root.yaml", passthrough("{FIRST: {path: ../generated/producer.parquet, schema: ../producer/p.yaml}, SECOND: {path: ../generated/producer.parquet, schema: ../producer/./p.yaml}}", "FIRST", "../generated/root.csv", "int"));
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Lock);
            let mut report = Report::default();
            let attempt = build.build(&mut activation, &mut report, scope());
            assert!(attempt.accepted());
            assert!(activation.trace.is_empty());
            assert_eq!(attempt.graph.activation, Default::default());
            assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,13\n2,17\n");
            let produced = artifact(&attempt, 0);
            assert!(produced.starts_with(b"PAR1") && produced.ends_with(b"PAR1"));
            for source in &attempt.graph.nodes[1].dataset.sources {
                assert_eq!(
                    source.snapshot.as_ref().unwrap().as_ptr(),
                    produced.as_ptr()
                );
                let table = source.table.as_ref().unwrap();
                assert_eq!(table.row_count(), 2);
                assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(13));
                assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Int(17));
            }
            assert!(!study.0.join("generated").exists());
            let complete = build.build_reported(&mut activation, report_id(), scope());
            assert!(complete.accepted());
            let value =
                yamaa_adapters::producer_report::attempt_report(&complete, report_id()).unwrap();
            assert_eq!(value["outcome"], "success");
            assert_eq!(
                value["activation"],
                yamaa_adapters::json!({"lock":"not_requested","bindings":[],"tests":[]})
            );
            assert_eq!(value["artifacts"][0]["profile"], "parquet");
            assert_eq!(
                value["artifacts"][0]["records"],
                yamaa_adapters::json!(["[\"ID\", \"VALUE\"]", "[1, 13]", "[2, 17]"])
            );
            assert_eq!(value["artifacts"][1]["content"], "ID,VALUE\n1,13\n2,17\n");
        }

        #[test]
        fn native_diamond_executes_each_canonical_node_once_and_keeps_shared_leaf_bytes() {
            let study = Study::new();
            study.rounded();
            for branch in ["left", "right"] {
                study.write(
                    &format!("{branch}.yaml"),
                    passthrough(
                        "{LEAF: {path: generated/producer.csv, schema: producer/p.yaml}}",
                        "LEAF",
                        &format!("generated/{branch}.csv"),
                        "float",
                    ),
                );
            }
            let root = CONSUMER
        .replace(
            "../generated/producer.csv, schema: ../producer/p.yaml",
            "../generated/left.csv, schema: ../left.yaml",
        )
        .replace(
            "../generated/producer.csv, schema: ../producer/./p.yaml",
            "../generated/right.csv, schema: ../right.yaml",
        )
        .replace(
            "intermediates:",
            "  AGAIN: {path: ../generated/left.csv, schema: .././left.yaml}\nintermediates:",
        );
            study.write("consumer/root.yaml", root);
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let mut report = Report::default();
            let attempt = build.build(&mut activation, &mut report, scope());
            assert!(attempt.accepted());
            assert_eq!(
                attempt
                    .graph
                    .nodes
                    .iter()
                    .map(|n| n.node)
                    .collect::<Vec<_>>(),
                [3, 1, 2, 0]
            );
            assert_eq!(
                activation.trace,
                [
                    "lock",
                    "bind",
                    "invoke:7",
                    "invoke:0",
                    "invoke:1.234",
                    "invoke:2.345"
                ]
            );
            assert_eq!(
                report.entered,
                [
                    study.path("producer/p.yaml"),
                    study.path("left.yaml"),
                    study.path("right.yaml"),
                    study.path("consumer/root.yaml")
                ]
            );
            let left = attempt.graph.nodes[1].dataset.sources[0]
                .snapshot
                .as_ref()
                .unwrap();
            let right = attempt.graph.nodes[2].dataset.sources[0]
                .snapshot
                .as_ref()
                .unwrap();
            assert!(Arc::ptr_eq(left, right));
            assert_eq!(left.as_ptr(), artifact(&attempt, 0).as_ptr());
            assert_eq!(artifact(&attempt, 3), b"ID,VALUE\n1,2.46\n2,4.70\n");
            assert!(!study.0.join("generated").exists());
        }

        #[test]
        fn native_rounded_bytes_are_reingested_twice_after_producer_only_activation_with_original_origins(
        ) {
            for language in [Language::Python, Language::R] {
                let study = Study::new();
                study.rounded();
                let mut build = study.build(language);
                let mut activation = Activation::new(&study, Mode::ChangeAfterActivation);
                // A capture before the final activation case would observe the wrong data.
                study.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n");
                let mut report = Report::default();
                let attempt = build.build(&mut activation, &mut report, scope());
                assert!(attempt.accepted(), "{:?}", attempt.graph.outcome);
                assert_eq!(
                    activation.trace,
                    [
                        "lock",
                        "bind",
                        "invoke:7",
                        "invoke:0",
                        "invoke:1.234",
                        "invoke:2.345"
                    ]
                );
                assert_eq!(
                    attempt
                        .graph
                        .nodes
                        .iter()
                        .map(|n| n.node)
                        .collect::<Vec<_>>(),
                    [1, 0]
                );
                assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
                assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
                let execution = attempt.graph.nodes[0]
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap();
                for (row, expected) in [1.234, 2.345].into_iter().enumerate() {
                    let ValueRef::Float(value) = execution.dataset.cell(row, 1).unwrap() else {
                        panic!("held unrounded producer float")
                    };
                    assert_eq!(value.get(), expected);
                }
                let consumer = &attempt.graph.nodes[1].dataset;
                assert_eq!(
                    consumer
                        .sources
                        .iter()
                        .map(|r| r.read.snapshots_created)
                        .collect::<Vec<_>>(),
                    [Some(0), Some(0)]
                );
                assert_eq!(consumer.sources.len(), 2);
                assert!(Arc::ptr_eq(
                    consumer.sources[0].snapshot.as_ref().unwrap(),
                    consumer.sources[1].snapshot.as_ref().unwrap()
                ));
                assert_eq!(
                    &**consumer.sources[0].snapshot.as_ref().unwrap(),
                    b"ID,VALUE\n1,1.23\n2,2.35\n"
                );
                assert_eq!(
                    report.entered,
                    [
                        study.path("producer/p.yaml"),
                        study.path("consumer/root.yaml")
                    ]
                );
                let metadata = attempt.provenance.metadata();
                assert!(metadata.nodes()[0].activation_slots().is_empty());
                assert_eq!(metadata.nodes()[1].activation_slots(), [0]);
                assert_eq!(
                    metadata.nodes()[0].producers()[0]
                        .output_origin()
                        .declaring_source(),
                    study.path("producer/layers/base.yaml")
                );
                assert!(!study.0.join("generated").exists());
                drop(build);
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
                assert_eq!(
                    attempt.provenance.documents()[1].source().bytes,
                    b"schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: PRODUCER\n"
                );
                assert!(attempt
                    .resources
                    .iter()
                    .any(|(p, b)| p.ends_with("/raw/raw.csv")
                        && &**b == b"ID,VALUE\n1,1.234\n2,2.345\n"));
            }
        }

        #[test]
        fn fresh_builds_repeat_activation_and_detect_changed_study_after_activation_with_original_snapshot(
        ) {
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let mut report = Report::default();
            let first = build.build(&mut activation, &mut report, scope());
            let second = build.build(&mut activation, &mut report, scope());
            assert!(first.accepted() && second.accepted());
            assert_eq!(
                first.graph.nodes[0].dataset.sources[0]
                    .read
                    .snapshots_created,
                Some(1)
            );
            assert_eq!(
                second.graph.nodes[0].dataset.sources[0]
                    .read
                    .snapshots_created,
                Some(0)
            );
            assert_eq!(activation.trace.len(), 12);
            study.write("raw/raw.csv", b"ID,VALUE\n1,9.99\n");
            activation.trace.clear();
            let changed = build.build(&mut activation, &mut report, scope());
            assert!(!changed.accepted());
            assert!(changed.boundary.is_ok());
            assert_eq!(activation.trace, ["lock", "bind", "invoke:7", "invoke:0"]);
            assert_eq!(changed.graph.nodes.len(), 1);
            assert!(matches!(
                changed.graph.nodes[0].dataset.result,
                Err(PortError::Capture(CaptureError::External(
                    ResourceError::Changed
                )))
            ));
            assert!(changed.resources.iter().any(
                |(p, b)| p.ends_with("/raw/raw.csv") && &**b == b"ID,VALUE\n1,1.234\n2,2.345\n"
            ));
            assert_eq!(artifact(&first, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
        }

        #[test]
        fn native_changed_metadata_refuses_before_host_or_study_authority() {
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let mut report = Report::default();
            study.write("producer/layers/base.yaml", b"changed metadata");
            let changed = build.build(&mut activation, &mut report, scope());
            assert!(
                matches!(&changed.boundary, Err(BoundaryFailure::Resource { identity: Some(p), error: ResourceError::Changed }) if p == &study.path("producer/layers/base.yaml"))
            );
            assert!(activation.trace.is_empty() && changed.graph.nodes.is_empty());
            assert!(changed
                .resources
                .iter()
                .any(|(p, b)| p.ends_with("/producer/layers/base.yaml")
                    && &**b == PRODUCER.as_bytes()));
            assert!(!changed.resources.iter().any(|(p, _)| p.ends_with(".csv")));
        }

        #[test]
        fn native_activation_failures_interrupts_and_unwind_retain_owned_original_prefix_and_stop_dependents(
        ) {
            for mode in [
                Mode::Lock,
                Mode::CaseInterrupt,
                Mode::LiveFailure,
                Mode::LiveInterrupt,
                Mode::Unwind,
            ] {
                let study = Study::new();
                study.rounded();
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, mode);
                let mut report = Report::default();
                let attempt = build.build(&mut activation, &mut report, scope());
                assert!(!attempt.accepted());
                assert!(report.entered.is_empty());
                match mode {
                    Mode::Lock => {
                        assert!(attempt.graph.nodes.is_empty());
                        assert!(
                            matches!(&attempt.graph.outcome, Err(Failure::Activation(ActivationFailure::Lock(e))) if Rc::ptr_eq(&e.original, &activation.original))
                        );
                    }
                    Mode::CaseInterrupt => {
                        assert!(attempt.graph.nodes.is_empty());
                        assert_eq!(activation.trace.len(), 4);
                    }
                    Mode::Unwind => {
                        let Err(BoundaryFailure::Unwind(payload)) = &attempt.boundary else {
                            panic!("original unwind")
                        };
                        assert!(Arc::ptr_eq(
                            payload.downcast_ref::<Arc<()>>().unwrap(),
                            &activation.unwind
                        ));
                        assert_eq!(attempt.graph.nodes.len(), 1);
                        assert!(matches!(attempt.graph.outcome, Err(Failure::Incomplete)));
                        assert_eq!(
                            attempt.graph.nodes[0]
                                .dataset
                                .sources
                                .iter()
                                .filter(|s| s.snapshot.is_some())
                                .count(),
                            1
                        );
                    }
                    _ => {
                        assert_eq!(attempt.graph.nodes.len(), 1);
                        let execution = attempt.graph.nodes[0].dataset.result.as_ref().unwrap();
                        let error = execution.result.as_ref().unwrap_err();
                        let ExecutionError::ProjectFunction { error: failure, .. } = &**error
                        else {
                            panic!("original live failure: {error:?}")
                        };
                        let FailureKind::CallFailed(payload) = &failure.kind else {
                            panic!("opaque original failure")
                        };
                        assert!(Rc::ptr_eq(&payload.original, &activation.original));
                        assert_eq!(payload.interrupt, matches!(mode, Mode::LiveInterrupt));
                    }
                }
                assert!(!study.0.join("generated").exists());
            }
        }

        #[test]
        fn original_graph_host_failure_visits_keep_stage_identity_after_other_owners_drop() {
            for (mode, stage) in [
                (Mode::Lock, "lock"),
                (Mode::CaseInterrupt, "conformance"),
                (Mode::LiveFailure, "derivation"),
                (Mode::LiveInterrupt, "derivation"),
            ] {
                let study = Study::new();
                study.rounded();
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, mode);
                let original = Rc::clone(&activation.original);
                let attempt = build.build(&mut activation, &mut Report::default(), scope());
                let entered = attempt.graph.nodes.len();
                drop(activation);
                drop(build);
                fs::remove_dir_all(&study.0).unwrap();
                for _ in 0..2 {
                    let mut observed = Vec::new();
                    yamaa_adapters::producer_attempt::visit_host_failures(
                        &attempt,
                        |stage, error| {
                            observed.push((stage, error));
                        },
                    );
                    assert_eq!(observed.len(), 1);
                    assert_eq!(observed[0].0, stage);
                    assert!(Rc::ptr_eq(&observed[0].1.original, &original));
                    assert_eq!(
                        observed[0].1.interrupt,
                        matches!(mode, Mode::CaseInterrupt | Mode::LiveInterrupt)
                    );
                }
                assert_eq!(attempt.graph.nodes.len(), entered);
            }
        }

        #[test]
        fn graph_failure_visits_leave_native_unwind_and_success_as_original_boundaries() {
            for mode in [Mode::Pass, Mode::Unwind] {
                let study = Study::new();
                study.rounded();
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, mode);
                let unwind = Arc::clone(&activation.unwind);
                let attempt = build.build(&mut activation, &mut Report::default(), scope());
                let mut count = 0;
                yamaa_adapters::producer_attempt::visit_host_failures(&attempt, |_, _| count += 1);
                assert_eq!(count, 0);
                if matches!(mode, Mode::Unwind) {
                    let Err(BoundaryFailure::Unwind(payload)) = &attempt.boundary else {
                        panic!("original unwind remains held")
                    };
                    assert!(Arc::ptr_eq(
                        payload.downcast_ref::<Arc<()>>().unwrap(),
                        &unwind
                    ));
                    assert_eq!(attempt.graph.nodes.len(), 1);
                } else {
                    assert!(attempt.accepted());
                    assert_eq!(attempt.graph.nodes.len(), 2);
                }
            }
        }

        #[test]
        fn native_ingestion_verification_report_and_host_failures_keep_distinct_prefixes() {
            for mode in [0, 1, 3, 4] {
                let study = Study::new();
                study.rounded();
                match mode {
                    0 => study.write("raw/raw.csv", b"ID,VALUE\n1,not-a-float\n"),
                    1 => study.write(
                        "producer/layers/base.yaml",
                        format!("{PRODUCER}verifications:\n  - row_count: {{min: 99}}\n"),
                    ),
                    _ => {}
                }
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, Mode::Pass);
                let mut report = Report {
                    fail: mode == 3,
                    ..Default::default()
                };
                if mode == 4 {
                    activation.mode = Mode::LiveFailure;
                }
                let attempt = build.build(&mut activation, &mut report, scope());
                assert!(!attempt.accepted());
                assert!(!study.0.join("generated").exists());
                if mode == 5 {
                    assert_eq!(attempt.graph.nodes.len(), 2);
                    assert!(matches!(
                        attempt.graph.nodes[1].dataset.result,
                        Err(PortError::Run(_))
                    ));
                    assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
                } else {
                    assert_eq!(attempt.graph.nodes.len(), 1);
                }
                match mode {
                    0 | 1 => {
                        assert!(attempt.graph.nodes[0]
                            .dataset
                            .result
                            .as_ref()
                            .map_or(true, |e| e.result.is_err()));
                        assert!(report.entered.is_empty());
                    }
                    3 => assert!(matches!(
                        attempt.graph.nodes[0].output,
                        Err(CompleteError::Report("original report refusal"))
                    )),
                    4 => {
                        assert!(attempt
                            .resources
                            .iter()
                            .any(|(path, bytes)| path.ends_with("/raw/raw.csv")
                                && &**bytes == b"ID,VALUE\n1,1.234\n2,2.345\n"));
                        assert!(attempt.graph.nodes[0]
                            .dataset
                            .result
                            .as_ref()
                            .unwrap()
                            .result
                            .is_err());
                    }
                    _ => {}
                }
            }
        }

        #[cfg(unix)]
        #[test]
        fn native_build_preserves_canonical_unix_root_with_literal_backslashes() {
            let outer = Study::new();
            let path = outer.0.join(r"native\root");
            fs::create_dir(&path).unwrap();
            let study = Study(fs::canonicalize(path).unwrap());
            study.rounded();
            let mut build = study.build(Language::Python);
            let attempt = build.build(
                &mut Activation::new(&study, Mode::Pass),
                &mut Report::default(),
                scope(),
            );
            assert!(attempt.accepted());
            assert!(attempt
                .provenance
                .metadata()
                .nodes()
                .iter()
                .all(|n| n.identity().starts_with(&study.path(""))));
            assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
            assert!(!study.0.join("generated").exists());
        }
        #[test]
        fn final_report_rendering_refusal_prevents_retaining_or_consuming_producer_bytes() {
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut activation = Activation::new(&study, Mode::Pass);
            let mut report = Report {
                final_failure: true,
                ..Default::default()
            };
            let attempt = build.build(&mut activation, &mut report, scope());
            assert!(!attempt.accepted());
            assert_eq!(attempt.graph.nodes.len(), 1);
            assert!(attempt.graph.nodes[0]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .is_ok());
            assert!(matches!(
                attempt.graph.nodes[0].output,
                Err(CompleteError::Report("original report rendering refusal"))
            ));
            assert_eq!(report.entered.len(), 1);
            assert!(!study.0.join("generated").exists());
        }

        fn report_id() -> yamaa_adapters::specification_report::Identity<'static> {
            yamaa_adapters::specification_report::Identity {
                runtime: "python",
                runtime_version: "fixture-host",
                engine_version: "fixture-engine",
                example: "independent-rounded-producer",
                specification: "unused-entry-alias",
                base_directory: "unused-caller-base",
            }
        }

        #[test]
        fn producer_filter_type_failure_retains_the_complete_original_findings() {
            use yamaa_adapters::{
                file_application, file_preparation::FileSpecification, producer_result,
            };

            for (language, runtime, filter_count) in [
                (Language::Python, "python", 1),
                (Language::R, "r", 1),
                (Language::Python, "python", 2),
                (Language::R, "r", 2),
            ] {
                let study = Study::new();
                // This filter is genuinely invalid independently of inferred choice types:
                // EXSTDT is produced by to_date, while the right operand is a string.
                study.write("producer/p.yaml", "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {EX: {path: ../raw/ex.csv}}\nbase: EX\nintermediates:\n  - id: EXP\n    dataset: EX\n    derivations: {EXSTDT: {to_date: {source: EXSTDTC}}}\n    filter: \"EX.EXSTDT >= 'not-a-date'\"\n    no_match: null\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: EX.USUBJID}\n  - {name: TRTSDT, type: date, label: First Dose Date, derivation: EXP.EXSTDT}\noutput: {path: ../generated/adsl.csv, columns: [USUBJID, TRTSDT]}\n");
                if filter_count == 2 {
                    let path = study.0.join("producer/p.yaml");
                    let source = fs::read_to_string(&path).unwrap().replace(
                        "\ncolumns:\n",
                        "\n  - id: SECOND\n    dataset: EX\n    derivations: {EXSTDT: {to_date: {source: EXSTDTC}}}\n    filter: \"EX.EXSTDT >= 'also-not-a-date'\"\n    no_match: null\ncolumns:\n",
                    );
                    // SECOND must stay live under pruning (REQ-0638) for its
                    // filter finding to be reported: TRTSDT2 reads it and is
                    // itself an output column.
                    let source = source
                        .replace(
                            "  - {name: TRTSDT, type: date, label: First Dose Date, derivation: EXP.EXSTDT}\n",
                            "  - {name: TRTSDT, type: date, label: First Dose Date, derivation: EXP.EXSTDT}\n  - {name: TRTSDT2, type: date, label: Second Dose Date, derivation: SECOND.EXSTDT}\n",
                        )
                        .replace("columns: [USUBJID, TRTSDT]}", "columns: [USUBJID, TRTSDT, TRTSDT2]}");
                    fs::write(path, source).unwrap();
                }
                study.write("consumer/root.yaml", "schema_version: '1.0'\ndomain: ADEX\nkeys: [USUBJID]\ninput: {ADSL: {path: ../generated/adsl.csv, schema: ../producer/p.yaml}}\nbase: ADSL\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: ADSL.USUBJID}\n  - {name: TRTSDT, type: date, label: First Dose Date, derivation: ADSL.TRTSDT}\noutput: {path: ../generated/consumer.csv, columns: [USUBJID, TRTSDT]}\n");
                study.write("raw/ex.csv", "USUBJID,EXSTDTC\nS01,2024-01-01\n");
                study.write("generated/adsl.csv", b"untouched producer output");
                study.write("generated/consumer.csv", b"untouched consumer output");
                let id = || yamaa_adapters::specification_report::Identity {
                    runtime,
                    ..report_id()
                };
                let expected = yamaa_core::json::Value::Array((0..filter_count).map(|index| yamaa_adapters::json!({
                    "phase": "validation", "condition": "incompatible_input_type",
                    "requirement": "REQ-0190", "spec_paths": [format!("intermediates[{index}].filter")],
                    "context": {"left_type": "date", "right_type": "str"},
                })).collect());
                let prepared = FileSpecification::prepare(
                    Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                    "producer/p.yaml",
                )
                .unwrap();
                let mut direct = file_application::plain(prepared, id(), Scope::default()).unwrap();
                assert_eq!(direct.observations()["diagnostics"], expected);
                assert!(direct.output().is_none());
                assert!(direct.save().is_err());

                let mut build = study.build(language);
                let mut activation = Activation::new(&study, Mode::Pass);
                let attempt = build.build_owned_reported(&mut activation, id(), scope());
                assert!(!attempt.accepted());
                assert!(activation.trace.is_empty());
                assert_eq!(attempt.graph.nodes.len(), 1);
                let producer_node = attempt.graph.nodes[0].node;
                let producer = build.provenance().metadata().nodes()[producer_node]
                    .identity()
                    .to_owned();
                assert!(producer.ends_with("/producer/p.yaml"));
                assert!(
                    matches!(attempt.graph.outcome, Err(Failure::Node { node }) if node == producer_node)
                );
                assert!(matches!(
                    &attempt.graph.nodes[0].dataset.result,
                    Err(PortError::Run(yamaa_core::specification_run::RunError::Bind(
                        yamaa_core::specification::BindError::Invalid(findings)
                    ))) if findings.len() == filter_count
                ));
                drop(build);
                for path in ["producer/p.yaml", "consumer/root.yaml", "raw/ex.csv"] {
                    fs::remove_file(study.0.join(path)).unwrap();
                }
                let mut result = producer_result::prepare_owned_classified(&attempt, id(), &[])
                    .unwrap_or_else(|error| {
                        panic!("{runtime}: typed producer filter cause was refused: {error:?}")
                    });
                // The whole-build union is entry-relative (#2713): the
                // producer's findings carry the schema link that reaches
                // them, while the producer node's own report section keeps
                // its local paths beside its specification name.
                let merged = yamaa_core::json::Value::Array((0..filter_count).map(|index| yamaa_adapters::json!({
                    "phase": "validation", "condition": "incompatible_input_type",
                    "requirement": "REQ-0190", "spec_paths": [format!("input.ADSL.schema.intermediates[{index}].filter")],
                    "context": {"left_type": "date", "right_type": "str"},
                })).collect());
                assert_eq!(result.observations()["diagnostics"], merged);
                assert_eq!(result.observations()["nodes"][0]["diagnostics"], expected);
                assert_eq!(result.observations()["nodes"][0]["specification"], producer);
                assert_eq!(result.issues().len(), filter_count);
                for (index, issue) in result.issues().iter().enumerate() {
                    assert_eq!(issue.condition, "incompatible_input_type");
                    assert_eq!(issue.requirement, Some("REQ-0190".into()));
                    assert_eq!(
                        issue.spec_paths,
                        [format!("input.ADSL.schema.intermediates[{index}].filter")]
                    );
                }
                assert_eq!(
                    producer_result::prepare_owned_classified(&attempt, id(), &[])
                        .unwrap()
                        .observations(),
                    result.observations(),
                );
                assert!(result.output().is_none() && result.output_table().is_none());
                for _ in 0..2 {
                    assert!(result.save().is_err());
                }
                assert_eq!(
                    fs::read(study.0.join("generated/adsl.csv")).unwrap(),
                    b"untouched producer output"
                );
                assert_eq!(
                    fs::read(study.0.join("generated/consumer.csv")).unwrap(),
                    b"untouched consumer output"
                );
                assert_eq!(activation.trace, Vec::<String>::new());
            }
        }

        #[test]
        fn linked_producer_binding_findings_carry_the_schema_link_prefix() {
            use yamaa_adapters::producer_result::{self, RootResult};

            fn build_at(study: &Study, entry: &str) -> RootResult {
                let mut build = FileGraph::prepare(
                    Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                    entry,
                    environment(Language::Python),
                    shipped_schema::capture().unwrap(),
                )
                .unwrap_or_else(|error| panic!("graph fixture: {:?}", error.error()))
                .into_build();
                let attempt = build.build_owned_reported(
                    &mut Activation::new(study, Mode::Pass),
                    report_id(),
                    scope(),
                );
                producer_result::prepare_owned_classified(&attempt, report_id(), &[])
                    .unwrap_or_else(|error| panic!("root projection refused: {error:?}"))
            }
            fn producer_yaml(when: &str) -> String {
                format!(
                    "schema_version: '1.0'\ndomain: BASE\nkeys: [USUBJID]\ninput: {{RAW: {{path: ../raw/raw.csv, types: {{USUBJID: str, WEIGHT: float}}}}}}\nbase: RAW\ncolumns:\n  - {{name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}}\n  - {{name: WEIGHT, type: float, label: Weight, derivation: RAW.WEIGHT}}\n  - {{name: WTFL, type: str, label: Weight Recorded, derivation: {{case: [{{when: \"{when}\", then: {{literal: \"Y\"}}}}]}}}}\noutput: {{path: ../generated/base.csv, columns: [USUBJID, WEIGHT, WTFL]}}\n"
                )
            }
            fn consumer_yaml(schema: &str, artifact: &str) -> String {
                format!(
                    "schema_version: '1.0'\ndomain: SUMM\nkeys: [USUBJID]\ninput: {{BASE: {{path: ../generated/{artifact}, schema: {schema}}}}}\nbase: BASE\ncolumns:\n  - {{name: USUBJID, type: str, label: Subject, derivation: BASE.USUBJID}}\n  - {{name: WEIGHT, type: float, label: Weight, derivation: BASE.WEIGHT}}\n  - {{name: WTFL, type: str, label: Weight Recorded, derivation: {{case: [{{when: \"WEIGHT IS NOT NULL\", then: {{literal: \"Y\"}}}}]}}}}\noutput: {{path: ../generated/summ.csv, columns: [USUBJID, WEIGHT, WTFL]}}\n"
                )
            }
            fn study_with(when: &str, chain: bool) -> Study {
                let study = Study::new();
                study.write("raw/raw.csv", "USUBJID,WEIGHT\nA1,72.5\nA2,\n");
                study.write("producer/base.yaml", producer_yaml(when));
                if chain {
                    study.write(
                        "mid/mid.yaml",
                        consumer_yaml("../producer/base.yaml", "base.csv")
                            .replace("domain: SUMM", "domain: MID")
                            .replace("../generated/summ.csv", "../generated/mid.csv"),
                    );
                    study.write(
                        "consumer/root.yaml",
                        consumer_yaml("../mid/mid.yaml", "mid.csv"),
                    );
                } else {
                    study.write(
                        "consumer/root.yaml",
                        consumer_yaml("../producer/base.yaml", "base.csv"),
                    );
                }
                study
            }
            let row = |paths: &[&str]| {
                yamaa_adapters::json!({
                    "phase": "validation", "condition": "incompatible_input_type",
                    "requirement": "REQ-0190", "spec_paths": paths,
                    "context": {"left_type": "float", "right_type": "str"},
                })
            };

            // The issue's example: the producer compares a float with text.
            // Its own build keeps the authored path; the consumer's row is
            // prefixed with the link, and the producer node's own report
            // section keeps its local paths.
            let study = study_with("WEIGHT > 'heavy'", false);
            let direct = build_at(&study, "producer/base.yaml");
            assert!(direct.output().is_none());
            assert_eq!(
                direct.observations()["diagnostics"],
                yamaa_core::json::Value::Array(vec![row(&[
                    "columns.WTFL.derivation.case[0].when"
                ])])
            );
            let consumer = build_at(&study, "consumer/root.yaml");
            assert!(consumer.output().is_none());
            assert_eq!(consumer.issues().len(), 1);
            assert_eq!(
                consumer.issues()[0].spec_paths,
                ["input.BASE.schema.columns.WTFL.derivation.case[0].when"]
            );
            assert_eq!(
                consumer.observations()["diagnostics"],
                yamaa_core::json::Value::Array(vec![row(&[
                    "input.BASE.schema.columns.WTFL.derivation.case[0].when"
                ])])
            );
            let observations = consumer.observations();
            let producer_section = observations["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| {
                    node["specification"]
                        .as_str()
                        .is_some_and(|identity| identity.ends_with("producer/base.yaml"))
                })
                .expect("producer node section");
            assert_eq!(
                producer_section["diagnostics"],
                yamaa_core::json::Value::Array(vec![row(&[
                    "columns.WTFL.derivation.case[0].when"
                ])])
            );

            // The unknown-field variant prefixes the same way.
            let study = study_with("HEIGHT IS NOT NULL", false);
            let consumer = build_at(&study, "consumer/root.yaml");
            assert_eq!(consumer.issues().len(), 1);
            assert_eq!(consumer.issues()[0].condition, "unknown_field");
            assert_eq!(consumer.issues()[0].requirement, Some("REQ-0189".into()));
            assert_eq!(
                consumer.issues()[0].spec_paths,
                ["input.BASE.schema.columns.WTFL.derivation.case[0].when"]
            );
            assert_eq!(consumer.issues()[0].context, "{\"identifier\":\"HEIGHT\"}");

            // A three-level chain accumulates one link segment per hop, and
            // the middle specification as entry carries exactly one.
            let study = study_with("WEIGHT > 'heavy'", true);
            let middle = build_at(&study, "mid/mid.yaml");
            assert_eq!(
                middle.issues()[0].spec_paths,
                ["input.BASE.schema.columns.WTFL.derivation.case[0].when"]
            );
            let consumer = build_at(&study, "consumer/root.yaml");
            assert_eq!(consumer.issues().len(), 1);
            assert_eq!(
                consumer.issues()[0].spec_paths,
                ["input.BASE.schema.input.BASE.schema.columns.WTFL.derivation.case[0].when"]
            );

            // A valid producer: both builds succeed and no row is prefixed
            // or invented.
            let study = study_with("WEIGHT IS NOT NULL", false);
            let direct = build_at(&study, "producer/base.yaml");
            assert!(direct.issues().is_empty());
            assert!(direct.output().is_some());
            let consumer = build_at(&study, "consumer/root.yaml");
            assert!(consumer.issues().is_empty());
            assert!(consumer.output().is_some());
        }

        #[test]
        fn linked_producer_contract_findings_keep_their_single_link_prefix() {
            // The producer binds, but its WTFL column has no label, so the
            // consumer's contract check (REQ-0534) reports it. That finding
            // is authored from the consumer's perspective and already
            // carries exactly one link segment; #2713 must neither change
            // it nor prefix it a second time.
            let study = Study::new();
            study.write("raw/raw.csv", "USUBJID,WEIGHT\nA1,72.5\nA2,\n");
            study.write(
                "producer/base.yaml",
                "schema_version: '1.0'\ndomain: BASE\nkeys: [USUBJID]\ninput: {RAW: {path: ../raw/raw.csv, types: {USUBJID: str, WEIGHT: float}}}\nbase: RAW\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n  - {name: WEIGHT, type: float, label: Weight, derivation: RAW.WEIGHT}\n  - {name: WTFL, type: str, derivation: {case: [{when: \"WEIGHT IS NOT NULL\", then: {literal: \"Y\"}}]}}\noutput: {path: ../generated/base.csv, columns: [USUBJID, WEIGHT, WTFL]}\n",
            );
            study.write(
                "consumer/root.yaml",
                "schema_version: '1.0'\ndomain: SUMM\nkeys: [USUBJID]\ninput: {BASE: {path: ../generated/base.csv, schema: ../producer/base.yaml}}\nbase: BASE\ncolumns:\n  - {name: USUBJID, type: str, label: Subject, derivation: BASE.USUBJID}\n  - {name: WEIGHT, type: float, label: Weight, derivation: BASE.WEIGHT}\n  - {name: WTFL, type: str, label: Weight Recorded, derivation: BASE.WTFL}\noutput: {path: ../generated/summ.csv, columns: [USUBJID, WEIGHT, WTFL]}\n",
            );
            // Graph admission refuses the build with the original REQ-0534
            // diagnostic, exactly as authored from the consumer.
            let rejected = match FileGraph::prepare(
                Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                "consumer/root.yaml",
                environment(Language::Python),
                shipped_schema::capture().unwrap(),
            ) {
                Ok(_) => panic!("invalid producer contract must refuse the graph"),
                Err(error) => error,
            };
            let yamaa_adapters::file_graph::Error::Graph(
                yamaa_adapters::producer_graph::Error::Graph(
                    yamaa_core::producer_graph::Error::Admission {
                        error: yamaa_core::producer_admission::Error::Invalid(diagnostics),
                        ..
                    },
                ),
            ) = rejected.error()
            else {
                panic!("contract admission refusal: {:?}", rejected.error());
            };
            assert_eq!(diagnostics.len(), 1);
            let diagnostic = &diagnostics[0];
            assert_eq!(
                diagnostic.code,
                yamaa_core::diagnostic::ConditionCode::ProducerInvalidContract
            );
            assert_eq!(
                diagnostic.spec_paths,
                ["input.BASE.schema.columns.WTFL.label"]
            );
            assert_eq!(
                diagnostic.context["dataset"],
                yamaa_core::json::Value::String("BASE".into())
            );
            assert_eq!(
                diagnostic.context["field"],
                yamaa_core::json::Value::String("WTFL".into())
            );
            assert_eq!(
                diagnostic.context["reason"],
                yamaa_core::json::Value::String("label".into())
            );
            // The static-check facade surfaces the same row as an issue.
            let issues = yamaa_adapters::file_application::check(
                yamaa_core::domain_entry::Request {
                    specification: &study.path("consumer/root.yaml"),
                    environment: None,
                },
                report_id(),
            );
            let contract = issues
                .iter()
                .find(|issue| issue.condition == "invalid_producer_contract")
                .unwrap_or_else(|| panic!("contract issue missing: {issues:?}"));
            assert_eq!(contract.requirement, Some("REQ-0534".into()));
            assert_eq!(
                contract.spec_paths,
                ["input.BASE.schema.columns.WTFL.label"]
            );
            let context: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(&contract.context).unwrap();
            assert_eq!(context["dataset"], "BASE");
            assert_eq!(context["field"], "WTFL");
            assert_eq!(context["reason"], "label");
        }

        #[test]
        fn public_root_result_keeps_independent_unrounded_truth_after_all_other_owners_and_files_drop(
        ) {
            use arrow_array::{Float64Array, Int64Array};
            use yamaa_adapters::producer_result;
            let study = Study::new();
            study.rounded();
            // Independent root precision witness: consumer references retain the
            // established 2.46/4.70 truth; this separate root column is not rounded.
            study.write("consumer/root.yaml", CONSUMER.replace(
        "output: {path:",
        "  - {name: ROOT_UNROUNDED, type: float, label: Root precision, derivation: {compute: {expr: 'VALUE + 0.004'}}}\noutput: {path:",
    ).replace("columns: [ID, VALUE], decimals: 2", "columns: [ROOT_UNROUNDED, ID, VALUE], decimals: 2"));
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Pass);
            let first = build.build_reported(&mut host, report_id(), scope());
            let second = build.build_reported(&mut host, report_id(), scope());
            assert_eq!(host.trace.len(), 12);
            assert_eq!(artifact(&first, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
            assert_eq!(
                artifact(&first, 1),
                b"ROOT_UNROUNDED,ID,VALUE\n2.46,1,2.46\n4.70,2,4.70\n"
            );
            drop(build);
            drop(host);
            fs::remove_dir_all(&study.0).unwrap();
            for attempt in [&first, &second] {
                let result =
                    producer_result::prepare_classified(attempt, report_id(), &[]).unwrap();
                assert!(result.issues().is_empty());
                assert_eq!(result.observations()["outcome"], "success");
                let table = result.output_table().unwrap();
                let columns = table.columns();
                assert_eq!(
                    columns
                        .iter()
                        .map(|field| field.name().as_str())
                        .collect::<Vec<_>>(),
                    ["ROOT_UNROUNDED", "ID", "VALUE"]
                );
                let batch = &table.batches()[0];
                assert_eq!(
                    batch
                        .column(0)
                        .as_any()
                        .downcast_ref::<Float64Array>()
                        .unwrap()
                        .iter()
                        .collect::<Vec<_>>(),
                    [Some(2.464), Some(4.704)]
                );
                assert_eq!(
                    batch
                        .column(1)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .unwrap()
                        .iter()
                        .collect::<Vec<_>>(),
                    [Some(1), Some(2)]
                );
                assert_eq!(
                    batch
                        .column(2)
                        .as_any()
                        .downcast_ref::<Float64Array>()
                        .unwrap()
                        .iter()
                        .collect::<Vec<_>>(),
                    [Some(2.46), Some(4.70)]
                );
                let copied =
                    yamaa_adapters::public_table::PublicTable::decode(result.output().unwrap())
                        .unwrap();
                assert_eq!(
                    copied.batches()[0]
                        .column(0)
                        .as_any()
                        .downcast_ref::<Float64Array>()
                        .unwrap()
                        .iter()
                        .collect::<Vec<_>>(),
                    [Some(2.464), Some(4.704)]
                );
                assert_eq!(artifact(attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
            }
        }

        #[test]
        fn public_failed_graph_retains_prior_artifacts_as_evidence_without_root_output() {
            use yamaa_adapters::producer_result;
            let study = Study::new();
            study.rounded();
            study.write(
                "consumer/root.yaml",
                format!("{CONSUMER}verifications:\n  - row_count: {{min: 99}}\n"),
            );
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Pass);
            let attempt = build.build_reported(&mut host, report_id(), scope());
            assert!(!attempt.accepted());
            assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
            let before = host.trace.clone();
            let result = producer_result::prepare_classified(&attempt, report_id(), &[]).unwrap();
            assert!(result.output().is_none() && result.output_table().is_none());
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.observations()["outcome"], "failure");
            assert_eq!(host.trace, before);
            drop(build);
            drop(host);
            fs::remove_dir_all(&study.0).unwrap();
            assert!(
                producer_result::prepare_classified(&attempt, report_id(), &[])
                    .unwrap()
                    .output()
                    .is_none()
            );
        }

        #[test]
        fn public_root_projection_requires_complete_classification_and_preserves_original_host_failure(
        ) {
            use yamaa_adapters::{
                producer_report::ProjectionError, producer_result,
                project_function_diagnostics::HostDetails, project_report::Classified,
            };
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::LiveFailure);
            let original = Rc::clone(&host.original);
            let attempt = build.build_reported(&mut host, report_id(), scope());
            drop(build);
            drop(host);
            fs::remove_dir_all(&study.0).unwrap();
            let mut failures = vec![];
            yamaa_adapters::producer_attempt::visit_host_failures(&attempt, |_, error| {
                failures.push(error)
            });
            assert_eq!(failures.len(), 1);
            assert!(Rc::ptr_eq(&original, &failures[0].original));
            match producer_result::prepare_classified(&attempt, report_id(), &[]) {
                Err(producer_result::Error::Report(ProjectionError::Original {
                    attempt: held,
                    observations,
                })) => {
                    assert!(std::ptr::eq(&attempt, held));
                    assert_eq!(observations["outcome"], "failure");
                }
                _ => panic!("opaque original attempt must remain beside refusal"),
            }
            let facts = [Classified::Host {
                failure: failures[0],
                details: HostDetails::Exception {
                    class: "OriginalError",
                    message: "held failure",
                },
            }];
            let result =
                producer_result::prepare_classified(&attempt, report_id(), &facts).unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition, "function_call_failed");
            assert!(Rc::ptr_eq(&original, &failures[0].original));
        }

        #[test]
        fn public_root_result_refuses_contradictory_whole_attempts_before_output_copy() {
            use yamaa_adapters::{
                producer_report::ProjectionError, producer_result, specification_report::Error,
            };
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Pass);
            let mut attempt = build.build_reported(&mut host, report_id(), scope());
            let before = host.trace.clone();
            attempt.graph.nodes.swap(0, 1);
            assert!(matches!(
                producer_result::prepare_classified(&attempt, report_id(), &[]),
                Err(producer_result::Error::Report(ProjectionError::Report(
                    Error::InvalidObservation
                )))
            ));
            assert_eq!(host.trace, before);
            assert!(!study.0.join("generated").exists());
        }

        #[test]
        fn public_root_keeps_complete_large_truth_after_resources_drop() {
            use yamaa_adapters::producer_result;
            let study = Study::new();
            study.rounded();
            study.write(
                "producer/layers/base.yaml",
                PRODUCER
                    .replace("ID: int", "ID: str")
                    .replace("name: ID, type: int", "name: ID, type: str"),
            );
            let declarations = (0..59).map(|index| format!("  - {{name: EXTRA{index}, type: str, label: Extra value, derivation: {{literal: null}}}}\n")).collect::<String>();
            let names = std::iter::once("ID".to_owned())
                .chain(std::iter::once("VALUE".to_owned()))
                .chain((0..59).map(|index| format!("EXTRA{index}")))
                .collect::<Vec<_>>()
                .join(", ");
            study.write(
                "consumer/root.yaml",
                CONSUMER
                    .replace("name: ID, type: int", "name: ID, type: str")
                    .replace("output: {path:", &format!("{declarations}output: {{path:"))
                    .replace(
                        "columns: [ID, VALUE], decimals: 2",
                        &format!("columns: [{names}], decimals: 2"),
                    ),
            );
            study.write(
                "raw/raw.csv",
                format!(
                    "ID,VALUE\n{}",
                    (1..=23000)
                        .map(|index| format!("{index},1.234\n"))
                        .collect::<String>()
                ),
            );
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Pass);
            let attempt = build.build_reported(&mut host, report_id(), scope());
            assert!(attempt.accepted());
            let before = host.trace.clone();
            let result = producer_result::prepare_classified(&attempt, report_id(), &[])
                .unwrap_or_else(|_| panic!("accepted graph preserves its full output"));
            assert!(result.issues().is_empty());
            let output = result.output_table().unwrap();
            assert_eq!((output.rows(), output.columns().len()), (23_000, 61));
            let observations = result.observations();
            assert_eq!(observations["outcome"], "success");
            assert_eq!(observations["nodes"].as_array().unwrap().len(), 2);
            assert_eq!(observations["artifacts"].as_array().unwrap().len(), 2);
            assert_eq!(host.trace, before);
            assert!(!study.0.join("generated").exists());
            drop(build);
            drop(host);
            fs::remove_dir_all(&study.0).unwrap();
            assert!(attempt.accepted());
            let repeated = producer_result::prepare_classified(&attempt, report_id(), &[])
                .unwrap_or_else(|_| panic!("public projection owns its accepted truth"));
            assert_eq!(repeated.output(), result.output());
        }

        #[test]
        fn classified_graph_reports_keep_actual_host_causes_and_authored_origins_without_replay() {
            use yamaa_adapters::{
                producer_report::attempt_report_classified,
                project_function_diagnostics::HostDetails,
                project_lock::{Finding, Reason},
                project_report::Classified,
            };
            for (mode, condition) in [
                (Mode::Lock, "runtime_artifact_mismatch"),
                (Mode::BindingFailure, "project_environment_invalid"),
                (Mode::CaseFailure, "function_conformance_failed"),
                (Mode::CaseMismatch, "function_conformance_failed"),
                (Mode::LiveFailure, "function_call_failed"),
            ] {
                let study = Study::new();
                study.rounded();
                let mut build = study.build(Language::Python);
                let mut host = Activation::new(&study, mode);
                let original = Rc::clone(&host.original);
                let attempt = build.build_reported(&mut host, report_id(), scope());
                let trace = host.trace.clone();
                drop(host);
                drop(build);
                fs::remove_dir_all(&study.0).unwrap();
                let mut held = vec![];
                yamaa_adapters::producer_attempt::visit_host_failures(&attempt, |stage, error| {
                    held.push((stage, error))
                });
                let lock = [Finding {
                    package: "yamaa".into(),
                    reason: Reason::VersionMismatch,
                    expected: vec!["0.2.0".into()],
                    actual: Some("0.3.0".into()),
                }];
                let facts = held
                    .iter()
                    .map(|(stage, error)| {
                        assert!(Rc::ptr_eq(&original, &error.original));
                        if *stage == "lock" {
                            Classified::Lock {
                                failure: *error,
                                findings: &lock,
                            }
                        } else {
                            Classified::Host {
                                failure: *error,
                                details: HostDetails::Exception {
                                    class: "ActualHostError",
                                    message: "original quoted \"cause\"",
                                },
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let report = attempt_report_classified(&attempt, report_id(), &facts).unwrap();
                assert_eq!(report["outcome"], "failure");
                assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
                assert_eq!(report["diagnostics"][0]["condition"], condition);
                assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
                if !matches!(mode, Mode::LiveFailure) {
                    assert!(attempt.graph.nodes.is_empty());
                    assert_eq!(report["source_reads"], yamaa_adapters::json!([]));
                    assert_eq!(
                        report["diagnostics"][0]["context"]["source"],
                        "held/environment.yaml"
                    );
                    assert_eq!(
                        report["diagnostics"][0]["context"]["entry"],
                        "held/environment.yaml"
                    );
                } else {
                    assert_eq!(attempt.graph.nodes.len(), 1);
                    assert_eq!(
                        report["diagnostics"][0]["spec_paths"],
                        yamaa_adapters::json!(["columns.VALUE.derivation.function"])
                    );
                    assert_eq!(
                        report["diagnostics"][0]["context"]["keys"],
                        yamaa_adapters::json!([{"ID":1}])
                    );
                }
                assert_eq!(
                    attempt_report_classified(&attempt, report_id(), &facts).unwrap(),
                    report
                );
                assert_eq!(
                    trace
                        .iter()
                        .filter(|entry| entry.starts_with("invoke:"))
                        .count(),
                    if matches!(mode, Mode::LiveFailure) {
                        3
                    } else if matches!(mode, Mode::Lock | Mode::BindingFailure) {
                        0
                    } else {
                        2
                    }
                );
            }
        }

        #[test]
        fn graph_classification_refuses_unrelated_duplicate_wrong_stage_and_keeps_long_facts() {
            use yamaa_adapters::{
                producer_report::{attempt_report_classified, ProjectionError},
                project_function_diagnostics::HostDetails,
                project_report::Classified,
                specification_report::Error,
            };
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::LiveFailure);
            let attempt = build.build_reported(&mut host, report_id(), scope());
            let mut held = vec![];
            yamaa_adapters::producer_attempt::visit_host_failures(&attempt, |_, error| {
                held.push(error)
            });
            let error = held[0];
            let ordinary = |failure| Classified::Host {
                failure,
                details: HostDetails::Exception {
                    class: "ActualHostError",
                    message: "original",
                },
            };
            let unrelated = host.payload(false);
            for facts in [
                vec![ordinary(&unrelated)],
                vec![ordinary(error), ordinary(error)],
                vec![Classified::Lock {
                    failure: error,
                    findings: &[],
                }],
            ] {
                assert!(matches!(
                    attempt_report_classified(&attempt, report_id(), &facts),
                    Err(ProjectionError::Report(Error::InvalidObservation))
                ));
            }
            let message = "\\\"".repeat(131072);
            let facts = [Classified::Host {
                failure: error,
                details: HostDetails::Exception {
                    class: "ActualHostError",
                    message: &message,
                },
            }];
            let report = attempt_report_classified(&attempt, report_id(), &facts).unwrap();
            assert_eq!(report["diagnostics"][0]["context"]["host_message"], message);
            assert_eq!(attempt.graph.nodes.len(), 1);
            assert!(Rc::ptr_eq(&host.original, &error.original));
        }

        #[test]
        fn graph_host_classification_cannot_authorize_distinct_zero_sized_payloads_by_address() {
            use yamaa_adapters::{
                producer_report::{attempt_report_classified, ProjectionError},
                project_report::Classified,
                specification_report::Error,
            };
            struct Host;
            impl ActivationPort for Host {
                type Error = ();
                type Handle = ();
                fn verify_lock(
                    &mut self,
                    _: Language,
                    _: &LockReference,
                    _: &[ProjectFunctionIdentity],
                ) -> Result<(), ()> {
                    Err(())
                }
                fn bind(
                    &mut self,
                    _: &ProjectFunctionIdentity,
                    _: &LogicalSignature,
                ) -> Result<(), ()> {
                    unreachable!()
                }
                fn is_interrupt(&self, _: &()) -> bool {
                    false
                }
                fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<()>> {
                    unreachable!()
                }
            }
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let attempt = build.build_reported(&mut Host, report_id(), scope());
            let mut held = vec![];
            yamaa_adapters::producer_attempt::visit_host_failures(&attempt, |_, error| {
                held.push(error)
            });
            assert_eq!(held.len(), 1);
            let facts = [Classified::Lock {
                failure: held[0],
                findings: &[],
            }];
            assert!(matches!(
                attempt_report_classified(&attempt, report_id(), &facts),
                Err(ProjectionError::Report(Error::InvalidObservation))
            ));
            assert!(matches!(
                attempt_report_classified(&attempt, report_id(), &[]),
                Err(ProjectionError::Original { .. })
            ));
            assert!(attempt.graph.nodes.is_empty());
        }
        fn truth_table(
            specification: &str,
            stage: &str,
            name: &str,
            columns: &[&str],
            rows: &[&[&str]],
        ) -> yamaa_core::json::Value {
            use yamaa_adapters::json;
            let kinds = columns
                .iter()
                .map(|&column| if column == "ID" { "int" } else { "float" })
                .collect::<Vec<_>>();
            json!({"specification":specification,"stage":stage,"name":name,"columns":columns,"types":kinds,
        "rows":rows.iter().map(|row| row.iter().zip(&kinds).map(|(value, kind)| json!({"type":kind,"value":value})).collect::<Vec<_>>()).collect::<Vec<_>>()})
        }
        #[test]
        fn native_complete_reports_pin_independent_rounded_truth_and_union_after_all_owners_drop() {
            use yamaa_adapters::json;
            use yamaa_adapters::producer_report::attempt_report;
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Pass);
            let first = build.build_reported(&mut host, report_id(), scope());
            let second = build.build_reported(&mut host, report_id(), scope());
            assert!(first.accepted() && second.accepted());
            assert_eq!(host.trace.len(), 12);
            let producer = study.path("producer/p.yaml");
            let consumer = study.path("consumer/root.yaml");
            let raw_rows: &[&[&str]] = &[&["1", "3ff3be76c8b43958"], &["2", "4002c28f5c28f5c3"]];
            let rounded_rows: &[&[&str]] =
                &[&["1", "3ff3ae147ae147ae"], &["2", "4002cccccccccccd"]];
            let derived_rows: &[&[&str]] = &[
                &[
                    "1",
                    "3ff3ae147ae147ae",
                    "3ff3ae147ae147ae",
                    "4003ae147ae147ae",
                ],
                &[
                    "2",
                    "4002cccccccccccd",
                    "4002cccccccccccd",
                    "4012cccccccccccd",
                ],
            ];
            let tables = json!([
                truth_table(&producer, "source", "RAW", &["ID", "VALUE"], raw_rows),
                truth_table(&producer, "derived", "output", &["ID", "VALUE"], raw_rows),
                truth_table(&consumer, "source", "FIRST", &["ID", "VALUE"], rounded_rows),
                truth_table(
                    &consumer,
                    "source",
                    "SECOND",
                    &["ID", "VALUE"],
                    rounded_rows
                ),
                truth_table(
                    &consumer,
                    "derived",
                    "output",
                    &["ID", "FIRST_VALUE", "SECOND_VALUE", "VALUE"],
                    derived_rows
                ),
            ]);
            let activation = json!({"lock":"verified","bindings":[{"function":"id_float","call":"program.id_float","source":"held/environment.yaml","outcome":"bound"}],
            "tests":[
                {"function":"id_float","call":"program.id_float","source":"held/environment.yaml","case":"normal","args":{"x":{"float":"401c000000000000"}},"expected":{"float":"401c000000000000"},"comparison_decimals":4,"outcome":"passed","invoked":true,"actual_retained":true,"actual":{"float":"401c000000000000"}},
                {"function":"id_float","call":"program.id_float","source":"held/environment.yaml","case":"boundary","args":{"x":{"float":"0000000000000000"}},"expected":{"float":"0000000000000000"},"comparison_decimals":4,"outcome":"passed","invoked":true,"actual_retained":true,"actual":{"float":"0000000000000000"}},
                {"function":"id_float","call":"program.id_float","source":"held/environment.yaml","case":"missing","args":{"x":{"missing":null}},"expected":{"missing":null},"comparison_decimals":4,"outcome":"passed","invoked":false,"actual_retained":true,"actual":{"missing":null}}
            ]});
            drop(build);
            drop(host);
            fs::remove_dir_all(&study.0).unwrap();
            for (attempt, created) in [(&first, 1), (&second, 0)] {
                let observed = attempt_report(attempt, report_id()).unwrap();
                let expected = json!({"report_version":"0.3.0-draft","runtime":"python","backend":"rust","runtime_version":"fixture-host","engine_version":"fixture-engine","example":"independent-rounded-producer","entry":"consumer/root.yaml","outcome":"success","activation":activation,"environment":{"source":"held/environment.yaml","lock_source":"held/lock"},
            "nodes":[{"specification":producer,"outcome":"success","diagnostics":[],"unsupported":[],"handler_counts":[]},{"specification":consumer,"outcome":"success","diagnostics":[],"unsupported":[],"handler_counts":[]}],
            "artifacts":[
                {"name":"producer","profile":"csv","columns":["ID","VALUE"],"types":["int","float"],"row_count":2,"records":["ID,VALUE","1,1.23","2,2.35"],"byte_length":23,"content":"ID,VALUE\n1,1.23\n2,2.35\n","specification":producer},
                {"name":"root","profile":"csv","columns":["ID","VALUE"],"types":["int","float"],"row_count":2,"records":["ID,VALUE","1,2.46","2,4.70"],"byte_length":23,"content":"ID,VALUE\n1,2.46\n2,4.70\n","specification":consumer}],
            "tables":tables,"diagnostics":[],"unsupported":[],"handler_counts":[],"verifications":[],"callbacks":[],"error":null,
            "source_reads":[{"base_directory":study.path("producer"),"path":"../raw/raw.csv","outcome":"captured","condition":null,"snapshots_created":created,"specification":producer}]});
                assert_eq!(observed.observations_value(false), expected);
                assert_eq!(
                    attempt_report(attempt, report_id())
                        .unwrap()
                        .observations_value(false),
                    expected
                );
                let bytes = attempt.graph.nodes[1]
                    .output
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .artifact()
                    .unwrap()
                    .bytes();
                assert_eq!(bytes, b"ID,VALUE\n1,2.46\n2,4.70\n");
            }
        }

        #[test]
        fn complete_failed_reports_keep_entered_producer_and_consumer_check_and_source_prefixes() {
            use yamaa_adapters::json;
            use yamaa_adapters::producer_report::attempt_report;
            for mode in 0..4 {
                let study = Study::new();
                study.rounded();
                match mode {
                    0 => study.write(
                        "producer/layers/base.yaml",
                        format!("{PRODUCER}verifications:\n  - row_count: {{min: 99}}\n"),
                    ),
                    1 => study.write(
                        "consumer/root.yaml",
                        format!("{CONSUMER}verifications:\n  - row_count: {{min: 99}}\n"),
                    ),
                    2 => fs::remove_file(study.0.join("raw/raw.csv")).unwrap(),
                    3 => study.write("raw/raw.csv", b"ID,VALUE\n1,bad\n"),
                    _ => unreachable!(),
                }
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, Mode::Pass);
                let attempt = build.build_reported(&mut activation, report_id(), scope());
                assert!(!attempt.accepted());
                let before = activation.trace.clone();
                drop(build);
                fs::remove_dir_all(&study.0).unwrap();
                let value = attempt_report(&attempt, report_id()).unwrap();
                assert_eq!(activation.trace, before);
                assert_eq!(value["outcome"], "failure");
                assert_eq!(
                    value["nodes"].as_array().unwrap().len(),
                    if mode == 1 { 2 } else { 1 }
                );
                assert_eq!(
                    value["artifacts"].as_array().unwrap().len(),
                    if mode == 1 { 1 } else { 0 }
                );
                assert_eq!(value["diagnostics"].as_array().unwrap().len(), 1);
                let finding = &value["diagnostics"][0];
                match mode {
                    0 | 1 => {
                        assert_eq!(finding["condition"], "row_count_failed");
                        assert_eq!(finding["spec_paths"], json!(["verifications[0].row_count"]));
                        assert_eq!(finding["context"]["count"], 2);
                        assert_eq!(value["verifications"].as_array().unwrap().len(), 1);
                    }
                    2 => {
                        assert_eq!(finding["condition"], "resource_path_missing");
                        assert_eq!(finding["requirement"], "REQ-0785");
                        assert_eq!(finding["context"]["path"], "../../raw/raw.csv");
                        assert!(value["source_reads"].as_array().unwrap().is_empty());
                    }
                    3 => {
                        assert_eq!(finding["condition"], "field_parse_failed");
                        assert_eq!(finding["requirement"], "REQ-0536");
                        assert_eq!(finding["context"]["value"], "bad");
                        assert_eq!(value["source_reads"][0]["outcome"], "captured");
                    }
                    _ => unreachable!(),
                }
                assert_eq!(attempt_report(&attempt, report_id()).unwrap(), value);
            }
        }

        #[test]
        fn complete_projection_refusal_keeps_original_opaque_interrupt_unwind_and_actual_prefix() {
            use yamaa_adapters::producer_report::{attempt_report, ProjectionError};
            for mode in [
                Mode::Lock,
                Mode::CaseInterrupt,
                Mode::LiveFailure,
                Mode::LiveInterrupt,
                Mode::Unwind,
            ] {
                let study = Study::new();
                study.rounded();
                let mut build = study.build(Language::Python);
                let mut activation = Activation::new(&study, mode);
                let attempt = build.build_reported(&mut activation, report_id(), scope());
                let original = Rc::clone(&activation.original);
                let unwind = Arc::clone(&activation.unwind);
                let before = activation.trace.clone();
                drop(build);
                fs::remove_dir_all(&study.0).unwrap();
                let Err(ProjectionError::Original {
                    attempt: held,
                    observations,
                }) = attempt_report(&attempt, report_id())
                else {
                    panic!("original failure must accompany projection refusal");
                };
                assert!(std::ptr::eq(held, &attempt));
                assert_eq!(activation.trace, before);
                assert_eq!(observations["outcome"], "failure");
                assert!(observations["artifacts"].as_array().unwrap().is_empty());
                assert_eq!(
                    observations["nodes"].as_array().unwrap().len(),
                    attempt.graph.nodes.len()
                );
                match mode {
                    Mode::Lock => assert!(
                        matches!(&held.graph.outcome, Err(Failure::Activation(ActivationFailure::Lock(e))) if Rc::ptr_eq(&original, &e.original))
                    ),
                    Mode::Unwind => assert!(
                        matches!(&held.boundary, Err(BoundaryFailure::Unwind(payload)) if Arc::ptr_eq(payload.downcast_ref::<Arc<()>>().unwrap(), &unwind))
                    ),
                    Mode::CaseInterrupt => {
                        assert_eq!(
                            observations["activation"]["tests"]
                                .as_array()
                                .unwrap()
                                .len(),
                            2
                        );
                        assert_eq!(
                            observations["activation"]["tests"][1]["outcome"],
                            "interrupted"
                        );
                    }
                    _ => {
                        let error = held.graph.nodes[0]
                            .dataset
                            .result
                            .as_ref()
                            .unwrap()
                            .result
                            .as_ref()
                            .unwrap_err();
                        assert!(
                            matches!(error.as_ref(), ExecutionError::ProjectFunction { error, .. } if matches!(&error.kind, FailureKind::CallFailed(e) if Rc::ptr_eq(&original, &e.original)))
                        );
                        assert_eq!(observations["source_reads"].as_array().unwrap().len(), 1);
                        assert_eq!(observations["tables"].as_array().unwrap().len(), 1);
                    }
                }
            }
        }

        #[test]
        fn report_context_pointer_and_table_count_admission_precede_table_authority_and_clear_old_selection(
        ) {
            use yamaa_adapters::{producer_report::NativeReport, specification_report::Error};
            use yamaa_core::specification_run::{CapturedAttempt, CapturedSource};
            struct Bomb {
                schema: yamaa_core::table::TableSchema,
                reads: Rc<std::cell::Cell<usize>>,
            }
            impl TableAccess for Bomb {
                type Error = Payload;
                fn schema(&self) -> &yamaa_core::table::TableSchema {
                    &self.schema
                }
                fn row_count(&self) -> usize {
                    usize::MAX
                }
                fn cell(
                    &self,
                    _: usize,
                    _: usize,
                ) -> Result<ValueRef<'_>, yamaa_core::table::CellError<Payload>> {
                    self.reads.set(self.reads.get() + 1);
                    panic!("count admission must precede cell authority")
                }
            }
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let unrelated = study.build(Language::Python);
            let mut attempt = build.build_reported(
                &mut Activation::new(&study, Mode::Pass),
                report_id(),
                scope(),
            );
            assert!(attempt.accepted());
            let mut report = NativeReport::new(build.provenance(), report_id());
            let node = &build.provenance().metadata().nodes()[1];
            report
                .select(node, &attempt.graph.nodes[0].dataset)
                .unwrap();
            assert!(matches!(
                report.select(
                    &unrelated.provenance().metadata().nodes()[1],
                    &attempt.graph.nodes[0].dataset
                ),
                Err(Error::InvalidObservation)
            ));
            assert!(matches!(report.failure(), Err(Error::InvalidObservation)));
            let mut entered = attempt.graph.nodes.remove(0);
            let source = entered.dataset.sources.remove(0);
            let reads = Rc::new(std::cell::Cell::new(0));
            let fake = CapturedAttempt::<(), (), Bomb> {
                sources: vec![CapturedSource {
                    read: source.read,
                    snapshot: source.snapshot,
                    owned_snapshot: source.owned_snapshot,
                    table: Some(Bomb {
                        schema: source.table.unwrap().schema().clone(),
                        reads: Rc::clone(&reads),
                    }),
                }],
                result: Ok(entered.dataset.result.map_err(|_| ()).unwrap()),
            };
            assert!(matches!(
                report.select(node, &fake),
                Err(Error::ResourceLimit(refusal))
                    if refusal.resource == "table_rows"
                        && refusal.limit == yamaa_core::limits::Limits::TABLE_ROWS
                        && refusal.actual == Some(usize::MAX)
            ));
            assert_eq!(reads.get(), 0);
            assert!(matches!(report.failure(), Err(Error::InvalidObservation)));
        }

        #[test]
        fn whole_report_rejects_contradictory_actual_activation_and_node_order_without_replay() {
            use yamaa_adapters::{
                producer_report::{attempt_report, ProjectionError},
                specification_report::Error,
            };
            let study = Study::new();
            study.rounded();
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Pass);
            let mut attempt = build.build_reported(&mut host, report_id(), scope());
            let before = host.trace.clone();
            attempt.graph.activation.tests[0].actual = Some(Value::float(99.0));
            assert!(matches!(
                attempt_report(&attempt, report_id()),
                Err(ProjectionError::Report(Error::InvalidObservation))
            ));
            attempt.graph.activation.tests[0].actual = Some(Value::float(7.0));
            attempt.graph.nodes.swap(0, 1);
            assert!(matches!(
                attempt_report(&attempt, report_id()),
                Err(ProjectionError::Report(Error::InvalidObservation))
            ));
            assert_eq!(host.trace, before);
        }

        #[test]
        fn native_reports_keep_complete_long_borrowed_identity_without_replay() {
            let study = Study::new();
            study.write(
                "producer/p.yaml",
                passthrough(
                    "{RAW: {path: ../raw/raw.csv, types: {ID: int, VALUE: str}}}",
                    "RAW",
                    "../generated/producer.csv",
                    "str",
                ),
            );
            study.write(
                "consumer/root.yaml",
                passthrough(
                    "{FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml}}",
                    "FIRST",
                    "../generated/root.csv",
                    "str",
                ),
            );
            let text = "small";
            study.write("raw/raw.csv", format!("ID,VALUE\n1,{text}\n"));
            let oversized_runtime = "x".repeat(65_537);
            let identity = yamaa_adapters::specification_report::Identity {
                runtime_version: &oversized_runtime,
                ..report_id()
            };
            let mut build = study.build(Language::Python);
            let mut host = Activation::new(&study, Mode::Lock);
            let attempt = build.build_reported(&mut host, identity, scope());
            assert!(attempt.accepted());
            assert!(host.trace.is_empty());
            assert_eq!(attempt.graph.nodes.len(), 2);
            assert!(attempt.graph.nodes[0]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .is_ok());
            assert_eq!(
                attempt.graph.nodes[0].dataset.sources[0]
                    .table
                    .as_ref()
                    .unwrap()
                    .cell(0, 1)
                    .unwrap(),
                ValueRef::Str(text)
            );
            // Reprojection borrows the held attempt without entering any source or host.
            let report = yamaa_adapters::producer_report::attempt_report(
                &attempt,
                yamaa_adapters::specification_report::Identity {
                    runtime_version: &oversized_runtime,
                    ..report_id()
                },
            )
            .unwrap()
            .observations_value(false);
            assert_eq!(report["outcome"], "success");
            assert_eq!(report["runtime_version"], oversized_runtime);
            assert_eq!(report["artifacts"].as_array().unwrap().len(), 2);
            assert_eq!(
                report["tables"][0]["rows"][0][1],
                yamaa_adapters::json!({"type":"str","value":text})
            );
            assert!(!study.0.join("generated").exists());
        }

        #[test]
        fn original_native_selection_builds_the_retained_rounded_alias_workflow_without_recapture()
        {
            use yamaa_adapters::file_workflow::{self, Selected};
            let study = Study::new();
            study.rounded();
            // This preparation fixture independently supplies the real original native
            // environment/lock closure rather than the supplied in-memory test owner.
            study.write("environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n  id_float:\n    function: program.id_float\n    description: Float identity.\n    params: [{name: x, type: float}]\n    returns: float\n    tests:\n      - {id: normal, covers: [normal, numeric-comparison], args: {x: 7.0}, result: 7.0}\n      - {id: boundary, covers: [boundary], args: {x: 0.0}, result: 0.0}\n      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n");
            study.write("uv.lock", "version = 1\npackage = []\n");
            let Selected::Producer(graph) = file_workflow::select_with_resources(
                Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                "consumer/root.yaml",
                "environment.yaml",
                Language::Python,
                shipped_schema::capture().unwrap(),
                shipped_schema::capture_environment().unwrap(),
            )
            .unwrap() else {
                panic!("producer capability");
            };
            assert_eq!(graph.capture_reads(), 5); // environment, lock, entry, producer, parent
            let root = Arc::clone(&graph.graph().nodes()[0].document);
            let producer = Arc::clone(&graph.graph().nodes()[1].document);
            assert!(Arc::ptr_eq(
                &graph.graph().nodes()[0].producers[0].document,
                &graph.graph().nodes()[0].producers[1].document
            ));
            let mut build = (*graph).into_build();
            assert!(Arc::ptr_eq(&root, &build.provenance().documents()[0]));
            assert!(Arc::ptr_eq(&producer, &build.provenance().documents()[1]));
            let mut activation = Activation::new(&study, Mode::Pass);
            let attempt = build.build_reported(&mut activation, report_id(), scope());
            let execution = attempt.graph.nodes[0]
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap();
            for (row, expected) in [1.234, 2.345].into_iter().enumerate() {
                let ValueRef::Float(value) = execution.dataset.cell(row, 1).unwrap() else {
                    panic!("separate unrounded producer truth");
                };
                assert_eq!(value.get(), expected);
            }
            assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
            assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
            assert_eq!(attempt.graph.nodes.len(), 2);
            assert_eq!(attempt.graph.nodes[0].dataset.sources.len(), 1);
            assert_eq!(attempt.graph.nodes[1].dataset.sources.len(), 2);
            assert!(!study.0.join("generated/producer.csv").exists());
            assert!(!study.0.join("generated/root.csv").exists());
            drop(build);
            drop(root);
            drop(producer);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(artifact(&attempt, 0), b"ID,VALUE\n1,1.23\n2,2.35\n");
            assert_eq!(artifact(&attempt, 1), b"ID,VALUE\n1,2.46\n2,4.70\n");
        }

        #[test]
        fn key_failures_retain_the_verification_log_and_report_as_output_findings() {
            use yamaa_adapters::producer_result::prepare_owned_classified;
            // Key failures are output findings, not verification checks: feeding
            // them to the verification-phase projection used to fail
            // `prepare_diagnostics`, dropping this node's verification log.
            for (csv, condition) in [
                ("ID,GRP,VALUE\n1,a,10\n1,b,20\n", "duplicate_key"),
                ("ID,GRP,VALUE\n,a,10\n1,b,20\n", "missing_key"),
            ] {
                let study = Study::new();
                fs::create_dir(study.0.join("generated")).unwrap();
                study.write("input.csv", csv);
                study.write(
            "root.yaml",
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, GRP: str, VALUE: int}}}\nrows:\n  - id: grouped\n    dataset: SRC\n    group_by: [SRC.GRP]\n    derivations: {ID: {aggregate: 'MAX(SRC.ID)'}, GRP: SRC.GRP, VALUE: {aggregate: 'MAX(SRC.VALUE)'}}\ncolumns:\n  - {name: ID, type: int, label: Identifier}\n  - {name: GRP, type: str, label: Group}\n  - name: VALUE\n    type: int\n    label: Value\n    verifications: [{not_missing: {id: value-present}}]\noutput: {path: generated/root.csv, columns: [ID, GRP, VALUE], verification_log: generated/root-checks.csv, warning_log: generated/root-warnings.csv}\n",
        );
                let environment = project_source::prepare(
                    shipped_schema::capture_environment().unwrap(),
                    Source {
                        identity: "held/environment.yaml".into(),
                        bytes: b"schema_version: '1.0'\n".to_vec(),
                    },
                    Language::Python,
                    &mut LockCapture {
                        language: Language::Python,
                    },
                )
                .unwrap()
                .into_owned();
                let mut build = FileGraph::prepare(
                    Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                    "root.yaml",
                    environment,
                    shipped_schema::capture().unwrap(),
                )
                .unwrap_or_else(|error| panic!("graph fixture: {:?}", error.error()))
                .into_build();
                let mut activation = Activation::new(&study, Mode::Pass);
                let owned = build.build_owned_reported(&mut activation, report_id(), scope());
                assert!(!owned.accepted(), "{condition}");
                assert_eq!(owned.graph.nodes.len(), 1, "{condition}");
                assert!(activation.trace.is_empty(), "{condition}");
                let mut result = prepare_owned_classified(&owned, report_id(), &[]).unwrap();
                assert!(result.output().is_none(), "{condition}");
                assert!(result.verification_log().is_some(), "{condition}");
                let table = result.log_table(false).unwrap();
                assert_eq!(table.rows(), 1, "{condition}");
                assert!(
                    result.issues().iter().any(|issue| issue.phase == "output"
                        && issue.condition == condition
                        && issue.requirement.as_deref() == Some("REQ-0240")),
                    "{condition}: {:?}",
                    result.issues()
                );
                // The retained log is the completed verification prefix, so a
                // diagnostic save publishes exactly that one artifact.
                for name in ["root.csv", "root-warnings.csv"] {
                    study.write(&format!("generated/{name}"), b"held sentinel");
                }
                assert!(result.save().is_err(), "{condition}");
                assert_eq!(
                    result.observations()["artifacts"].as_array().unwrap().len(),
                    1,
                    "{condition}: exactly the retained verification log is published"
                );
                assert_eq!(
                    fs::read(study.0.join("generated").join("root.csv")).unwrap(),
                    b"held sentinel"
                );
                assert_eq!(
                    fs::read(study.0.join("generated").join("root-warnings.csv")).unwrap(),
                    b"held sentinel"
                );
                let checks = String::from_utf8(
                    fs::read(study.0.join("generated").join("root-checks.csv")).unwrap(),
                )
                .unwrap();
                assert!(checks.contains("value-present"), "{condition}: {checks}");
                assert!(checks.contains("not_missing"), "{condition}: {checks}");
            }
        }
    }
}
