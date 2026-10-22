#![cfg(feature = "yaml")]
//! Native specification run assertions.
mod execution {
    mod specification_run {
        use crate::support::ports::NoFunctions;
        use crate::support::specification_run::*;
        use crate::support::Edit as _;
        use yamaa_core::application_observer::Scope;

        #[test]
        fn original_window_failures_match_independent_diagnostics_and_completed_observations() {
            use crate::support::ports::Sources;
            use yamaa_adapters::json;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            use yamaa_core::json::Value as Json;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-window-functions");
            let schema = schema(&root.join("yaml"));
            // The benchmark as 0.5 wrote it: in 1.0 the schema refuses an
            // ordered baseline window before WINDOW-5 compiles it. Its input
            // is the benchmark's own.
            let original = std::fs::read_to_string(
                crate::support::benchmarks::legacy("schema-window-functions").join("spec.yaml"),
            )
            .unwrap();
            let cases: Vec<Json> = yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/specs/window-failures.json"
            ))
            .unwrap();
            for variant in cases {
                let raw = original.replace(
                    variant["before"].as_str().unwrap(),
                    variant["after"].as_str().unwrap(),
                );
                let run = PreparedRun::prepare(prepare(&schema, raw.as_bytes())).unwrap();
                let mut content = std::fs::read_to_string(case.join("input/vs.csv")).unwrap();
                if let Some(input_before) = variant["input_before"].as_str() {
                    content =
                        content.replace(input_before, variant["input_after"].as_str().unwrap());
                }
                let mut host = Sources::by_path([("input/vs.csv", content.as_bytes())]);
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
                // A failed window has no file to save.
                assert!(result.files().is_empty());
                let actual = result.saved_observations().unwrap();
                let mut expected: Json = yamaa_adapters::json_wire::decode(include_str!(
                    "fixtures/specs/schema-window-functions.json"
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
                if let Some(cells) = variant["source_cells"].as_array() {
                    for cell in cells {
                        expected["tables"][0]["rows"][cell[0].as_u64().unwrap() as usize]
                            [cell[1].as_u64().unwrap() as usize] = cell[2].clone();
                    }
                }
                assert_eq!(
                    actual,
                    expected,
                    "{}",
                    yamaa_adapters::json_wire::encode(&variant["name"])
                );
                assert_eq!(host.captured(), ["VS"]);
            }
        }

        #[test]
        fn inherited_original_preparation_prunes_before_source_binding() {
            use yamaa_core::schema::SchemaSource;
            use yamaa_core::{inheritance as graph, inheritance_preparation as lifecycle};
            struct Host {
                root: std::path::PathBuf,
                reads: Vec<String>,
                resolutions: Vec<String>,
                paths: Vec<String>,
                fail_path: bool,
            }
            impl graph::InheritanceSourcePort for Host {
                type Error = &'static str;
                fn canonicalize(
                    &mut self,
                    _: &str,
                    written: &str,
                ) -> Result<graph::Source, graph::SourceError<&'static str>> {
                    self.resolutions.push(written.into());
                    Ok(graph::Source {
                        identity: written.into(),
                        display_path: written.into(),
                    })
                }
                fn read(
                    &mut self,
                    source: &graph::Source,
                ) -> Result<Document, graph::SourceError<&'static str>> {
                    self.reads.push(source.identity.clone());
                    Ok(
                        decode_yaml(&std::fs::read(self.root.join(&source.identity)).unwrap())
                            .unwrap()
                            .document,
                    )
                }
            }
            impl lifecycle::PathPort for Host {
                fn rebase(
                    &mut self,
                    _: &graph::Source,
                    _: &graph::Source,
                    written: &str,
                ) -> Result<String, &'static str> {
                    if self.fail_path {
                        return Err("original path failure");
                    }
                    self.paths.push(written.into());
                    Ok(written.into())
                }
            }
            impl yamaa_adapters::specification_source::InheritancePort for Host {
                type Error = &'static str;
                fn canonicalize(
                    &mut self,
                    declaring: &str,
                    written: &str,
                ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
                    graph::InheritanceSourcePort::canonicalize(self, declaring, written)
                }
                fn capture(
                    &mut self,
                    source: &graph::Source,
                ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
                    self.reads.push(source.identity.clone());
                    let bytes = std::fs::read(self.root.join(&source.identity)).unwrap();

                    Ok(bytes)
                }
                fn rebase(
                    &mut self,
                    layer: &graph::Source,
                    entry: &graph::Source,
                    written: &str,
                ) -> Result<String, Self::Error> {
                    lifecycle::PathPort::rebase(self, layer, entry, written)
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-inheritance");
            // The 0.5 inheritance lifecycle reads the layers as 0.5 wrote
            // them; the inputs and the expected output are the benchmark's.
            let layers = crate::support::benchmarks::legacy("schema-inheritance");
            let schema = schema(&root.join("yaml"));
            let raw = decode_yaml(&std::fs::read(layers.join("spec_study.yaml")).unwrap())
                .unwrap()
                .document;
            let mut host = Host {
                root: layers.clone(),
                reads: vec![],
                resolutions: vec![],
                paths: vec![],
                fail_path: false,
            };
            let prepared = lifecycle::prepare(
                schema.structure(),
                graph::Source {
                    identity: "spec_study.yaml".into(),
                    display_path: "spec_study.yaml".into(),
                },
                raw.clone(),
                &mut host,
            )
            .unwrap();
            assert_eq!(host.reads, ["spec_compound.yaml", "spec_organization.yaml"]);
            assert_eq!(
                host.resolutions,
                ["spec_compound.yaml", "spec_organization.yaml"]
            );
            assert_eq!(
                prepared
                    .layers()
                    .iter()
                    .map(|layer| layer.source.identity.as_str())
                    .collect::<Vec<_>>(),
                [
                    "spec_organization.yaml",
                    "spec_compound.yaml",
                    "spec_study.yaml"
                ]
            );
            let d = prepared.model().document();
            let columns = sequence(d, d.field(d.root(), "columns").unwrap());
            assert_eq!(
                columns
                    .iter()
                    .map(|&column| text(d, d.field(column, "name").unwrap()))
                    .collect::<Vec<_>>(),
                ["USUBJID", "PARAMCD", "AVAL", "AVALU"]
            );
            assert_eq!(
                text(d, d.field(columns[2], "label").unwrap()),
                "Analysis Value"
            );
            assert_eq!(
                text(d, d.field(columns[3], "label").unwrap()),
                "Standardized Analysis Unit"
            );
            let compiled =
                yamaa_core::specification::PreparedSpecification::prepare(prepared.model())
                    .unwrap();
            assert_eq!(compiled.sources().len(), 1);
            assert_eq!(
                (&*compiled.source().name, &*compiled.source().path),
                ("LB", "input/lb.csv")
            );
            assert_eq!(prepared.origins().len(), d.nodes().len());
            for origin in prepared.origins() {
                if origin.source == SchemaSource::Input {
                    assert!(origin.node < prepared.normalization_input().nodes().len());
                }
            }
            assert!(prepared
                .provenance()
                .iter()
                .any(|origin| origin.path == "columns.AVAL.label" && origin.layer == 2));

            let table = yamaa_adapters::typed_csv::parse(
                &std::fs::read(case.join("input/lb.csv")).unwrap(),
                &compiled.source().types,
            )
            .unwrap();
            let plan = compiled.bind(table.schema()).unwrap();
            let execution = plan
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
                execution.dataset.rows(),
                &[
                    vec![
                        Value::Str("01".into()),
                        Value::Str("ALT".into()),
                        Value::float(12.5),
                        Value::Str("U/L".into())
                    ],
                    vec![
                        Value::Str("02".into()),
                        Value::Str("ALT".into()),
                        Value::float(21.0),
                        Value::Str("U/L".into())
                    ],
                ]
            );
            // The organization level declares no verification: its `unique`
            // repeated the study's keys, which output-key validation asserts.
            assert!(execution.verifications.is_empty());
            assert_eq!(
                yamaa_adapters::csv_artifact::render(&execution.dataset, &[0, 1, 2, 3], None)
                    .unwrap(),
                std::fs::read(case.join("expected/adlb.csv")).unwrap()
            );
            for (original, reserved, path) in [
                ("scope", "label", "metadata.label"),
                ("analysis_role", "origin", "columns.AVAL.metadata.origin"),
            ] {
                let mut nodes = d.nodes().to_vec();
                let key = nodes
                    .iter()
                    .position(|n| matches!(n,N::Text(value) if value == original))
                    .unwrap();
                nodes[key] = N::Text(reserved.into());
                let changed = SpecificationDocument::admit(Document::new(nodes, d.root()).unwrap())
                    .unwrap()
                    .unwrap();
                let error = yamaa_core::specification::PreparedSpecification::prepare(&changed)
                    .unwrap_err();
                assert!(
                    matches!(error,yamaa_core::specification::PrepareError::Invalid(ref items) if items.iter().any(|item|matches!(item, yamaa_core::specification::PreflightFinding::Scalar(diagnostic) if diagnostic.code == yamaa_core::diagnostic::ConditionCode::SubmissionReservedMetadata && diagnostic.spec_paths == [path])))
                );
            }
            host.reads.clear();
            let captured = schema
                .prepare_inherited(
                    Source {
                        identity: "spec_study.yaml".into(),
                        bytes: std::fs::read(layers.join("spec_study.yaml")).unwrap(),
                    },
                    "spec_study.yaml".into(),
                    &mut host,
                )
                .unwrap();
            assert_eq!(host.reads, ["spec_compound.yaml", "spec_organization.yaml"]);
            assert_eq!(captured.parents().len(), 2);
            for parent in captured.parents() {
                assert_eq!(
                    parent.source().bytes,
                    std::fs::read(layers.join(&parent.source().identity)).unwrap()
                );
                assert!(!parent.raw().locations.is_empty());
            }
            assert!(captured.inheritance().is_some());
            let run = yamaa_adapters::specification_run::PreparedRun::prepare(captured).unwrap();
            struct Data {
                bytes: Arc<[u8]>,
                reads: usize,
            }
            impl yamaa_adapters::specification_run::SpecificationSourcePort for Data {
                fn inspect(
                    &mut self,
                    _source: &yamaa_core::specification::SourceDeclaration,
                ) -> Result<(), Self::Error> {
                    Ok(())
                }
                fn resource_failure(
                    &self,
                    _error: &Self::Error,
                ) -> Option<yamaa_core::resource::ResourceFailure> {
                    None
                }
                type Error = ();
                fn capture_reads(&self) -> usize {
                    self.reads
                }
                fn capture(
                    &mut self,
                    source: &yamaa_core::specification::SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, ()> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                    assert_eq!((&*source.name, &*source.path), ("LB", "input/lb.csv"));
                    self.reads = 1;
                    Ok(self.bytes.clone())
                }
            }
            let mut data = Data {
                bytes: Arc::from(std::fs::read(case.join("input/lb.csv")).unwrap()),
                reads: 0,
            };
            let artifact = std::fs::read(case.join("expected/adlb.csv")).unwrap();
            for created in [1, 0] {
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut data, Scope::default());
                let result = yamaa_adapters::specification_report::build_result(
                    &run,
                    &attempt,
                    yamaa_adapters::specification_report::Identity {
                        runtime: "python",
                        runtime_version: "fixture-runtime",
                        engine_version: "fixture-engine",
                        example: "schema-inheritance",
                        specification: "spec_study.yaml",
                        base_directory: ".",
                    },
                    &[],
                )
                .unwrap();
                assert_eq!(result.files(), [("adlb.csv", &artifact[..])]);
                let report = result.saved_observations().unwrap();
                let mut expected: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                    include_str!("fixtures/specs/schema-inheritance.json"),
                )
                .unwrap();
                expected["source_reads"][0]["snapshots_created"] = yamaa_adapters::json!(created);
                assert_eq!(report, expected);
            }
            host.fail_path = true;
            let error = lifecycle::prepare(
                schema.structure(),
                graph::Source {
                    identity: "spec_study.yaml".into(),
                    display_path: "spec_study.yaml".into(),
                },
                raw.clone(),
                &mut host,
            )
            .unwrap_err();
            assert!(matches!(
                error,
                lifecycle::Error::Path("original path failure")
            ));
        }

        #[test]
        fn classified_capture_failures_retain_complete_reports_without_publication() {
            use yamaa_adapters::json;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::{PortError, PreparedRun, SpecificationSourcePort},
            };
            use yamaa_core::resource::ResourceFailure;
            use yamaa_core::specification::SourceDeclaration;
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
            impl SpecificationSourcePort for Port {
                fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                    Ok(())
                }
                type Error = Payload;
                fn resource_failure(&self, error: &Payload) -> Option<ResourceFailure> {
                    error.cause
                }
                fn capture_reads(&self) -> usize {
                    self.reads
                }
                fn capture(
                    &mut self,
                    source: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, Payload> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
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
                    Ok(Arc::clone(&self.bytes))
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let case = root.join("benchmarks/schema-lookup");
            let schema = schema(&root.join("yaml"));
            let run = PreparedRun::prepare(prepare(
                &schema,
                &std::fs::read(case.join("spec.yaml")).unwrap(),
            ))
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

        #[test]
        fn resource_findings_use_written_inherited_paths_not_rebased_locations() {
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::{PreparedRun, SpecificationSourcePort},
                specification_source::{InheritancePort, Source},
            };
            use yamaa_core::resource::ResourceFailure;
            use yamaa_core::{
                inheritance::{Source as FileIdentity, SourceError},
                specification::SourceDeclaration,
            };
            struct Files {
                parent: Vec<u8>,
            }
            impl InheritancePort for Files {
                type Error = ();
                fn canonicalize(
                    &mut self,
                    _: &str,
                    written: &str,
                ) -> Result<FileIdentity, SourceError<()>> {
                    assert_eq!(written, "parent.yaml");
                    Ok(FileIdentity {
                        identity: written.into(),
                        display_path: written.into(),
                    })
                }
                fn capture(&mut self, _: &FileIdentity) -> Result<Vec<u8>, SourceError<()>> {
                    Ok(self.parent.clone())
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
            impl SpecificationSourcePort for Missing {
                fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                    Ok(())
                }
                type Error = ();
                fn resource_failure(&self, _: &()) -> Option<ResourceFailure> {
                    Some(ResourceFailure::Missing)
                }
                fn capture_reads(&self) -> usize {
                    0
                }
                fn capture(
                    &mut self,
                    _: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, ()> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                    Err(())
                }
            }
            for declaration in [
                "SRC: nested/data.csv",
                "SRC: {path: nested/data.csv, types: {ID: int}}",
            ] {
                for child_path in [None, Some("child.csv")] {
                    let mut entry = String::from("schema_version: '1.0'\nparent: parent.yaml\ndomain: TEST\nkeys: [ID]\noutput: {path: result.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n");
                    if let Some(path) = child_path {
                        entry.push_str(&format!("input: {{SRC: {path}}}\n"));
                    }
                    let parent = format!("schema_version: '1.0'\ninput: {{{declaration}}}\n");
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

        #[test]
        fn original_column_literals_match_complete_reference_reports_and_csv() {
            independent_scalar_reports(include_str!("fixtures/column_literals.json"), "literal", 4);
        }

        #[test]
        fn original_conversion_handlers_match_complete_reference_reports_and_csv() {
            independent_scalar_reports(
                include_str!("fixtures/original_conversion_handlers.json"),
                "handler",
                5,
            );
        }

        #[test]
        fn original_row_conversion_handlers_match_complete_reference_reports_and_csv() {
            independent_scalar_reports(
                include_str!("fixtures/original_row_conversion_handlers.json"),
                "row-handler",
                8,
            );
        }

        #[test]
        fn metadata_inspection_reports_all_known_failures_before_any_study_read() {
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::{PortError, PreparedRun, SpecificationSourcePort},
            };
            use yamaa_core::resource::ResourceFailure;
            use yamaa_core::specification::SourceDeclaration;
            struct Port {
                failures: yamaa_core::json::Value,
                inspected: Vec<String>,
            }
            impl SpecificationSourcePort for Port {
                type Error = ResourceFailure;
                fn resource_failure(&self, error: &ResourceFailure) -> Option<ResourceFailure> {
                    Some(*error)
                }
                fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), ResourceFailure> {
                    self.inspected.push(source.name.clone());
                    match self.failures[&source.name].as_str() {
                        Some("missing") => Err(ResourceFailure::Missing),
                        Some("not_regular_file") => Err(ResourceFailure::NotRegularFile),
                        None => Ok(()),
                        _ => panic!("unknown independent inspection cause"),
                    }
                }
                fn capture_reads(&self) -> usize {
                    0
                }
                fn capture(
                    &mut self,
                    _: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, ResourceFailure> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                    panic!("metadata failure reached study capture");
                }
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            let schema = schema(&root.join("yaml"));
            let run = PreparedRun::prepare(prepare(
                &schema,
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

        #[test]
        fn original_source_filters_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/source_filters.json"),
                "source-filter",
                13,
            );
        }

        #[test]
        fn original_first_available_matches_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/first_available.json"),
                "first-available",
                11,
            );
        }

        #[test]
        fn original_assertions_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_assertions.json"),
                "assert",
                17,
            );
        }

        #[test]
        fn original_all_or_none_matches_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_all_or_none.json"),
                "all-or-none",
                11,
            );
        }

        #[test]
        fn original_column_not_missing_matches_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_not_missing.json"),
                "column-check",
                9,
            );
        }

        #[test]
        fn row_filter_carrier_failures_match_complete_normative_reports() {
            replay_source_selection_truth(
                include_str!("fixtures/row_filter_admission.json"),
                "predicate-carrier",
                2,
            );
        }

        #[test]
        fn exact_wide_count_bounds_preserve_empty_counts_and_completed_check_prefix() {
            use crate::support::ports::Sources;
            use yamaa_adapters::json;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let prefix = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: source.csv}\noutput: {path: result.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            for input in [b"ID\n".as_slice(), b"ID\n1\n2\n".as_slice()] {
                for bound in [
                    "min: 999999999999999999999999999999",
                    "max: -999999999999999999999999999999",
                ] {
                    let source = format!("{prefix}verifications:\n  - row_count: {{id: first, min: -999999999999999999999999999999, max: 999999999999999999999999999999}}\n  - row_count: {{{bound}}}\n");
                    let run = PreparedRun::prepare(prepare(&schema, source.as_bytes())).unwrap();
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

        #[test]
        fn row_filters_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/row_filters.json"),
                "row-filter",
                27,
            );
        }

        #[test]
        fn row_reductions_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/row_reductions.json"),
                "row-reduction",
                25,
            );
        }

        #[test]
        fn aggregate_relations_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/correlated_aggregates.json"),
                "correlated-aggregate",
                16,
            );
        }

        #[test]
        fn odm_reads_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(include_str!("fixtures/odm_reads.json"), "odm", 14);
        }

        #[test]
        fn deferred_odm_defaults_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/odm_deferred.json"),
                "odm-deferred",
                7,
            );
        }

        #[test]
        fn odm_preflight_matches_complete_independent_findings_before_source_authority() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let mut count = 0;
            for row in fixture_rows(include_str!("fixtures/odm_preflight.json")) {
                let raw = fixture_bytes(&row["source"]);
                let error =
                    yamaa_adapters::specification_run::PreparedRun::prepare(prepare(&schema, &raw))
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

        #[test]
        fn row_column_checks_match_complete_independent_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/row_column_checks.json"),
                "row-column",
                47,
            );
        }

        #[test]
        fn original_column_values_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_column_values.json"),
                "column-value",
                20,
            );
        }

        #[test]
        fn original_column_matches_match_complete_reports_and_exact_csv() {
            replay_source_selection_truth(
                include_str!("fixtures/original_column_matches.json"),
                "column-matches",
                16,
            );
        }

        #[test]
        fn deferred_column_findings_keep_vocabulary_and_report_admission() {
            use yamaa_adapters::specification_run::{Error, PreparedRun};
            use yamaa_core::specification::PrepareError;
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let raw = |first: &str, severity: &str| {
                format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: unread.csv}}\noutput: {{path: result.csv, columns: [ID, V]}}\ncolumns:\n  - {{name: ID, type: str, label: ID, derivation: SRC.ID}}\n  - name: V\n    type: str\n    label: V\n    derivation: SRC.V\n    verifications:\n      - {first}\n      - matches: {{pattern: 'a{{1000001}}', severity: {severity}}}\n"
    )
            };
            for first in ["matches: {pattern: '('}", "allowed_values: {values: []}"] {
                let document = prepare(&schema, raw(first, "error").as_bytes());
                PreparedRun::prepare(document).expect("the earlier finding remains deferred");
                let document = prepare(&schema, raw(first, "warning").as_bytes());
                assert!(matches!(PreparedRun::prepare(document),
            Err(Error::Prepare(PrepareError::Invalid(items)))
                if items.iter().any(|item| item.diagnostic().definition().condition == "missing_report")));
                let with_report = raw(first, "warning")
                    .edit("columns: [ID, V]}", "columns: [ID, V], report: report.csv}");
                PreparedRun::prepare(prepare(&schema, with_report.as_bytes()))
                    .expect("the earlier finding remains deferred with a declared report file");
            }
        }

        #[test]
        fn static_verification_checks_match_complete_issues_without_study_authority() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let rows = fixture_rows(include_str!("fixtures/static_verification_checks.json"));
            assert_eq!(rows.len(), 15);
            for row in &rows {
                let document = prepare(&schema, &fixture_bytes(&row["source"]));
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

        #[test]
        fn original_source_bindings_match_complete_reports_and_exact_repeated_csv() {
            use crate::support::ports::Sources;
            use std::collections::BTreeMap;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let mut count = 0;
            for row in fixture_rows(include_str!("fixtures/original_source_bindings.json")) {
                let case = row["case"].as_str().unwrap();
                let run =
                    PreparedRun::prepare(prepare(&schema, &fixture_bytes(&row["source"]))).unwrap();
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
            assert_eq!(count, 16);
        }

        #[test]
        fn original_mapping_cut_match_complete_reports_and_exact_repeated_csv() {
            use crate::support::ports::Sources;
            use std::collections::BTreeMap;
            use yamaa_adapters::{
                specification_report::{self, Identity},
                specification_run::PreparedRun,
            };
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let mut count = 0;
            for row in fixture_rows(include_str!("fixtures/original_mapping_cut.json")) {
                let case = row["case"].as_str().unwrap();
                assert_eq!(
                    yamaa_adapters::json_wire::decode_slice(&fixture_bytes(&row["expected_wire"]))
                        .unwrap(),
                    row["expected"]
                );
                let run =
                    PreparedRun::prepare(prepare(&schema, &fixture_bytes(&row["source"]))).unwrap();
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

        #[test]
        fn source_condition_stays_unknown_under_the_normative_grammar() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let raw=b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: src.csv}\noutput: {path: result.csv, columns: [ID]}\ncolumns:\n  - name: ID\n    type: str\n    label: ID\n    derivation:\n      source: {variable: SRC.ID, condition: TRUE}\n";
            let error = schema
                .prepare_standalone(Source {
                    identity: "spec.yaml".into(),
                    bytes: raw.to_vec(),
                })
                .unwrap_err();
            let findings = crate::support::findings(
                yamaa_adapters::specification_diagnostics::capture_failure(&error),
            );
            assert!(findings
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["condition"] == "unknown_field"
                    && issue["spec_paths"]
                        == yamaa_adapters::json!(["columns.ID.derivation.source.condition"])));
        }

        #[test]
        fn original_mapping_cut_admission_has_complete_findings_before_study_ports() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            for row in fixture_rows(include_str!("fixtures/mapping_cut_admission.json")) {
                let bytes = fixture_bytes(&row["source"]);
                let error = yamaa_adapters::specification_run::PreparedRun::prepare(prepare(
                    &schema, &bytes,
                ))
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
