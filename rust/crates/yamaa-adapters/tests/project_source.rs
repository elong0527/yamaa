#![cfg(feature = "yaml")]
//! Project contracts: project source and project source decoder.
mod project {
    mod project_source {
        use crate::support::project_schemas::candidate_schemas;
        use std::sync::Arc;
        use yamaa_adapters::{
            project_source::{
                self, CaptureFailure, Failure, Kind, Origin, ProjectCapturePort, Reply, Request,
            },
            specification_source::{Error, Source},
        };
        use yamaa_core::{
            project_environment::{Finding, LockKind, Undeclared},
            project_function::Language,
            value::Value,
        };
        /// The caller-supplied project file; it declares the environment links and codelists.
        const PROJECT: &str = "study/project.yaml";
        /// The Python environment file the project links; it declares the lock and functions.
        const PYTHON: &str = "study/python/environment.yaml";
        fn source(identity: &str, bytes: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.as_bytes().to_vec(),
            }
        }
        /// One linked environment file whose lock and function paths resolve from it.
        fn environment_file(call: &str) -> String {
            format!("schema_version: '1.0'\nlock: project.lock\nfunctions:\n  first: functions/constant.yaml\n  inline: {{function: {call}, description: Constant, params: [], returns: int, tests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: 7}}]}}\n")
        }
        #[derive(Debug, PartialEq)]
        struct Payload(Box<str>);
        struct Port {
            trace: Vec<(Kind, String, String)>,
            /// Bytes of every linked environment file; its identity follows the link.
            environment: String,
            /// Callable spelling of the external function file.
            call: &'static str,
            /// Fail every dependency the environment file and project declare.
            fail: bool,
            /// Fail the linked environment file itself.
            fail_environment: bool,
            wide: bool,
            lock_kind: LockKind,
            interrupt_at: Option<String>,
            interrupt_address: usize,
        }
        impl Port {
            fn new() -> Self {
                Self {
                    trace: vec![],
                    environment: environment_file("project.constant"),
                    call: "project.constant",
                    fail: false,
                    fail_environment: false,
                    wide: false,
                    lock_kind: LockKind::Uv,
                    interrupt_at: None,
                    interrupt_address: 0,
                }
            }
        }
        impl ProjectCapturePort for Port {
            type Error = Payload;
            fn is_interrupt(&self, error: &Payload) -> bool {
                error.0.as_ref() == "host interrupt"
            }
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Payload> {
                self.trace.push((
                    request.kind,
                    request.declaring_source.into(),
                    request.written.into(),
                ));
                if self.interrupt_at.as_deref() == Some(request.written) {
                    let payload = Payload("host interrupt".into());
                    self.interrupt_address = payload.0.as_ptr() as usize;
                    return Err(payload);
                }
                let fail = if request.kind == Kind::Environment {
                    self.fail_environment
                } else {
                    self.fail
                };
                if fail {
                    return Err(Payload(request.written.into()));
                }
                match request.kind {
                    Kind::Environment => Ok(Reply::Document(source(
                        &format!("study/{}", request.written),
                        &self.environment,
                    ))),
                    Kind::Lock => Ok(Reply::Lock {
                        source: source("held/project.lock", "version = 1\n"),
                        kind: self.lock_kind,
                    }),
                    Kind::Function => Ok(Reply::Document(source(
                        "held/function.yaml",
                        &format!(
                            "function: {}\ndescription: Constant\nparams: []\nreturns: int\ntests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: {}}}]\n",
                            self.call,
                            if self.wide { "9223372036854775808" } else { "7" }
                        ),
                    ))),
                    Kind::Codelist => Ok(Reply::Document(source(
                        "held/terminology.yaml",
                        "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n",
                    ))),
                }
            }
        }
        fn root() -> Source {
            source(PROJECT, "schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\ncodelists: [ct/sex.yaml]\nstudy: {id: Study, name: Study, description: Study, protocol_name: Protocol}\nadam: {standard: {name: ADaMIG, version: '1.3', status: Final}, specs: [specs/adsl.yaml], define: data/define.xml, documents: [{id: guide, kind: supplemental, link: docs/adrg.pdf, title: Guide}]}\n")
        }
        #[test]
        fn owned_environment_parts_preserve_original_allocations_and_capture_evidence() {
            let mut port = Port::new();
            let prepared =
                project_source::prepare(candidate_schemas(), root(), Language::Python, &mut port)
                    .unwrap();
            let root_bytes = prepared.root.source().bytes.as_ptr();
            let environment_bytes = prepared
                .environment_file
                .as_ref()
                .unwrap()
                .source()
                .bytes
                .as_ptr();
            let lock_bytes = prepared.lock.as_ref().unwrap().source.bytes.as_ptr();
            let cases = prepared.environment.draft().functions.as_ref().unwrap()[0]
                .tests
                .as_ptr();
            let captures = prepared.captures.as_ptr();
            let count = port.trace.len();
            let (environment, provenance) = prepared.into_owned().into_parts();
            assert_eq!(provenance.root().source().bytes.as_ptr(), root_bytes);
            assert_eq!(
                provenance
                    .environment_file()
                    .unwrap()
                    .source()
                    .bytes
                    .as_ptr(),
                environment_bytes
            );
            assert_eq!(provenance.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
            assert_eq!(
                environment.functions()[0].definition().tests.as_ptr(),
                cases
            );
            assert_eq!(provenance.captures().as_ptr(), captures);
            assert_eq!(provenance.origins().functions, ["first", "inline"]);
            assert_eq!(environment.lock().unwrap().written, "project.lock");
            assert_eq!(
                environment
                    .catalogue()
                    .get("SEX")
                    .unwrap()
                    .items
                    .as_ref()
                    .unwrap()[0]
                    .value,
                Value::Str("F".into())
            );
            assert_eq!(port.trace.len(), count);
        }
        #[test]
        fn held_dependency_capture_is_ordered_typed_and_repeated_without_study_or_code_ports() {
            let schemas = candidate_schemas();
            let mut port = Port::new();
            for _ in 0..2 {
                let prepared =
                    project_source::prepare(schemas.clone(), root(), Language::Python, &mut port)
                        .unwrap();
                assert!(Arc::ptr_eq(prepared.root.schema(), &schemas.project));
                assert_eq!(prepared.root.source().bytes, root().bytes);
                let environment = prepared.environment_file.as_ref().unwrap();
                assert!(Arc::ptr_eq(environment.schema(), &schemas.environment));
                assert_eq!(environment.source().identity, PYTHON);
                assert_eq!(
                    prepared
                        .environment
                        .draft()
                        .functions
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|f| f.name.name.as_str())
                        .collect::<Vec<_>>(),
                    ["first", "inline"]
                );
                assert_eq!(prepared.captures.len(), 2);
                assert_eq!(
                    prepared.captures[0].origin,
                    Origin::Function("first".into())
                );
                assert_eq!(
                    prepared.captures[0].document.source().identity,
                    "held/function.yaml"
                );
                assert_eq!(
                    prepared.captures[0].document.class_name(),
                    "function_definition_class"
                );
                assert!(Arc::ptr_eq(
                    prepared.captures[0].document.schema(),
                    &schemas.environment
                ));
                assert_eq!(prepared.captures[1].origin, Origin::Codelist(0));
                assert_eq!(
                    prepared.captures[1].document.class_name(),
                    "codelist_source_class"
                );
                assert!(Arc::ptr_eq(
                    prepared.captures[1].document.schema(),
                    &schemas.project
                ));
                assert_eq!(
                    prepared.lock.as_ref().unwrap().source.bytes,
                    b"version = 1\n"
                );
                // The study and sections lower beside, in the project root.
                let declarations = prepared.project.submission.as_ref().unwrap();
                assert!(declarations.study.is_some());
                assert!(!declarations.sections.is_empty());
                let root_bytes = prepared.root.source().bytes.as_ptr();
                let environment_bytes = environment.source().bytes.as_ptr();
                let lock_bytes = prepared.lock.as_ref().unwrap().source.bytes.as_ptr();
                let cases = prepared.environment.draft().functions.as_ref().unwrap()[0]
                    .tests
                    .as_ptr();
                let items = prepared.environment.draft().codelists[0].codelists[0]
                    .items
                    .as_ref()
                    .unwrap()
                    .as_ptr();
                let captured = port.trace.len();
                let owned = prepared.into_owned();
                assert_eq!(owned.root().source().bytes.as_ptr(), root_bytes);
                assert_eq!(
                    owned.environment_file().unwrap().source().bytes.as_ptr(),
                    environment_bytes
                );
                assert_eq!(owned.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
                assert_eq!(
                    owned.environment().functions()[0]
                        .definition()
                        .tests
                        .as_ptr(),
                    cases
                );
                assert_eq!(
                    owned.environment().catalogue().sources()[0].codelists[0]
                        .items
                        .as_ref()
                        .unwrap()
                        .as_ptr(),
                    items
                );
                assert_eq!(owned.origins().functions, ["first", "inline"]);
                assert_eq!(owned.origins().codelists, [0]);
                assert_eq!(owned.captures().len(), 2);
                assert_eq!(owned.environment().host(), Some(Language::Python));
                assert_eq!(owned.environment().language(), Some(Language::Python));
                assert_eq!(owned.environment().undeclared(), None);
                assert_eq!(port.trace.len(), captured);
            }
            // The environment file declares the lock and functions; the project
            // declares its environment link and codelists. The R link stays unread.
            let one = [
                (Kind::Environment, PROJECT, "python/environment.yaml"),
                (Kind::Lock, PYTHON, "project.lock"),
                (Kind::Function, PYTHON, "functions/constant.yaml"),
                (Kind::Codelist, PROJECT, "ct/sex.yaml"),
            ];
            assert_eq!(
                port.trace
                    .iter()
                    .map(|(kind, declaring, written)| (*kind, declaring.as_str(), written.as_str()))
                    .collect::<Vec<_>>(),
                one.into_iter().chain(one).collect::<Vec<_>>()
            );
        }
        #[test]
        fn each_host_reads_only_the_environment_file_its_own_key_names() {
            for (host, written, call, lock_kind) in [
                (
                    Language::Python,
                    "python/environment.yaml",
                    "project.constant",
                    LockKind::Uv,
                ),
                (
                    Language::R,
                    "r/environment.yaml",
                    "project::constant",
                    LockKind::Renv,
                ),
            ] {
                let mut port = Port::new();
                port.environment = environment_file(call);
                port.call = call;
                port.lock_kind = lock_kind;
                let prepared =
                    project_source::prepare(candidate_schemas(), root(), host, &mut port)
                        .unwrap_or_else(|_| panic!("{host:?} environment admits"));
                let declaring = format!("study/{written}");
                assert_eq!(
                    prepared
                        .environment_file
                        .as_ref()
                        .unwrap()
                        .source()
                        .identity,
                    declaring
                );
                assert_eq!(
                    port.trace,
                    [
                        (Kind::Environment, PROJECT.into(), written.into()),
                        (Kind::Lock, declaring.clone(), "project.lock".into()),
                        (
                            Kind::Function,
                            declaring.clone(),
                            "functions/constant.yaml".into()
                        ),
                        (Kind::Codelist, PROJECT.into(), "ct/sex.yaml".into()),
                    ]
                );
                assert_eq!(prepared.environment.draft().language, Some(host));
                assert_eq!(
                    prepared.environment.draft().lock.as_ref().unwrap().kind,
                    lock_kind
                );
                let owned = prepared.into_owned();
                assert_eq!(owned.environment().host(), Some(host));
                for function in owned.environment().functions() {
                    assert_eq!(function.call(), call);
                }
            }
        }
        #[test]
        fn project_without_the_host_environment_captures_no_code_and_admits_without_language() {
            for project in [
                "schema_version: '1.0'\nenvironments: {r: r/environment.yaml}\ncodelists: [ct/sex.yaml]\n",
                "schema_version: '1.0'\ncodelists: [ct/sex.yaml]\n",
            ] {
                let mut port = Port::new();
                let prepared = project_source::prepare(
                    candidate_schemas(),
                    source(PROJECT, project),
                    Language::Python,
                    &mut port,
                )
                .unwrap_or_else(|_| panic!("a project without code admits"));
                assert_eq!(
                    port.trace,
                    [(Kind::Codelist, PROJECT.into(), "ct/sex.yaml".into())]
                );
                assert!(prepared.environment_file.is_none());
                assert!(prepared.lock.is_none());
                assert_eq!(prepared.captures.len(), 1);
                let draft = prepared.environment.draft();
                assert_eq!(draft.language, None);
                assert!(draft.lock.is_none());
                assert!(draft.functions.is_none());
                assert_eq!(draft.codelists[0].codelists[0].id.name, "SEX");
                let owned = prepared.into_owned();
                assert!(owned.environment_file().is_none());
                assert_eq!(owned.environment().language(), None);
                // A call to a project function now names the absent environment.
                assert_eq!(
                    owned.environment().undeclared(),
                    Some(Undeclared::Language(Language::Python))
                );
            }
        }
        #[test]
        fn sections_that_list_only_specifications_admit_without_study() {
            let mut port = Port::new();
            let text = "schema_version: '1.0'\nsdtm: {specs: [sdtm/dm.yaml]}\nadam: {specs: [adam/adsl.yaml], datasets: json}\n";
            let prepared = project_source::prepare(
                candidate_schemas(),
                source(PROJECT, text),
                Language::Python,
                &mut port,
            )
            .unwrap_or_else(|_| panic!("submission metadata is not an admission obligation"));
            assert!(port.trace.is_empty());
            let document = &prepared.root.normalized().document;
            assert!(document.field(document.root(), "study").is_none());
            let adam = document.field(document.root(), "adam").unwrap();
            assert!(document.field(adam, "standard").is_none());
            assert!(document.field(adam, "define").is_none());
            assert_eq!(prepared.root.source().bytes, text.as_bytes());
        }
        #[test]
        fn invalid_root_and_inline_scalars_fail_before_every_dependency_capture() {
            let mut port = Port::new();
            // Installed code belongs to the linked environment file, not the project.
            let invalid = source(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\nruntime: {language: python}\nlock: project.lock\n",
            );
            let Err(Failure::Root(Error::Findings(captured))) =
                project_source::prepare(candidate_schemas(), invalid, Language::Python, &mut port)
            else {
                panic!("project structural failure")
            };
            assert_eq!(
                captured
                    .findings()
                    .iter()
                    .map(|finding| (finding.path.as_str(), finding.definition().condition))
                    .collect::<Vec<_>>(),
                [("runtime", "unknown_field"), ("lock", "unknown_field")]
            );
            let invalid=source("project.yaml","schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [{codelists: [{id: WIDE, name: Wide, data_type: integer, items: [{value: 9223372036854775808}]}]}]\n");
            let Err(Failure::RootScalar { root, findings }) =
                project_source::prepare(candidate_schemas(), invalid, Language::Python, &mut port)
            else {
                panic!("owned scalar failure")
            };
            assert_eq!(root.source().identity, "project.yaml");
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "codelists[0].codelists[0].items[0].value");
            assert!(port.trace.is_empty());
        }
        #[test]
        fn every_capture_failure_retains_original_opaque_payload_and_declaration_context() {
            let mut port = Port::new();
            port.fail = true;
            port.environment =
                "schema_version: '1.0'\nlock: project.lock\nfunctions: {a: a.yaml, b: b.yaml}\n"
                    .into();
            let root = source(PROJECT, "schema_version: '1.0'\nenvironments: {python: python/environment.yaml}\ncodelists: [one.yaml, two.yaml]\n");
            let Err(Failure::Rejected(rejected)) =
                project_source::prepare(candidate_schemas(), root, Language::Python, &mut port)
            else {
                panic!("all source failures")
            };
            assert_eq!(port.trace.len(), 6);
            assert_eq!(rejected.sources.len(), 5);
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|f| (f.origin.clone(), f.written.as_str()))
                    .collect::<Vec<_>>(),
                [
                    (Origin::Lock, "project.lock"),
                    (Origin::Function("a".into()), "a.yaml"),
                    (Origin::Function("b".into()), "b.yaml"),
                    (Origin::Codelist(0), "one.yaml"),
                    (Origin::Codelist(1), "two.yaml")
                ]
            );
            for failure in &rejected.sources {
                let CaptureFailure::Port(Payload(original)) = &failure.error else {
                    panic!("opaque error preserved")
                };
                assert_eq!(&**original, failure.written);
            }
            assert_eq!(rejected.root.source().identity, PROJECT);
            assert_eq!(
                rejected
                    .environment_file
                    .as_ref()
                    .unwrap()
                    .source()
                    .identity,
                PYTHON
            );
            assert!(rejected.captures.is_empty());
            assert_eq!(rejected.host, Some(Language::Python));
            assert_eq!(rejected.draft.language, Some(Language::Python));
            assert!(rejected.draft.lock.is_none());
            assert_eq!(rejected.draft.functions.as_deref(), Some([].as_slice()));
            assert!(rejected.draft.codelists.is_empty());
        }
        #[test]
        fn environment_file_capture_failure_keeps_its_link_and_requests_no_lock_or_functions() {
            let mut port = Port::new();
            port.fail_environment = true;
            let Err(Failure::Rejected(rejected)) =
                project_source::prepare(candidate_schemas(), root(), Language::Python, &mut port)
            else {
                panic!("environment file failure")
            };
            assert_eq!(
                port.trace,
                [
                    (
                        Kind::Environment,
                        PROJECT.into(),
                        "python/environment.yaml".into()
                    ),
                    (Kind::Codelist, PROJECT.into(), "ct/sex.yaml".into()),
                ]
            );
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(
                rejected.sources[0].origin,
                Origin::Environment(Language::Python)
            );
            assert_eq!(rejected.sources[0].written, "python/environment.yaml");
            let CaptureFailure::Port(Payload(original)) = &rejected.sources[0].error else {
                panic!("opaque environment error preserved")
            };
            assert_eq!(&**original, "python/environment.yaml");
            assert!(rejected.environment_file.is_none());
            assert!(rejected.lock.is_none());
            assert_eq!(rejected.captures.len(), 1);
            assert_eq!(rejected.captures[0].origin, Origin::Codelist(0));
            // The key still states the language; no environment supplied code.
            assert_eq!(rejected.draft.language, Some(Language::Python));
            assert!(rejected.draft.lock.is_none());
            assert!(rejected.draft.functions.is_none());
            assert_eq!(rejected.draft.codelists[0].codelists[0].id.name, "SEX");
            assert!(rejected.admission.is_none());
        }
        #[test]
        fn environment_file_inline_scalars_are_a_retained_failure_of_its_link() {
            let schemas = candidate_schemas();
            let mut port = Port::new();
            port.environment = "schema_version: '1.0'\nlock: project.lock\nfunctions: {wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: wide, covers: [normal], args: {}, result: 9223372036854775808}]}}\n".into();
            let Err(Failure::Rejected(rejected)) =
                project_source::prepare(schemas.clone(), root(), Language::Python, &mut port)
            else {
                panic!("environment file scalar failure")
            };
            assert_eq!(
                port.trace
                    .iter()
                    .map(|(kind, _, written)| (*kind, written.as_str()))
                    .collect::<Vec<_>>(),
                [
                    (Kind::Environment, "python/environment.yaml"),
                    (Kind::Codelist, "ct/sex.yaml")
                ]
            );
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(
                rejected.sources[0].origin,
                Origin::Environment(Language::Python)
            );
            assert_eq!(rejected.sources[0].written, "python/environment.yaml");
            let CaptureFailure::Scalar { document, findings } = &rejected.sources[0].error else {
                panic!("owned environment context")
            };
            assert_eq!(document.source().identity, PYTHON);
            assert_eq!(document.class_name(), "environment_class");
            assert!(Arc::ptr_eq(document.schema(), &schemas.environment));
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "functions.wide.tests[0].result");
            assert!(findings[0].node < document.normalized().document.nodes().len());
            assert!(rejected.environment_file.is_none());
            assert!(rejected.lock.is_none());
            assert!(rejected.draft.functions.is_none());
        }

        #[test]
        fn dependency_scalar_failure_keeps_the_original_owned_document_and_other_sources() {
            let mut port = Port::new();
            port.wide = true;
            let Err(Failure::Rejected(rejected)) =
                project_source::prepare(candidate_schemas(), root(), Language::Python, &mut port)
            else {
                panic!("scalar failure")
            };
            assert_eq!(port.trace.len(), 4);
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.captures.len(), 1);
            assert_eq!(rejected.sources[0].origin, Origin::Function("first".into()));
            let CaptureFailure::Scalar { document, findings } = &rejected.sources[0].error else {
                panic!("owned context")
            };
            assert_eq!(document.source().identity, "held/function.yaml");
            assert!(
                String::from_utf8_lossy(&document.source().bytes).contains("9223372036854775808")
            );
            assert_eq!(findings[0].path, "tests[0].result");
            assert!(findings[0].node < document.normalized().document.nodes().len());
        }
        #[test]
        fn lock_kind_is_captured_syntax_metadata_and_mismatch_does_not_import_project_code() {
            let mut port = Port::new();
            port.lock_kind = LockKind::Renv;
            let Err(Failure::Rejected(rejected)) =
                project_source::prepare(candidate_schemas(), root(), Language::Python, &mut port)
            else {
                panic!("lock kind mismatch")
            };
            assert_eq!(port.trace.len(), 4);
            assert_eq!(
                rejected.admission,
                Some(vec![Finding::LockKindMismatch {
                    language: Language::Python,
                    actual: LockKind::Renv
                }])
            );
            assert!(rejected.sources.is_empty());
            assert_eq!(rejected.host, Some(Language::Python));
            assert_eq!(rejected.draft.language, Some(Language::Python));
            assert_eq!(
                rejected.draft.lock.as_ref().unwrap().written,
                "project.lock"
            );
            assert_eq!(rejected.draft.lock.as_ref().unwrap().kind, LockKind::Renv);
            let functions = rejected.draft.functions.as_ref().unwrap();
            assert_eq!(
                functions
                    .iter()
                    .map(|f| f.name.name.as_str())
                    .collect::<Vec<_>>(),
                ["first", "inline"]
            );
            assert_eq!(functions[0].tests[0].result, Value::Int(7));
            assert_eq!(functions[1].tests[0].result, Value::Int(7));
            assert_eq!(rejected.draft.codelists[0].codelists[0].id.name, "SEX");
            assert_eq!(
                rejected
                    .environment_file
                    .as_ref()
                    .unwrap()
                    .source()
                    .identity,
                PYTHON
            );
            // The study and sections lower beside, in the project root.
            let declarations = rejected.project.submission.as_ref().unwrap();
            assert!(declarations.study.is_some());
            assert!(!declarations.sections.is_empty());
        }

        #[test]
        fn interrupts_at_every_dependency_boundary_preserve_original_payload_and_abort_later_reads()
        {
            for (written, origin, observed) in [
                (
                    "python/environment.yaml",
                    Origin::Environment(Language::Python),
                    1,
                ),
                ("project.lock", Origin::Lock, 2),
                (
                    "functions/constant.yaml",
                    Origin::Function("first".into()),
                    3,
                ),
                ("ct/sex.yaml", Origin::Codelist(0), 4),
            ] {
                let mut port = Port::new();
                port.interrupt_at = Some(written.into());
                let Err(Failure::Interrupted {
                    origin: actual_origin,
                    written: actual_written,
                    error,
                }) = project_source::prepare(
                    candidate_schemas(),
                    root(),
                    Language::Python,
                    &mut port,
                )
                else {
                    panic!("interrupt must bypass collected static findings");
                };
                assert_eq!(actual_origin, origin);
                assert_eq!(actual_written, written);
                assert_eq!(error.0.as_ptr() as usize, port.interrupt_address);
                assert_eq!(error.0.as_ref(), "host interrupt");
                assert_eq!(port.trace.len(), observed);
            }
        }
    }
    mod project_source_decoder {
        use crate::support::project_call_compiler as support;
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_adapters::project_source_decoder::Decoder;
        use yamaa_core::{
            dataset::ExecutionError,
            function_invocation::{Argument, FailureKind, HostError},
            host_functions::{HostFunctions, Lock},
            project_activation::Failure,
            project_domain,
            specification_run::{
                CapturedAttempt, PortError, SourceDecoder, SpecificationSourcePort,
            },
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{Draft, Environment, LockKind, LockReference},
            project_function::Language,
            specification::{SourceDeclaration, SourceInterpretation, SourceProfile},
            table::{CellError, TableAccess, ValueRef},
            value::{ColumnType, Value},
        };

        #[derive(Debug, PartialEq)]
        struct Payload(Rc<()>);

        #[test]
        fn wide_producer_contract_decodes_1024_and_refuses_1025_actual_fields() {
            let names = (0..1_024)
                .map(|column| format!("F{column:04}"))
                .collect::<Vec<_>>();
            let columns = names
        .iter()
        .map(|name| {
            format!("  - {{name: {name}, type: str, label: Field, derivation: {{literal: x}}}}\n")
        })
        .collect::<String>();
            let document = yamaa_adapters::shipped_schema::capture().unwrap().prepare_standalone(
        yamaa_adapters::specification_source::Source {
            identity: "wide-producer.yaml".into(),
            bytes: format!("schema_version: '1.0'\ndomain: TEST\nkeys: [F0000]\ninput: {{RAW: raw.csv}}\ncolumns:\n{columns}output: {{path: produced.csv, columns: [{}]}}\n", names.join(", ")).into_bytes(),
        }
    ).unwrap();
            let contract = yamaa_core::producer_contract::prepare(document.model()).unwrap();
            let source = SourceDeclaration {
                name: "P".into(),
                path: "produced.csv".into(),
                types: names
                    .iter()
                    .map(|name| (name.clone(), ColumnType::Str))
                    .collect(),
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            let bytes = format!("{}\n{}\n", names.join(","), vec!["x"; 1_024].join(","));
            let mut decoder = Decoder::<Payload>::default();
            let table = decoder
                .decode(
                    &source,
                    bytes.as_bytes(),
                    SourceInterpretation::Ordinary,
                    Some(&contract),
                )
                .unwrap();
            assert_eq!(table.schema().columns().len(), 1_024);
            assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            let excess = format!("{},EXCESS\n", names.join(","));
            assert!(matches!(
                decoder.decode(
                    &source,
                    excess.as_bytes(),
                    SourceInterpretation::Ordinary,
                    Some(&contract)
                ),
                Err(yamaa_adapters::specification_run::Error::Source(
                    yamaa_adapters::csv_source::TextTableError::Csv(
                        yamaa_core::csv_source::Error::Limit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        }
                    )
                ))
            ));
            assert_eq!(
                decoder
                    .decode(
                        &source,
                        bytes.as_bytes(),
                        SourceInterpretation::Ordinary,
                        Some(&contract)
                    )
                    .unwrap()
                    .cell(0, 0)
                    .unwrap(),
                ValueRef::Str("x")
            );
        }

        #[test]
        fn producer_csv_admits_original_header_before_typing_and_owns_portable_mismatch() {
            let document = yamaa_adapters::shipped_schema::capture().unwrap().prepare_standalone(
        yamaa_adapters::specification_source::Source {
            identity: "held-producer.yaml".into(),
            bytes: b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n".to_vec(),
        }
    ).unwrap();
            let contract = yamaa_core::producer_contract::prepare(document.model()).unwrap();
            let source = SourceDeclaration {
                name: "P".into(),
                path: "produced.csv".into(),
                types: vec![
                    ("ID".into(), ColumnType::Int),
                    ("VALUE".into(), ColumnType::Int),
                ],
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            let mut decoder = Decoder::<Payload>::default();
            for bytes in [
                b"VALUE,ID\nnot-an-int,also-bad\n".as_slice(),
                b"ID\nnot-an-int\n",
                b"ID,VALUE,EXTRA\nnot-an-int,also-bad,x\n",
            ] {
                let yamaa_adapters::specification_run::Error::ProducerMetadata(diagnostic) =
                    decoder
                        .decode(
                            &source,
                            bytes,
                            SourceInterpretation::Ordinary,
                            Some(&contract),
                        )
                        .err()
                        .unwrap()
                else {
                    panic!("header mismatch must precede declared typing")
                };
                assert_eq!(
                    diagnostic.code,
                    yamaa_core::diagnostic::ConditionCode::ProducerContractMismatch
                );
                assert_eq!(diagnostic.spec_paths, ["input.P.schema", "input.P.path"]);
            }
            let table = decoder
                .decode(
                    &source,
                    b"ID,VALUE\n1,13\n",
                    SourceInterpretation::Ordinary,
                    Some(&contract),
                )
                .unwrap();
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(13));
            let mut ordinal_source = source.clone();
            ordinal_source.ordinal = Some("REC".into());
            let table = decoder
                .decode(
                    &ordinal_source,
                    b"ID,VALUE\n1,13\n2,17\n",
                    SourceInterpretation::Ordinary,
                    Some(&contract),
                )
                .unwrap();
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                ["ID", "VALUE", "REC"]
            );
            assert_eq!(
                (table.cell(0, 2).unwrap(), table.cell(1, 2).unwrap()),
                (ValueRef::Int(1), ValueRef::Int(2))
            );
            ordinal_source.ordinal = Some("ID".into());
            for bytes in [b"ID,VALUE\n1,13\n".as_slice(), b"ID,VALUE\n"] {
                assert!(matches!(
                    decoder.decode(
                        &ordinal_source,
                        bytes,
                        SourceInterpretation::Ordinary,
                        Some(&contract)
                    ),
                    Err(yamaa_adapters::specification_run::Error::Ordinal(
                        yamaa_core::source_ordinal::Error::Collision
                    ))
                ));
            }

            let header = (0..1_025)
                .map(|i| format!("F{i}"))
                .collect::<Vec<_>>()
                .join(",")
                + "\n";
            assert!(matches!(
                decoder.decode(
                    &source,
                    header.as_bytes(),
                    SourceInterpretation::Ordinary,
                    Some(&contract)
                ),
                Err(yamaa_adapters::specification_run::Error::Source(
                    yamaa_adapters::csv_source::TextTableError::Csv(
                        yamaa_core::csv_source::Error::Limit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        }
                    )
                ))
            ));
        }

        #[test]
        fn decoded_profiles_keep_exact_cells_bounds_and_ingestion_failure_categories() {
            let mut decoder = Decoder::<Payload>::default();
            let source = SourceDeclaration {
                name: "SRC".into(),
                path: "input.csv".into(),
                types: vec![
                    ("I".into(), ColumnType::Int),
                    ("S".into(), ColumnType::Str),
                    ("F".into(), ColumnType::Float),
                    ("D".into(), ColumnType::Date),
                    ("T".into(), ColumnType::DateTime),
                ],
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: true,
                ordinal: None,
            };
            let table = decoder
        .decode(
            &source,
            b"I,S,F,D,T\n9223372036854775807,\"x\0y\",-0.0,2024-02-29,2024-02-29T12:34:56\n",
            SourceInterpretation::Ordinary,
            None,
        )
        .unwrap();
            assert_eq!(table.row_count(), 1);
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(i64::MAX));
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str("x\0y"));
            let ValueRef::Float(value) = table.cell(0, 2).unwrap() else {
                panic!("exact float")
            };
            assert_eq!(value.get().to_bits(), (-0.0_f64).to_bits());
            let ValueRef::Date(value) = table.cell(0, 3).unwrap() else {
                panic!("exact date")
            };
            assert_eq!(value.to_string(), "2024-02-29");
            let ValueRef::DateTime(value) = table.cell(0, 4).unwrap() else {
                panic!("exact datetime")
            };
            assert_eq!(value.to_string(), "2024-02-29T12:34:56");
            assert_eq!(
                table.cell(1, 0),
                Err(CellError::OutOfBounds { row: 1, column: 0 })
            );
            assert_eq!(
                table.cell(0, 5),
                Err(CellError::OutOfBounds { row: 0, column: 5 })
            );
            let ValueRef::Str(text) = table.cell(0, 1).unwrap() else {
                unreachable!()
            };
            let retained = text.as_ptr();
            let owned = table.into_table();
            let ValueRef::Str(text) = owned.cell(0, 1).unwrap() else {
                unreachable!()
            };
            assert_eq!(text.as_ptr(), retained);
            let error = decoder
                .decode(
                    &source,
                    b"I,S,F,D,T\nbad,x,1,2024-02-29,2024-02-29T12:34:56\n",
                    SourceInterpretation::Ordinary,
                    None,
                )
                .err()
                .unwrap();
            assert!(decoder.continue_after(&error));
            let wide_header = (0..1025)
                .map(|index| format!("C{index}"))
                .collect::<Vec<_>>()
                .join(",")
                + "\n";
            let error = decoder
                .decode(
                    &source,
                    wide_header.as_bytes(),
                    SourceInterpretation::Ordinary,
                    None,
                )
                .err()
                .unwrap();
            assert!(matches!(
                &error,
                yamaa_adapters::specification_run::Error::Source(
                    yamaa_adapters::csv_source::TextTableError::Csv(
                        yamaa_core::csv_source::Error::Limit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        }
                    )
                )
            ));
            assert!(!decoder.continue_after(&error));

            #[cfg(feature = "parquet")]
            {
                let bytes = &crate::support::parquet_files::file("text")[..];
                for present in [false, true] {
                    let source = SourceDeclaration {
                        name: "SRC".into(),
                        path: "text.parquet".into(),
                        types: vec![("I".into(), ColumnType::Int), ("S".into(), ColumnType::Str)],
                        profile: SourceProfile::Parquet,
                        encoding: None,
                        byte_order_mark: false,
                        empty_string_present: present,
                        ordinal: None,
                    };
                    let table = decoder
                        .decode(&source, bytes, SourceInterpretation::Ordinary, None)
                        .unwrap();
                    assert_eq!(
                        table.cell(0, 1).unwrap(),
                        if present {
                            ValueRef::Str("")
                        } else {
                            ValueRef::Missing
                        }
                    );
                    assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("X"));
                }
            }
        }

        type Trace = Rc<RefCell<Vec<&'static str>>>;
        struct Activation {
            trace: Trace,
            payload: Rc<()>,
            live_failed: bool,
            lock_failed: bool,
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = Payload;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: Lock<'_>,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.reference.kind, LockKind::Uv);
                assert_eq!(
                    functions
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>(),
                    ["id"]
                );
                self.trace.borrow_mut().push("lock");
                if self.lock_failed {
                    Err(Payload(Rc::clone(&self.payload)))
                } else {
                    Ok(())
                }
            }
            fn bind(
                &mut self,
                identity: &ProjectFunctionIdentity,
                signature: &LogicalSignature,
            ) -> Result<(), Payload> {
                assert_eq!(identity.name, "id");
                assert_eq!(signature.parameters()[0].host_name, "x");
                self.trace.borrow_mut().push("bind");
                Ok(())
            }
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
                let [Argument {
                    name: "x",
                    value: ValueRef::Int(value),
                }] = args
                else {
                    panic!("exact argument")
                };
                self.trace
                    .borrow_mut()
                    .push(if *value == 1 { "live" } else { "case" });
                if self.live_failed && *value == 1 {
                    return Err(HostError::Raised(Payload(Rc::clone(&self.payload))));
                }
                Ok(Value::Int(*value))
            }
        }
        struct Source {
            trace: Trace,
            bytes: Arc<[u8]>,
            reads: usize,
        }
        impl SpecificationSourcePort for Source {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = Payload;
            fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Payload> {
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("inspect");
                Ok(())
            }
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                _: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Payload> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                self.trace.borrow_mut().push("capture");
                self.reads = 1;
                Ok(Arc::clone(&self.bytes))
            }
        }
        #[test]
        fn real_decoder_builds_reactivate_cached_sources_and_retain_original_non_send_errors() {
            let environment = Environment::admit(
                Some(Language::Python),
                Draft {
                    surface: yamaa_core::ast::Surface::V0_5,
                    language: Some(Language::Python),
                    lock: Some(LockReference {
                        written: "uv.lock".into(),
                        kind: LockKind::Uv,
                    }),
                    functions: Some(vec![
                        support::function("unused").definition().clone(),
                        support::function("id").definition().clone(),
                    ]),
                    codelists: vec![],
                },
            )
            .unwrap()
            .into_execution();
            let raw = b"schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: {value: {source: {variable: SRC.ID}}}}\n  - {name: VALUE, type: int, derivation: {value: {function: {name: id, args: {x: SRC.ID}}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";
            let decoded = yamaa_adapters::yaml_decode::decode_yaml(raw).unwrap();
            let model = yamaa_core::schema::SpecificationDocument::admit(decoded.document)
                .unwrap()
                .unwrap();
            let checked = project_domain::check_owned(&model, environment).unwrap();
            let trace = Rc::new(RefCell::new(vec![]));
            let payload = Rc::new(());
            let mut activation = Activation {
                trace: Rc::clone(&trace),
                payload: Rc::clone(&payload),
                live_failed: false,
                lock_failed: false,
            };
            let mut source = Source {
                trace: Rc::clone(&trace),
                bytes: Arc::from(b"ID\n1\n".as_slice()),
                reads: 0,
            };
            let mut decoder = Decoder::<Payload>::default();
            let mut attempt = CapturedAttempt::new(checked.compiled().source());
            for created in [Some(1), Some(0)] {
                checked
                    .build_into(
                        &mut activation,
                        b"",
                        &mut source,
                        &mut decoder,
                        &mut attempt,
                        None,
                        yamaa_core::application_observer::Scope::new(
                            &yamaa_core::application_observer::Noop,
                        ),
                    )
                    .unwrap();
                assert_eq!(attempt.sources[0].read.snapshots_created, created);
                assert_eq!(
                    attempt
                        .result
                        .as_ref()
                        .unwrap()
                        .result
                        .as_ref()
                        .unwrap()
                        .dataset
                        .rows(),
                    &[vec![Value::Int(1), Value::Int(1)]]
                );
                assert_eq!(
                    attempt.sources[0]
                        .table
                        .as_ref()
                        .unwrap()
                        .cell(0, 0)
                        .unwrap(),
                    ValueRef::Int(1)
                );
                assert!(Arc::ptr_eq(
                    attempt.sources[0].snapshot.as_ref().unwrap(),
                    &source.bytes
                ));
            }
            let once = ["lock", "bind", "case", "case", "inspect", "capture", "live"];
            assert_eq!(
                &*trace.borrow(),
                &once.into_iter().chain(once).collect::<Vec<_>>()
            );
            activation.live_failed = true;
            checked
                .build_into(
                    &mut activation,
                    b"",
                    &mut source,
                    &mut decoder,
                    &mut attempt,
                    None,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                )
                .unwrap();
            let ExecutionError::ProjectFunction { path, error, .. } = attempt
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap_err()
                .as_ref()
            else {
                panic!("project failure")
            };
            assert_eq!(path, "columns.VALUE.derivation.function");
            let FailureKind::CallFailed(Payload(original)) = &error.kind else {
                panic!("original host payload")
            };
            assert!(Rc::ptr_eq(original, &payload));
            assert_eq!(source.reads, 1);
            activation.lock_failed = true;
            trace.borrow_mut().clear();
            let Failure::Lock(Payload(original)) = checked
                .build_into(
                    &mut activation,
                    b"",
                    &mut source,
                    &mut decoder,
                    &mut attempt,
                    None,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                )
                .unwrap_err()
            else {
                panic!("original lock failure")
            };
            assert!(Rc::ptr_eq(&original, &payload));
            assert!(attempt.sources.is_empty());
            assert!(matches!(attempt.result, Err(PortError::Incomplete)));
            assert_eq!(&*trace.borrow(), &["lock"]);
        }
    }
}
