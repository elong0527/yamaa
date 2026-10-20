#![cfg(feature = "yaml")]
//! Declaration contracts.
#[allow(dead_code)]
mod support;
mod declaration {
    mod schema_capture {
        use yamaa_adapters::json;
        use yamaa_adapters::{
            specification_diagnostics::{capture_failure, Rejection},
            specification_source::{CapturedSchema, Error, Source},
            yaml_decode::SourcePosition,
        };
        use yamaa_core::json::Value;
        use yamaa_core::schema::{
            BundleError, BundleIssue, BundleIssueKind, DocumentNode as N, NormalizationError,
            SchemaSource,
        };
        const SHARED:&str="version: '1.0'\nincludes: []\nshared:\n  - id: {type: str, required: true}\n  - label: {type: str, default: untitled}\n";
        const ROOT:&str="version: '1.0'\nincludes: [schema_s.yaml]\nroot_class:\n  - fields_from: shared\n  - schema_version: {type: str}\n";
        fn source(identity: &str, bytes: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.as_bytes().to_vec(),
            }
        }
        fn closure() -> Vec<Source> {
            vec![source("schema_s.yaml", SHARED), source("r", ROOT)]
        }
        #[test]
        fn original_sources_locations_reused_defaults_and_fresh_normalization_are_retained() {
            for _ in 0..2 {
                let sources = closure();
                let arena = sources.as_ptr();
                let bytes = sources.iter().map(|s| s.bytes.as_ptr()).collect::<Vec<_>>();
                let identities = sources
                    .iter()
                    .map(|s| s.identity.as_ptr())
                    .collect::<Vec<_>>();
                let schema = CapturedSchema::admit(sources, 1).unwrap();
                assert_eq!(schema.sources().as_ptr(), arena);
                assert_eq!(
                    schema
                        .sources()
                        .iter()
                        .map(|s| s.identity.as_str())
                        .collect::<Vec<_>>(),
                    ["schema_s.yaml", "r"]
                );
                assert_eq!(schema.sources()[0].bytes, SHARED.as_bytes());
                assert_eq!(schema.sources()[1].bytes, ROOT.as_bytes());
                for i in 0..2 {
                    assert_eq!(schema.sources()[i].bytes.as_ptr(), bytes[i]);
                    assert_eq!(schema.sources()[i].identity.as_ptr(), identities[i]);
                }
                let descriptor = schema.structure().root_class().fields[1].descriptor;
                let field = &schema.structure().descriptors()[descriptor];
                assert_eq!((field.module, field.path.as_str()), (0, "root_class.label"));
                let node = field.descriptor.default_node().unwrap();
                let offset = SHARED.find("untitled").unwrap();
                let line_start = SHARED[..offset].rfind('\n').unwrap() + 1;
                assert_eq!(
                    schema.locations()[0][node],
                    SourcePosition {
                        offset,
                        line: 5,
                        column: offset - line_start + 1
                    }
                );
                assert_eq!(
                    schema.structure().modules()[0].document.nodes()[node],
                    N::Text("untitled".into())
                );
                let raw = source("domain.yaml", "schema_version: '1.0'\nid: ID01\n");
                let raw_pointer = raw.bytes.as_ptr();
                let prepared = schema.prepare_structural(raw).unwrap();
                assert_eq!(prepared.source().bytes.as_ptr(), raw_pointer);
                let d = &prepared.normalized().document;
                let label = d.field(d.root(), "label").unwrap();
                assert_eq!(d.nodes()[label], N::Text("untitled".into()));
                assert_eq!(
                    prepared.normalized().origins[label].source,
                    SchemaSource::Default {
                        module: 0,
                        descriptor
                    }
                );
                assert_eq!(prepared.normalized().origins[label].node, node);
            }
        }

        #[test]
        fn physical_decode_precedes_invalid_entry_and_earlier_bundle_semantics() {
            for entry in [0, usize::MAX] {
                let error = CapturedSchema::admit(
                    vec![
                        source("same", "ordinary scalar\n"),
                        source("same", "[unclosed"),
                        source("late", "[later unclosed"),
                    ],
                    entry,
                )
                .unwrap_err();
                assert!(matches!(error,Error::Decode{ref identity,..} if identity=="same"));
            }
            let error = CapturedSchema::admit(closure(), usize::MAX).unwrap_err();
            assert!(
                matches!(error,Error::Bundle(BundleError::Invalid(ref issues)) if issues==&[BundleIssue{module:usize::MAX,path:"$".into(),kind:BundleIssueKind::ModuleName}])
            );
        }
        #[test]
        fn complete_original_codec_reports_survive_schema_closure_admission() {
            let mut count = 0;
            for row in yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/yaml_transport.json"
            ))
            .unwrap()
            .iter()
            {
                let mut expected: Value = row["expected"].clone();
                if expected["outcome"]["status"] != "invalid" {
                    continue;
                }
                for finding in expected["outcome"]["diagnostics"].as_array_mut().unwrap() {
                    finding["phase"] = json!("validation");
                    finding["requirement"] = json!(match finding["condition"].as_str().unwrap() {
                        "non_ascii_source" => "REQ-0705",
                        "invalid_text" => "REQ-0029",
                        "invalid_yaml" => "REQ-0280",
                        condition => panic!("unexpected original source condition: {condition}"),
                    });
                    if finding["condition"] == "non_ascii_source" {
                        finding["context"]["path"] = json!("source.yaml");
                    }
                }
                let bytes: Vec<u8> = if let Some(text) = row["source"].as_str() {
                    text.as_bytes().to_vec()
                } else {
                    let hex = row["source"]["hex"].as_str().unwrap();
                    hex.as_bytes()
                        .chunks_exact(2)
                        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                        .collect()
                };
                let error = CapturedSchema::admit(
                    vec![Source {
                        identity: "source.yaml".into(),
                        bytes,
                    }],
                    usize::MAX,
                )
                .unwrap_err();
                assert_eq!(
                    crate::support::findings(capture_failure(&error)),
                    expected["outcome"]["diagnostics"],
                    "{}",
                    row["id"].as_str().unwrap()
                );
                count += 1;
            }
            assert_eq!(count, 8);
        }
        #[test]
        fn complete_bundle_issue_order_and_later_invalid_default_gate_are_unchanged() {
            let error=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - first: {type: unknown_one}\n  - second: {type: unknown_two}\n")],0).unwrap_err();
            let Error::Bundle(ref bundle) = error else {
                panic!("full typed bundle failure expected")
            };
            assert_eq!(
                bundle,
                &BundleError::Invalid(vec![
                    BundleIssue {
                        module: 0,
                        path: "root_class.first".into(),
                        kind: BundleIssueKind::UnknownType("unknown_one".into())
                    },
                    BundleIssue {
                        module: 0,
                        path: "root_class.second".into(),
                        kind: BundleIssueKind::UnknownType("unknown_two".into())
                    },
                ])
            );
            assert_eq!(
                capture_failure(&error),
                Rejection::Fault(yamaa_core::application_issue::rejected(
                    yamaa_core::application_issue::Stage::Capture,
                    yamaa_core::application_issue::Fault::SchemaBundle,
                ))
            );
            let schema=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - schema_version: {type: str}\n  - value: {type: int, default: invalid}\n")],0).unwrap();
            assert!(matches!(
                schema.prepare_structural(source("root.yaml", "schema_version: '1.0'\n")),
                Err(Error::Normalize(NormalizationError::Defaults(_)))
            ));
        }

        #[test]
        fn fixed_yaml_file_size_boundary_precedes_physical_decoding() {
            use yamaa_adapters::yaml_decode::DecodeFailure;
            use yamaa_core::limits::{LimitExceeded, Limits};
            let prefix =
                "version: '1.0'\nincludes: []\nroot_class: [{schema_version: {type: str}}]\n#";
            for size in [
                Limits::SPEC_BYTES - 1,
                Limits::SPEC_BYTES,
                Limits::SPEC_BYTES + 1,
            ] {
                let mut bytes = prefix.as_bytes().to_vec();
                bytes.resize(size, b'x');
                let result = CapturedSchema::admit(
                    vec![Source {
                        identity: "authored-schema.yaml".into(),
                        bytes,
                    }],
                    0,
                );
                if size <= Limits::SPEC_BYTES {
                    let schema = result.unwrap();
                    assert_eq!(schema.sources()[0].bytes.len(), size);
                    assert_eq!(schema.structure().root_class().name, "root_class");
                } else {
                    assert!(
                        matches!(result, Err(Error::Decode { identity, error: DecodeFailure::Limit(LimitExceeded {
                resource: "spec_bytes", limit: Limits::SPEC_BYTES, actual: Some(actual)
            }) }) if identity == "authored-schema.yaml" && actual == size)
                    );
                }
            }
        }
    }
    mod schema_declaration_fixture {
        use crate::support::schema_declaration as fixture;
        use yamaa_adapters::json;

        const SCHEMA: &str = "version: '1.0'\nroot_class:\n  - needed: {type: str, required: true}\n  - count: {type: int}\n  - items: {type: 'list[int]'}\n";

        #[test]
        fn custom_versionless_class_preserves_ordered_required_type_and_unknown_findings() {
            assert_eq!(
                fixture::observe(SCHEMA, "count: true\nextra: null\n"),
                json!({"issues": [
                    {"phase":"validation", "condition":"missing_required_field", "requirement":"REQ-0285",
                        "spec_paths":["needed"], "context":{"field":"needed", "class":"root_class"}},
                    {"phase":"validation", "condition":"invalid_field_type", "requirement":"REQ-0287",
                        "spec_paths":["count"], "context":{"expected":"int", "actual":"bool"}},
                    {"phase":"validation", "condition":"unknown_field", "requirement":"REQ-0285",
                        "spec_paths":["extra"], "context":{"field":"extra", "class":"root_class"}}
                ]})
            );
        }

        #[test]
        fn custom_list_member_path_and_integer_identity_are_native_owned() {
            assert_eq!(
                fixture::observe(
                    SCHEMA,
                    "needed: kept\nitems: [1, false, 9223372036854775808]\n"
                ),
                json!({"issues": [{"phase":"validation", "condition":"invalid_field_type",
            "requirement":"REQ-0287", "spec_paths":["items[1]"],
            "context":{"expected":"int", "actual":"bool"}}]})
            );
            assert_eq!(
                fixture::observe(
                    SCHEMA,
                    "needed: kept\ncount: 9223372036854775808\nitems: [1, 2]\n"
                ),
                json!({"issues": []})
            );
        }

        #[test]
        fn declared_document_version_is_a_custom_field_without_a_shipped_version_gate() {
            let schema = "version: '1.0'\nroot_class:\n  - schema_version: {type: str, required: true}\n  - name: {type: str, required: true}\n";
            assert_eq!(
                fixture::observe(schema, "schema_version: custom\nname: kept\n"),
                json!({"issues": []})
            );
        }

        #[test]
        fn named_environment_class_uses_the_complete_authored_include_closure() {
            let sources = vec![
        (
            "schema_environment.yaml".into(),
            "version: '1.0'\nincludes: [schema_shared.yaml]\nenvironment_class:\n  - schema_version: {type: str, required: true}\n  - language: {type: str, values: [python, r]}\n  - lock: {type: project_path}\n".into(),
        ),
        (
            "schema_shared.yaml".into(),
            "version: '1.0'\nproject_path: {type: str, min_length: 1}\n".into(),
        ),
    ];
            let observed = fixture::observe_input(
                fixture::SchemaInput::Modules(sources),
                "schema_version: '1.0'\nlanguage: false\nlock: owned.lock\n",
                Some("environment_class"),
            );
            assert_eq!(
                observed,
                json!({"issues": [{"phase":"validation", "condition":"invalid_field_type",
        "requirement":"REQ-0287", "spec_paths":["language"],
        "context":{"expected":"str", "actual":"bool"}}]})
            );
        }
    }
    mod schema_diagnostic_projection {
        use yamaa_adapters::json;
        use yamaa_adapters::{
            specification_diagnostics::capture_failure,
            specification_source::{CapturedSchema, Error, Source},
        };
        use yamaa_core::json::Value;

        const BIG: &str = "922337203685477580812345678901234567890";
        fn source(identity: &str, text: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }

        #[test]
        fn captured_structural_projection_preserves_complete_ordered_constraint_outcome_and_authority(
        ) {
            let schema = CapturedSchema::admit_root(vec![source("schema.yaml", &format!(
        "version: '1.0'\npayload:\n  - schema_version: {{type: str}}\n  - alpha: {{type: str, values: [z, a, z], pattern: '^OK$', min_length: {BIG}}}\n  - items: {{type: 'list[int]', size: {BIG}}}\n"))],
        0, "payload").unwrap();
            let raw = "schema_version: '1.0'\nalpha: \"bad\\\"\\n\\u03BB\"\nitems: [1]\n";
            let error = schema
                .prepare_structural(source("held/entry.yaml", raw))
                .unwrap_err();
            drop(schema);
            let Error::Findings(captured) = &error else {
                panic!("held semantic report: {:?}", capture_failure(&error))
            };
            assert_eq!(captured.source().identity, "held/entry.yaml");
            assert_eq!(captured.source().bytes, raw.as_bytes());
            let held_bytes = captured.source().bytes.as_ptr();
            let held_nodes = captured.document().unwrap().nodes().as_ptr();
            let held_findings = captured.findings().as_ptr();
            let expected: Value = yamaa_adapters::json_wire::decode(&format!(r#"[
        {{"phase":"validation","condition":"value_not_permitted","requirement":"REQ-0287","spec_paths":["alpha"],"context":{{"value":"bad\"\nλ","permitted":["z","a","z"]}}}},
        {{"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["alpha"],"context":{{"value":"bad\"\nλ","pattern":"^OK$"}}}},
        {{"phase":"validation","condition":"minimum_length","requirement":"REQ-0287","spec_paths":["alpha"],"context":{{"minimum":{BIG}}}}},
        {{"phase":"validation","condition":"invalid_size","requirement":"REQ-0287","spec_paths":["items"],"context":{{"size":{BIG}}}}}
    ]"#)).unwrap();
            for _ in 0..2 {
                let rows = crate::support::findings(capture_failure(&error));
                assert_eq!(rows, expected);
                let encoded = yamaa_adapters::json_wire::encode(&rows);
                assert!(encoded.contains(&format!("\"minimum\":{BIG}")));
                assert!(encoded.contains(&format!("\"size\":{BIG}")));
                assert_eq!(captured.source().bytes.as_ptr(), held_bytes);
                assert_eq!(captured.document().unwrap().nodes().as_ptr(), held_nodes);
                assert_eq!(captured.findings().as_ptr(), held_findings);
            }
        }

        #[test]
        fn large_ordered_findings_retain_complete_values_and_original_authority() {
            let schema = CapturedSchema::admit_root(vec![source("schema.yaml",
        "version: '1.0'\npayload:\n  - schema_version: {type: str}\n  - first: {type: str, values: [OK]}\n  - last: {type: str, values: [OK]}\n")],
        0, "payload").unwrap();
            let raw = format!(
                "schema_version: '1.0'\nfirst: '{}'\nlast: '{}'\n",
                "x".repeat(1_400_000),
                "y".repeat(1_400_000)
            );
            let error = schema
                .prepare_structural(source("held/entry.yaml", &raw))
                .unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!(
                    "both language findings remain held: {:?}",
                    capture_failure(&error)
                )
            };
            assert_eq!(captured.findings().len(), 2);
            assert_eq!(captured.findings()[0].path, "first");
            assert_eq!(captured.findings()[1].path, "last");
            let original = captured.document().unwrap().nodes().as_ptr();
            let expected = json!([
                {"phase":"validation","condition":"value_not_permitted","requirement":"REQ-0287","spec_paths":["first"],"context":{"value":"x".repeat(1_400_000),"permitted":["OK"]}},
                {"phase":"validation","condition":"value_not_permitted","requirement":"REQ-0287","spec_paths":["last"],"context":{"value":"y".repeat(1_400_000),"permitted":["OK"]}}
            ]);
            for _ in 0..2 {
                assert_eq!(crate::support::findings(capture_failure(&error)), expected);
                assert_eq!(captured.document().unwrap().nodes().as_ptr(), original);
                assert_eq!(captured.source().bytes, raw.as_bytes());
                assert_eq!(captured.findings().len(), 2);
            }
        }
    }
    mod shipped_schema {
        //! Shipped schema identity and raw preparation without host schema interpretation.

        use yamaa_adapters::{
            shipped_schema,
            specification_source::{Error, InheritanceError, InheritancePort, Source},
        };
        use yamaa_core::inheritance::{Source as Identity, SourceError};

        struct NoParents;
        impl InheritancePort for NoParents {
            type Error = ();
            fn canonicalize(&mut self, _: &str, _: &str) -> Result<Identity, SourceError<()>> {
                panic!("entry rejection or standalone preparation must not resolve a parent")
            }
            fn capture(&mut self, _: &Identity) -> Result<Vec<u8>, SourceError<()>> {
                panic!("entry rejection or standalone preparation must not read a parent")
            }
            fn rebase(&mut self, _: &Identity, _: &Identity, _: &str) -> Result<String, ()> {
                panic!("entry rejection or standalone preparation must not rebase a parent")
            }
        }
        fn source(text: &str) -> Source {
            Source {
                identity: "arbitrary/location/domain.yaml".into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";

        #[test]
        fn shipped_closure_retains_every_authoritative_byte_and_admits_without_a_directory() {
            let schema = shipped_schema::capture().unwrap();
            assert_eq!(schema.structure().version(), "1.0");
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../yaml");
            let mut expected = std::fs::read_dir(&root)
                .unwrap()
                .map(|p| p.unwrap().file_name().into_string().unwrap())
                .filter(|p| p == "schema.yaml" || p.starts_with("schema_") && p.ends_with(".yaml"))
                .filter(|p| {
                    !matches!(
                        p.as_str(),
                        "schema_environment.yaml" | "schema_project.yaml"
                    )
                })
                .collect::<Vec<_>>();
            expected.sort();
            let mut actual = schema
                .sources()
                .iter()
                .map(|s| s.identity.clone())
                .collect::<Vec<_>>();
            actual.sort();
            assert_eq!(actual, expected);
            for module in schema.sources() {
                assert_eq!(
                    module.bytes,
                    std::fs::read(root.join(&module.identity)).unwrap()
                );
            }
            let prepared =
                shipped_schema::prepare(source(VALID), "domain.yaml".into(), &mut NoParents)
                    .unwrap();
            assert!(prepared.inheritance().is_none());
            assert_eq!(prepared.source().bytes, VALID.as_bytes());
            assert!(prepared.parents().is_empty());
        }

        #[test]
        fn shipped_project_and_environment_roots_retain_their_separate_authoritative_closures() {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../yaml");
            let schemas = shipped_schema::capture_project().unwrap();
            let environment = shipped_schema::capture_environment().unwrap();
            for (schema, class, entry) in [
                (&schemas.project, "project_class", "schema_project.yaml"),
                (
                    &schemas.environment,
                    "environment_class",
                    "schema_environment.yaml",
                ),
                (&environment, "environment_class", "schema_environment.yaml"),
            ] {
                assert_eq!(schema.structure().version(), "1.0");
                assert_eq!(schema.structure().root_class().name, class);
                assert_eq!(
                    schema
                        .sources()
                        .iter()
                        .map(|s| s.identity.as_str())
                        .collect::<Vec<_>>(),
                    [entry, "schema_shared.yaml"]
                );
                for module in schema.sources() {
                    assert_eq!(
                        module.bytes,
                        std::fs::read(root.join(&module.identity)).unwrap()
                    );
                }
            }
            // Codelist sources belong to the project root, function definitions to
            // the environment root; neither root closes over the other's classes.
            let malformed = || source("[unclosed");
            assert!(matches!(
                schemas
                    .project
                    .prepare_named_class(malformed(), "codelist_source_class"),
                Err(Error::Decode { .. })
            ));
            assert!(matches!(
                schemas
                    .environment
                    .prepare_named_class(malformed(), "function_definition_class"),
                Err(Error::Decode { .. })
            ));
            assert!(matches!(
                schemas
                    .project
                    .prepare_named_class(malformed(), "function_definition_class"),
                Err(Error::UnknownClass(_))
            ));
            assert!(matches!(
                schemas
                    .environment
                    .prepare_named_class(malformed(), "codelist_source_class"),
                Err(Error::UnknownClass(_))
            ));
        }

        #[test]
        fn unsupported_or_missing_version_is_rejected_before_any_parent_authority() {
            for text in [
                VALID.replace("'1.0'", "'99.0'"),
                VALID.replace("schema_version: '1.0'\n", ""),
            ] {
                let result =
                    shipped_schema::prepare(source(&text), "domain.yaml".into(), &mut NoParents);
                let Err(InheritanceError::Entry(Error::Findings(captured))) = result else {
                    panic!("expected shared schema version finding");
                };
                assert_eq!(captured.findings().len(), 1);
                assert_eq!(
                    captured.findings()[0].definition().condition,
                    "schema_version_mismatch"
                );
            }
            let malformed =
                shipped_schema::prepare(source("[unclosed"), "domain.yaml".into(), &mut NoParents);
            assert!(matches!(
                malformed,
                Err(InheritanceError::Entry(Error::Decode { .. }))
            ));
            // A null parent is absent: the entry takes the standalone route.
            let standalone = shipped_schema::prepare(
                source(&(VALID.to_owned() + "parent: null\n")),
                "domain.yaml".into(),
                &mut NoParents,
            )
            .unwrap();
            assert!(standalone.inheritance().is_none());
            assert!(standalone.parents().is_empty());
            // Any other written parent selects inheritance even when invalid; no fallback.
            let inherited = shipped_schema::prepare(
                source(&(VALID.to_owned() + "parent: [parent.yaml]\n")),
                "domain.yaml".into(),
                &mut NoParents,
            );
            assert!(matches!(
                inherited,
                Err(InheritanceError::Preparation { .. })
            ));
        }

        #[test]
        fn standalone_findings_retain_source_and_resolve_context_without_host_interpretation() {
            use yamaa_adapters::json;
            use yamaa_adapters::specification_diagnostics::capture_failure;
            let cases = [
                (
                    VALID.replace("'1.0'", "'99.0'"),
                    json!({"phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0281","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0"}}),
                ),
                (
                    VALID.replace("schema_version: '1.0'\n", ""),
                    json!({"phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0281","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":null}}),
                ),
                (
                    VALID.replace("domain: TEST", "domain: bad-name"),
                    json!({"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$"}}),
                ),
                (
                    VALID.replace("type: int", "type: unknown"),
                    json!({"phase":"validation","condition":"value_not_permitted","requirement":"REQ-0012","spec_paths":["columns.ID.type"],"context":{"value":"unknown","permitted":["str","int","float","date","datetime"]}}),
                ),
                (
                    VALID.replace("path: output.csv", "path: ''"),
                    json!({"phase":"validation","condition":"minimum_length","requirement":"REQ-0287","spec_paths":["output.path"],"context":{"minimum":1}}),
                ),
            ];
            for (text, expected) in cases {
                let schema = shipped_schema::capture().unwrap();
                let error = schema.prepare_standalone(source(&text)).unwrap_err();
                drop(schema);
                let Error::Findings(captured) = &error else {
                    panic!("expected captured findings");
                };
                assert_eq!(captured.source().bytes, text.as_bytes());
                assert!(captured.document().is_some());
                assert_eq!(captured.schema().structure().version(), "1.0");
                assert_eq!(
                    crate::support::findings(capture_failure(&error)),
                    json!([expected])
                );
            }
            let integer = "922337203685477580812345678901234567890";
            let error = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(source(&VALID.replace("'1.0'", integer)))
                .unwrap_err();
            let actual = crate::support::findings(capture_failure(&error));
            assert!(yamaa_adapters::json_wire::encode(&actual)
                .contains(&format!("\"actual\":{integer}")));
            assert_eq!(
                yamaa_adapters::json_wire::encode(&actual[0]["context"]["actual"]),
                integer
            );
        }

        #[test]
        fn window_expansion_findings_use_the_retained_normalized_arena() {
            use yamaa_adapters::json;
            use yamaa_adapters::specification_diagnostics::capture_failure;
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/negative-unknown-window");
            let raw = std::fs::read(root.join("spec.yaml")).unwrap();
            let error = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(Source {
                    identity: "spec.yaml".into(),
                    bytes: raw.clone(),
                })
                .unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!("expected window findings");
            };
            assert_eq!(captured.source().bytes, raw);
            let document = captured.document().unwrap();
            // Shorthand expansion changes this pass input from the authored arena.
            let decoded = yamaa_adapters::yaml_decode::decode_yaml(&raw).unwrap();
            assert_ne!(document, &decoded.document);
            // Independent expected/error.yaml is unchanged.
            assert_eq!(
                crate::support::findings(capture_failure(&error)),
                json!([{"phase":"validation","condition":"unknown_window","spec_paths":["columns.VISITSEQ.derivation.row_number.window"],"requirement":"REQ-1253","context":{"window":"VISITS_ORDER"}}])
            );
        }

        #[test]
        fn large_invalid_version_retains_its_complete_language_finding() {
            use yamaa_adapters::json;
            use yamaa_adapters::specification_diagnostics::capture_failure;
            let version = "x".repeat(2_800_000);
            let text = format!("schema_version: '{version}'\n");
            let error = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(source(&text))
                .unwrap_err();
            assert!(matches!(&error, Error::Findings(_)));
            assert_eq!(
                crate::support::findings(capture_failure(&error)),
                json!([{"phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0281","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":version}}])
            );
        }

        #[test]
        fn source_failures_preserve_independent_decoder_findings_before_any_host_effect() {
            use yamaa_adapters::json;
            use yamaa_adapters::specification_diagnostics::inheritance_failure_ref;
            use yamaa_core::json::Value;
            let mut count = 0;
            for row in yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/yaml_transport.json"
            ))
            .unwrap()
            .iter()
            {
                let mut expected: Value = row["expected"].clone();
                if expected["outcome"]["status"] != "invalid" {
                    continue;
                }
                for finding in expected["outcome"]["diagnostics"].as_array_mut().unwrap() {
                    finding["phase"] = json!("validation");
                    finding["requirement"] = json!(match finding["condition"].as_str().unwrap() {
                        "non_ascii_source" => "REQ-0705",
                        "invalid_text" => "REQ-0029",
                        "invalid_yaml" => "REQ-0280",
                        condition => panic!("unexpected original source condition: {condition}"),
                    });
                    if finding["condition"] == "non_ascii_source" {
                        finding["context"]["path"] = json!("source.yaml");
                    }
                }
                let bytes: Vec<u8> = if let Some(text) = row["source"].as_str() {
                    text.as_bytes().to_vec()
                } else {
                    let hex = row["source"]["hex"].as_str().unwrap();
                    hex.as_bytes()
                        .chunks_exact(2)
                        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                        .collect()
                };
                let error = shipped_schema::prepare(
                    Source {
                        identity: "source.yaml".into(),
                        bytes,
                    },
                    "source.yaml".into(),
                    &mut NoParents,
                )
                .unwrap_err();
                assert_eq!(
                    crate::support::findings(inheritance_failure_ref(&error).unwrap()),
                    expected["outcome"]["diagnostics"],
                    "{}",
                    row["id"].as_str().unwrap()
                );
                count += 1;
            }
            assert_eq!(count, 8);
        }

        struct ParentSource {
            raw: Vec<u8>,
            reads: usize,
        }
        impl InheritancePort for ParentSource {
            type Error = ();
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<Identity, SourceError<()>> {
                assert_eq!((declaring, written), ("entry.yaml", "parent.yaml"));
                Ok(Identity {
                    identity: "parent.yaml".into(),
                    display_path: "parent.yaml".into(),
                })
            }
            fn capture(&mut self, _: &Identity) -> Result<Vec<u8>, SourceError<()>> {
                self.reads += 1;
                Ok(self.raw.clone())
            }
            fn rebase(&mut self, _: &Identity, _: &Identity, written: &str) -> Result<String, ()> {
                Ok(written.into())
            }
        }
        fn inherited_error(entry: &str, parent: &str) -> InheritanceError<()> {
            let mut port = ParentSource {
                raw: parent.as_bytes().to_vec(),
                reads: 0,
            };
            let error = shipped_schema::prepare(
                Source {
                    identity: "entry.yaml".into(),
                    bytes: entry.as_bytes().to_vec(),
                },
                "entry.yaml".into(),
                &mut port,
            )
            .unwrap_err();
            if port.reads != 1 {
                panic!(
                    "unexpected early failure: {:?}",
                    yamaa_adapters::specification_diagnostics::inheritance_failure_ref(&error)
                        .unwrap()
                );
            }
            error
        }
        #[test]
        fn inherited_layer_and_composed_findings_resolve_without_exposing_context_arenas() {
            use yamaa_adapters::json;
            use yamaa_adapters::specification_diagnostics::inheritance_failure_ref;
            let entry = format!("{VALID}parent: parent.yaml\n");
            let invalid_parent = "schema_version: '1.0'\ndomain: bad-name\n";
            assert_eq!(
                crate::support::findings(
                    inheritance_failure_ref(&inherited_error(&entry, invalid_parent)).unwrap()
                ),
                json!([{"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","source":"parent.yaml","entry":"entry.yaml"}}])
            );
            let empty_parent = "schema_version: '1.0'\n";
            let missing = inherited_error(&entry.replace("domain: TEST\n", ""), empty_parent);
            let InheritanceError::Preparation { error, .. } = &missing else {
                panic!("expected lifecycle failure");
            };
            let yamaa_core::inheritance_preparation::Error::Normalize(context) = error.as_ref()
            else {
                panic!("expected normalization failure");
            };
            assert!(context.input.is_some());
            assert_eq!(
                crate::support::findings(inheritance_failure_ref(&missing).unwrap()),
                json!([{"phase":"validation","condition":"missing_required_field","requirement":"REQ-0285","spec_paths":["domain"],"context":{"field":"domain","class":"root_class"}}])
            );
            let unknown = inherited_error(
                &entry.replace(
                    "derivation: SRC.ID",
                    "derivation: {compute: {expr: UNKNOWN}}",
                ),
                empty_parent,
            );
            assert_eq!(
                crate::support::findings(inheritance_failure_ref(&unknown).unwrap()),
                json!([{"phase":"validation","condition":"unknown_reference","requirement":"REQ-0070","spec_paths":["columns.ID.derivation"],"context":{"column":"ID","dependency":"UNKNOWN"}}])
            );
        }

        #[test]
        fn shipped_project_version_override_reaches_the_same_owned_submission_model() {
            use yamaa_core::submission_document;
            let schema = shipped_schema::capture_project().unwrap().project;
            for (family, guide) in [("sdtm", "SDTMIG"), ("adam", "ADaMIG"), ("send", "SENDIG")] {
                for version in [None, Some("1.0")] {
                    let mut text = format!(
                "schema_version: '1.0'\nenvironments: {{python: python/environment.yaml}}\nstudy: {{id: Study, name: Study, description: Study, protocol_name: Protocol}}\n{family}:\n  standard: {{name: {guide}, version: '1.0', status: Final}}\n  specs: [absent.yaml]\n  define: absent/define.xml\n"
            );
                    if let Some(version) = version {
                        text.push_str(&format!("  source_system_version: '{version}'\n"));
                    }
                    let captured = schema.prepare_structural(source(&text)).unwrap();
                    let input = &captured.normalized().document;
                    let owned = submission_document::decode(input, input.root()).unwrap();
                    drop(captured);
                    let selected = owned.sections()[0].source_versions("0.3.0").unwrap();
                    assert_eq!(selected.metadata(), version.unwrap_or("0.3.0"));
                    assert_eq!(selected.installed(), "0.3.0");
                    assert_eq!(selected.override_origin().is_some(), version.is_some());
                }
            }
        }
    }
    mod standalone_preparation {
        //! Existing YAML representation and independent complete outcomes around engine delegation.
        use std::sync::Arc;
        use yamaa_adapters::json;
        use yamaa_adapters::{
            specification_diagnostics::capture_failure,
            specification_source::{
                CapturedSchema, Error, InheritanceError, InheritancePort, Source,
            },
            yaml_decode::{decode_yaml, DecodeFailure},
        };
        use yamaa_core::inheritance::{Source as Identity, SourceError};
        use yamaa_core::json::Value;
        use yamaa_core::schema::{DocumentNode as N, NormalizationError, SchemaSource};

        const SCHEMA: &str = "version: '1.0'\nroot_class:\n  - schema_version: {type: str}\n  - domain: {type: [str, int], default: DEFAULT}\n  - input: {type: dict}\n  - keys: {type: 'list[str]'}\n  - output: {type: dict}\n  - columns: {type: 'list[column_class]'}\n  - windows: {type: 'dict[str, window_spec]'}\ncolumn_class:\n  - name: {type: str}\n  - type: {type: str}\n  - derivation: {type: handled_expression_class}\nhandled_expression_class:\n  - value: {type: expression}\nexpression: {registry: expressions}\nexpressions:\n  row_number:\n    - window: {type: window_selection}\nwindow_selection: {type: [str, window_spec]}\nwindow_spec:\n  - group_by: {type: 'list[str]'}\n  - order_by: {type: 'list[order_term]'}\norder_term:\n  - variable: {type: str}\n";
        const VALID: &str = "schema_version: '1.0'\ndomain: X\ninput: {}\nkeys: []\noutput: {path: output.csv, columns: []}\ncolumns: []\n";
        fn source(text: &str) -> Source {
            Source {
                identity: "retained/spec.yaml".into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        fn schema() -> Arc<CapturedSchema> {
            CapturedSchema::admit(
                vec![Source {
                    identity: "schema.yaml".into(),
                    bytes: SCHEMA.as_bytes().to_vec(),
                }],
                0,
            )
            .unwrap()
        }
        fn outcome(error: &Error) -> Value {
            crate::support::findings(capture_failure(error))
        }
        struct NoParents;
        impl InheritancePort for NoParents {
            type Error = std::rc::Rc<usize>;
            fn canonicalize(
                &mut self,
                _: &str,
                _: &str,
            ) -> Result<Identity, SourceError<Self::Error>> {
                panic!("standalone path must not canonicalize")
            }
            fn capture(&mut self, _: &Identity) -> Result<Vec<u8>, SourceError<Self::Error>> {
                panic!("standalone path must not capture")
            }
            fn rebase(
                &mut self,
                _: &Identity,
                _: &Identity,
                _: &str,
            ) -> Result<String, Self::Error> {
                panic!("standalone path must not rebase")
            }
        }

        #[test]
        fn adapter_moves_source_allocations_and_retains_original_physical_locations_and_defaults() {
            let text = VALID.replace("domain: X\n", "");
            let entry = source(&text);
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let schema = schema();
            let prepared = schema.prepare_standalone(entry).unwrap();
            assert_eq!(prepared.source().bytes.as_ptr(), bytes);
            assert_eq!(prepared.source().identity.as_ptr(), identity);
            assert_eq!(prepared.source().bytes, text.as_bytes());
            let decoded = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(prepared.raw().document, decoded.document);
            assert_eq!(prepared.raw().locations, decoded.locations);
            assert_eq!(
                prepared.model().document().nodes()[prepared
                    .model()
                    .document()
                    .field(prepared.model().document().root(), "domain")
                    .unwrap()],
                N::Text("DEFAULT".into())
            );
            let descriptor = schema
                .structure()
                .class_named("root_class")
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "domain")
                .unwrap()
                .descriptor;
            let default = schema.structure().descriptors()[descriptor]
                .descriptor
                .default_node()
                .unwrap();
            let domain = prepared
                .model()
                .document()
                .field(prepared.model().document().root(), "domain")
                .unwrap();
            assert_eq!(
                prepared.origins()[domain].source,
                SchemaSource::Default {
                    module: 0,
                    descriptor
                }
            );
            assert_eq!(prepared.origins()[domain].node, default);
            assert!(!prepared.origins()[domain].generated);
            assert!(prepared.parents().is_empty());
            assert!(prepared.windows().is_empty());
            assert!(prepared.inheritance().is_none());
        }

        #[test]
        fn both_semantic_passes_keep_source_identity_and_independent_complete_reports() {
            let schema = schema();
            let text = VALID.replace("'1.0'", "92233720368547758081234567890");
            let entry = source(&text);
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let error = schema.prepare_standalone(entry).unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!()
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            let decoded = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(captured.document(), Some(&decoded.document));
            let expected:Value=yamaa_adapters::json_wire::decode("{\"phase\":\"validation\",\"condition\":\"schema_version_mismatch\",\"requirement\":\"REQ-0281\",\"spec_paths\":[\"schema_version\"],\"context\":{\"expected\":\"1.0\",\"actual\":92233720368547758081234567890}}").unwrap();
            assert_eq!(outcome(&error), json!([expected]));
            // Output names both columns so pruning (REQ-0638) keeps them live
            // and strict window expansion still reports their unknown windows.
            let text="schema_version: '1.0'\ninput: {}\nkeys: []\noutput: {columns: [Z, A]}\ncolumns:\n  - {name: Z, type: int, derivation: {value: {row_number: {window: SECOND}}}}\n  - {name: A, type: int, derivation: {value: {row_number: {window: FIRST}}}}\n";
            let entry = source(text);
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let error = schema.prepare_standalone(entry).unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!()
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            let normalized = captured.document().unwrap();
            assert_eq!(
                normalized.nodes()[normalized.field(normalized.root(), "domain").unwrap()],
                N::Text("DEFAULT".into())
            );
            assert_ne!(normalized, &decode_yaml(text.as_bytes()).unwrap().document);
            assert_eq!(
                outcome(&error),
                json!([
                    {"phase":"validation","condition":"unknown_window","requirement":"REQ-1253","spec_paths":["columns.Z.derivation.row_number.window"],"context":{"window":"SECOND"}},
                    {"phase":"validation","condition":"unknown_window","requirement":"REQ-1253","spec_paths":["columns.A.derivation.row_number.window"],"context":{"window":"FIRST"}}
                ])
            );
        }

        #[test]
        fn final_model_findings_retain_no_pass_document_and_move_original_source() {
            let text = VALID
                .replace("domain: X", "domain: 7")
                .replace("{path: output.csv, columns: []}", "{}");
            let entry = source(&text);
            let pointer = entry.bytes.as_ptr();
            let error = schema().prepare_standalone(entry).unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!()
            };
            assert!(captured.document().is_none());
            assert_eq!(captured.source().bytes.as_ptr(), pointer);
            assert_eq!(
                outcome(&error),
                json!([
                    {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"reason":"Input should be a valid string"}},
                    {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0285","spec_paths":["output.path"],"context":{"reason":"Field required"}},
                    {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0285","spec_paths":["output.columns"],"context":{"reason":"Field required"}}
                ])
            );
        }

        #[test]
        fn physical_decode_and_source_boundary_precede_lifecycle() {
            let schema = schema();
            let text = VALID.to_owned() + "parent: parent.yaml\n";
            assert!(matches!(
                schema.prepare_standalone(source(&text)),
                Err(Error::InheritanceRequired)
            ));
            assert!(matches!(
                schema.prepare_standalone(source("parent: parent.yaml\n[unterminated")),
                Err(Error::Decode {
                    error: DecodeFailure::InvalidYaml { .. },
                    ..
                })
            ));
            let mut entry = source("[unterminated");
            entry
                .bytes
                .resize(yamaa_core::limits::Limits::SPEC_BYTES + 1, b'x');
            assert!(
                matches!(schema.prepare_standalone(entry), Err(Error::Decode {
        error: DecodeFailure::Limit(error), ..
    }) if error.resource == "spec_bytes" && error.limit == yamaa_core::limits::Limits::SPEC_BYTES
        && error.actual == Some(yamaa_core::limits::Limits::SPEC_BYTES + 1))
            );
        }

        #[test]
        fn standalone_dispatch_has_no_parent_effect_for_valid_or_invalid_entries() {
            assert!(schema()
                .prepare_document(source(VALID), "retained/spec.yaml".into(), &mut NoParents)
                .is_ok());
            let error = schema()
                .prepare_document(
                    source(&VALID.replace("'1.0'", "'99.0'")),
                    "retained/spec.yaml".into(),
                    &mut NoParents,
                )
                .unwrap_err();
            assert!(matches!(error, InheritanceError::Entry(Error::Findings(_))));
        }

        #[test]
        fn schema_default_checkpoint_precedes_authored_version_and_window_errors() {
            let invalid = SCHEMA.replace(
                "type: [str, int], default: DEFAULT",
                "type: str, default: 7",
            );
            let schema = CapturedSchema::admit(
                vec![Source {
                    identity: "schema.yaml".into(),
                    bytes: invalid.into_bytes(),
                }],
                0,
            )
            .unwrap();
            let entry = source("schema_version: '99.0'\nwindows: {BAD: broken}\n");
            let Error::Normalize(NormalizationError::Defaults(defaults)) =
                schema.prepare_standalone(entry).unwrap_err()
            else {
                panic!()
            };
            assert_eq!(defaults.len(), 1);
            assert_eq!(defaults[0].diagnostics.len(), 1);
            assert_eq!(
                defaults[0].diagnostics[0].definition().condition,
                "invalid_field_type"
            );
            // This remains an admitted-schema default failure, not a language finding
            // addressing the authored entry or a strict-window/model failure.
            let descriptor = &schema.structure().descriptors()[defaults[0].descriptor];
            assert_eq!(
                descriptor
                    .descriptor
                    .default_node()
                    .map(|node| &schema.structure().modules()[descriptor.module]
                        .document
                        .nodes()[node]),
                Some(&N::Integer("7".into()))
            );
        }
    }
    mod static_declarations {
        use yamaa_adapters::{
            shipped_schema, specification_check, specification_run::PreparedRun,
            specification_source::Source,
        };

        fn checked(extra: &str, derivation: &str) -> PreparedRun {
            let raw = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{DM: never-read.csv}}\noutput: {{path: result.csv, columns: [ID, V]}}\n{extra}\ncolumns:\n  - {{name: ID, type: str, label: ID, derivation: DM.ID}}\n  - name: V\n    type: float\n    label: V\n    derivation: {derivation}\n");
            let document = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(Source {
                    identity: "spec.yaml".into(),
                    bytes: raw.into_bytes(),
                })
                .unwrap();
            PreparedRun::prepare(document).unwrap()
        }

        #[test]
        fn original_parser_and_window_declaration_truth_is_available_without_source_authority() {
            for (extra, derivation, condition, path) in [
                (
                    "",
                    "{compute: {expr: '2 +'}}",
                    "invalid_expression",
                    "columns.V.derivation.compute.expr",
                ),
                (
                    "",
                    "{aggregate: 'SUM(DM.V) +'}",
                    "invalid_expression",
                    "columns.V.derivation.aggregate",
                ),
                (
                    "",
                    "{flag: 'DM.V != 1'}",
                    "invalid_expression",
                    "columns.V.derivation.flag.condition",
                ),
                (
                    "filter: 'DM.V != 1'",
                    "{literal: 1}",
                    "invalid_expression",
                    "filter",
                ),
                (
                    "",
                    "{source: {variable: DM.V, filter: 'DM.V != 1'}}",
                    "invalid_expression",
                    "columns.V.derivation.source.filter",
                ),
                (
                    "",
                    "{row_value: {source: V, offset: 0, window: {order_by: [ID]}}}",
                    "zero_offset",
                    "columns.V.derivation.row_value.offset",
                ),
                (
                    "",
                    "{rank: {}}",
                    "window_order_by_required",
                    "columns.V.derivation.rank.window",
                ),
                (
                    "",
                    "{case: [{when: 'DM.V != 1', then: {literal: 1}}, {otherwise: {literal: 2}}]}",
                    "invalid_expression",
                    "columns.V.derivation.case[0].when",
                ),
            ] {
                let rows = specification_check::issues(&checked(extra, derivation));
                assert_eq!(rows.len(), 1, "{derivation}: {rows:?}");
                assert_eq!(rows[0].condition(), condition, "{derivation}");
                assert_eq!(rows[0].spec_paths, [path], "{derivation}");
                assert!(rows[0].requirement().is_some());
            }
        }

        #[test]
        fn arithmetic_predicates_are_admitted_at_every_predicate_site() {
            // The three predicate sites from #2204: a case `when`, a root `filter`
            // and an `assert.require`. Arithmetic over numeric operands belongs to
            // the one expression language, so none of them is refused.
            let head = "schema_version: '1.0'\ndomain: ADVS\nkeys: [USUBJID]\ninput: {VS: {path: never-read.csv, types: {SBPSUP: float, SBPSTD: float}}}\noutput: {path: advs.csv, columns: [USUBJID, OHYPO]}\ncolumns:\n  - {name: USUBJID, type: str, derivation: VS.USUBJID}\n  - {name: SBPSUP, type: float, derivation: VS.SBPSUP}\n  - {name: SBPSTD, type: float, derivation: VS.SBPSTD}\n";
            let rows = |extra: &str| {
                let document = shipped_schema::capture()
                    .unwrap()
                    .prepare_standalone(Source {
                        identity: "spec.yaml".into(),
                        bytes: format!("{head}{extra}").into_bytes(),
                    })
                    .unwrap();
                specification_check::issues(&PreparedRun::prepare(document).unwrap())
            };
            for (extra, path) in [
                (
                    "  - name: OHYPO\n    type: str\n    derivation:\n      case:\n        - {when: 'SBPSTD - SBPSUP <= -20', then: {literal: 'Y'}}\n        - {otherwise: {literal: 'N'}}\n",
                    "columns.OHYPO.derivation.case[0].when",
                ),
                (
                    "  - {name: OHYPO, type: str, derivation: {literal: 'N'}}\nfilter: 'VS.SBPSTD - VS.SBPSUP < 0'\n",
                    "filter",
                ),
                (
                    "  - {name: OHYPO, type: str, derivation: {literal: 'N'}}\nverifications:\n  - {assert: {when: \"OHYPO = 'Y'\", require: 'SBPSTD - SBPSUP <= -20'}}\n",
                    "verifications[0].assert.require",
                ),
            ] {
                let rows = rows(extra);
                assert!(rows.is_empty(), "{path}: {rows:?}");
            }
        }

        #[test]
        fn clean_declarations_still_defer_actual_dataset_fields_and_values_to_build() {
            assert!(
                specification_check::issues(&checked("", "{compute: {expr: 'UNKNOWN + 1'}}"))
                    .is_empty()
            );
            assert!(
                specification_check::issues(&checked("", "{compute: {expr: '1 / 0'}}")).is_empty()
            );
        }

        #[test]
        fn flag_missing_value_findings_preserve_original_scalar_kind_and_nul_bytes() {
            // The retired repository validator's flag assertion used these exact
            // false values. Observe native findings directly instead of replaying
            // them through the former JSON-line semantic fixture executable.
            for false_value in [
                yamaa_adapters::json!("N"),
                yamaa_adapters::json!(i64::MIN),
                yamaa_adapters::json!(i64::MAX),
                yamaa_adapters::json!(false),
                yamaa_adapters::json!(true),
                yamaa_adapters::json!("N\0U"),
            ] {
                let text = format!(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{DM: never-read.csv}}\noutput: {{path: result.csv, columns: [ID, B, F]}}\ncolumns:\n  - {{name: ID, type: str, derivation: DM.ID}}\n  - {{name: B, type: int, derivation: {{literal: 2}}}}\n  - {{name: F, type: str, derivation: {{flag: {{condition: 'B > 1', false_value: {}}}}}}}\n"
        , yamaa_adapters::json_wire::encode(&false_value));
                let document = shipped_schema::capture()
                    .unwrap()
                    .prepare_standalone(Source {
                        identity: "spec.yaml".into(),
                        bytes: text.into_bytes(),
                    })
                    .unwrap();
                let run = PreparedRun::prepare(document).unwrap();
                let rows = specification_check::issues(&run);
                assert_eq!(
                    rows.len(),
                    1,
                    "{}: {rows:?}",
                    yamaa_adapters::json_wire::encode(&false_value)
                );
                assert_eq!(rows[0].condition(), "missing_value_required");
                assert_eq!(rows[0].requirement(), Some("REQ-1258"));
                assert_eq!(rows[0].spec_paths, ["columns.F.derivation.flag.missing"]);
                let context = yamaa_core::json::Value::Object(rows[0].context.clone());
                assert_eq!(context, yamaa_adapters::json!({"false_value": false_value}));
            }
        }
    }
    mod string_template_syntax_fixture {
        use crate::support::string_template_syntax as fixture;
        use yamaa_adapters::json;

        #[test]
        fn typed_occurrences_preserve_escaped_text_spans_and_repeated_bindings() {
            let observed = fixture::observe("é{{{A}}}:{A}/{B}");
            assert_eq!(observed["outcome"]["identifiers"], json!(["A", "B"]));
            assert_eq!(
                observed["outcome"]["parts"],
                json!([
                    {"kind":"text","value":"é{","span":[0,3]},
                    {"kind":"placeholder","name":"A","span":[4,5]},
                    {"kind":"text","value":"}:","span":[6,9]},
                    {"kind":"placeholder","name":"A","span":[10,11]},
                    {"kind":"text","value":"/","span":[12,13]},
                    {"kind":"placeholder","name":"B","span":[14,15]}
                ])
            );
            assert_eq!(fixture::observe("")["outcome"]["parts"], json!([]));
        }

        #[test]
        fn typed_refusals_preserve_grammar_reason_placeholder_and_unicode_position() {
            let observed = fixture::observe("é{A + B}");
            assert_eq!(
                observed["outcome"],
                json!({
                    "status":"invalid", "condition":"invalid_string_template",
                    "requirement":"REQ-0461",
                    "context":{"reason":"invalid_placeholder","placeholder":"A + B"},
                    "position":{"byte":2,"character":1},
                    "span":{"byte_start":2,"byte_end":9,"character_start":1,"character_end":8}
                })
            );
        }
    }
    mod structural_documents {
        use crate::support::project_schemas::{
            candidate_environment_modules as environment_modules,
            candidate_project_modules as project_modules,
        };
        use std::sync::Arc;
        use yamaa_adapters::{
            specification_source::{CapturedSchema, Error, Source},
            yaml_decode::decode_yaml,
        };
        use yamaa_core::schema::{DocumentNode as N, SchemaContext as C, SchemaDiagnostic};

        #[derive(Debug, PartialEq, Eq)]
        enum LogicalTree {
            Null,
            Boolean(bool),
            Integer(String),
            Float(u64),
            Text(String),
            Sequence(Vec<Self>),
            Mapping(Vec<(Self, Self)>),
        }
        fn logical_tree(document: &yamaa_core::schema::Document, node: usize) -> LogicalTree {
            match &document.nodes()[node] {
                N::Null => LogicalTree::Null,
                N::Boolean(value) => LogicalTree::Boolean(*value),
                N::Integer(value) => LogicalTree::Integer(value.clone()),
                N::Float(value) => LogicalTree::Float(value.to_bits()),
                N::Text(value) => LogicalTree::Text(value.clone()),
                N::Sequence(items) => LogicalTree::Sequence(
                    items.iter().map(|&id| logical_tree(document, id)).collect(),
                ),
                N::Mapping(items) => LogicalTree::Mapping(
                    items
                        .iter()
                        .map(|&(key, value)| {
                            (logical_tree(document, key), logical_tree(document, value))
                        })
                        .collect(),
                ),
            }
        }

        fn source(name: &str, content: &str) -> Source {
            Source {
                identity: name.into(),
                bytes: content.as_bytes().to_vec(),
            }
        }
        fn environment_schema() -> Arc<CapturedSchema> {
            CapturedSchema::admit_root(environment_modules(), 0, "environment_class").unwrap()
        }
        fn project_schema() -> Arc<CapturedSchema> {
            CapturedSchema::admit_root(project_modules(), 0, "project_class").unwrap()
        }
        fn finding(
            path: &str,
            condition: &'static str,
            field: &str,
            class: &str,
        ) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: path.into(),
                code: match condition {
                    "unknown_field" => yamaa_core::diagnostic::ConditionCode::SchemaUnknownField,
                    _ => panic!("unexpected test cause"),
                },
                context: vec![
                    ("field", C::Text(field.into())),
                    ("class", C::Text(class.into())),
                ],
            }
        }
        fn captured_findings(schema: &Arc<CapturedSchema>, text: &str) -> Vec<SchemaDiagnostic> {
            let Err(Error::Findings(result)) =
                schema.prepare_structural(source("spec/root.yaml", text))
            else {
                panic!("expected complete structural findings");
            };
            assert_eq!(result.source().bytes, text.as_bytes());
            assert!(result.document().is_some());
            result.findings().to_vec()
        }

        #[test]
        fn structural_environment_owns_schema_source_raw_tree_and_explicit_defaults() {
            let mut original = environment_modules();
            let retained = original.iter().map(|s| s.bytes.clone()).collect::<Vec<_>>();
            let schema =
                CapturedSchema::admit_root(original.clone(), 0, "environment_class").unwrap();
            for source in &mut original {
                source.bytes.clear();
                source.identity.clear();
            }
            for (module, bytes) in schema.sources().iter().zip(retained) {
                assert_eq!(module.bytes, bytes);
            }
            let text="schema_version: '1.0'\nlock: ../uv.lock\nfunctions:\n  bmi:\n    function: projectbmi.bmi\n    description: Body mass index.\n    params: [{name: weight, type: float}]\n    returns: float\n    tests:\n      - {id: normal, covers: [normal], args: {weight: 70.0}, result: 70.0}\n";
            let captured = schema
                .prepare_structural(source("spec/environment.yaml", text))
                .unwrap();
            assert!(Arc::ptr_eq(captured.schema(), &schema));
            assert_eq!(captured.source().identity, "spec/environment.yaml");
            assert_eq!(captured.source().bytes, text.as_bytes());
            let raw = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(captured.raw().document, raw.document);
            assert_eq!(captured.raw().locations, raw.locations);
            let expected="schema_version: '1.0'\nlock: ../uv.lock\nfunctions:\n  bmi:\n    function: projectbmi.bmi\n    description: Body mass index.\n    params: [{name: weight, type: float, required: true, accepts_missing: false}]\n    returns: float\n    may_return_missing: false\n    comparison_decimals: 4\n    tests:\n      - {id: normal, covers: [normal], args: {weight: 70.0}, result: 70.0}\n";
            let expected = decode_yaml(expected.as_bytes()).unwrap().document;
            let actual = &captured.normalized().document;
            assert_eq!(
                logical_tree(actual, actual.root()),
                logical_tree(&expected, expected.root())
            );
            assert_eq!(captured.normalized().origins.len(), expected.nodes().len());
        }

        #[test]
        fn structural_project_owns_schema_source_raw_tree_and_explicit_defaults() {
            let mut original = project_modules();
            let retained = original.iter().map(|s| s.bytes.clone()).collect::<Vec<_>>();
            let schema = CapturedSchema::admit_root(original.clone(), 0, "project_class").unwrap();
            for source in &mut original {
                source.bytes.clear();
                source.identity.clear();
            }
            for (module, bytes) in schema.sources().iter().zip(retained) {
                assert_eq!(module.bytes, bytes);
            }
            let text="schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\ncodelists:\n  - ../ct.yaml\n  - codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\nadam: {specs: [adam/adsl.yaml]}\n";
            let captured = schema
                .prepare_structural(source("spec/project.yaml", text))
                .unwrap();
            assert!(Arc::ptr_eq(captured.schema(), &schema));
            assert_eq!(captured.source().identity, "spec/project.yaml");
            assert_eq!(captured.source().bytes, text.as_bytes());
            let raw = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(captured.raw().document, raw.document);
            assert_eq!(captured.raw().locations, raw.locations);
            let expected="schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\ncodelists:\n  - ../ct.yaml\n  - codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}]}]\nadam: {specs: [adam/adsl.yaml], context: Submission, xml_lang: en}\n";
            let expected = decode_yaml(expected.as_bytes()).unwrap().document;
            let actual = &captured.normalized().document;
            assert_eq!(
                logical_tree(actual, actual.root()),
                logical_tree(&expected, expected.root())
            );
            assert_eq!(captured.normalized().origins.len(), expected.nodes().len());
        }

        #[test]
        fn environment_and_project_roots_do_not_create_a_specification_or_require_domain_data() {
            for schema in [environment_schema(), project_schema()] {
                let captured = schema
                    .prepare_structural(source("root.yaml", "schema_version: '1.0'\n"))
                    .unwrap();
                let document = &captured.normalized().document;
                assert_eq!(
                    document.nodes()[document.root()].kind(),
                    yamaa_core::schema::DocumentKind::Mapping
                );
                assert_eq!(document.field(document.root(), "domain"), None);
                assert_eq!(document.field(document.root(), "input"), None);
                assert_eq!(document.field(document.root(), "output"), None);
            }
        }

        #[test]
        fn environment_parent_language_and_retired_runtime_fields_are_complete_schema_findings() {
            let schema = environment_schema();
            // The project key states the language; the environment file declares none.
            let findings = captured_findings(
        &schema,
        "schema_version: '1.0'\nparent: base.yaml\nlanguage: python\nruntime: {language: python}\nversion: old\n",
    );
            assert_eq!(
                findings[0],
                finding("parent", "unknown_field", "parent", "environment_class")
            );
            assert_eq!(findings.len(), 4);
            for (finding, field) in findings[1..].iter().zip(["language", "runtime", "version"]) {
                assert_eq!(finding.path, field);
                assert_eq!(finding.definition().condition, "unknown_field");
                assert_eq!(finding.definition().requirement, Some("REQ-0285"));
                assert_eq!(
                    finding.context,
                    vec![
                        ("field", C::Text(field.into())),
                        ("class", C::Text("environment_class".into())),
                    ]
                );
            }
        }

        #[test]
        fn project_and_environment_roots_refuse_each_others_fields() {
            let environment = captured_findings(
                &environment_schema(),
                "schema_version: '1.0'\nlock: ../renv.lock\ncodelists: [../ct.yaml]\nstudy: {id: STUDY01, name: STUDY01, description: Study, protocol_name: Study-01}\nadam: {specs: [adam/adsl.yaml]}\n",
            );
            assert_eq!(
                environment,
                ["codelists", "study", "adam"].map(|field| finding(
                    field,
                    "unknown_field",
                    field,
                    "environment_class"
                ))
            );
            let project = captured_findings(
                &project_schema(),
                "schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {bmi: functions/bmi.yaml}\n",
            );
            assert_eq!(
                project,
                ["language", "lock", "functions"].map(|field| finding(
                    field,
                    "unknown_field",
                    field,
                    "project_class"
                ))
            );
        }

        #[test]
        fn version_priority_precedes_every_other_root_finding() {
            for schema in [environment_schema(), project_schema()] {
                assert_eq!(
                    captured_findings(
                        &schema,
                        "schema_version: '99.0'\nparent: base.yaml\nruntime: {}\n"
                    ),
                    vec![SchemaDiagnostic {
                        path: "schema_version".into(),
                        code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                        context: vec![
                            ("expected", C::Text("1.0".into())),
                            ("actual", C::InputValue(1))
                        ],
                    }]
                );
            }
        }

        #[test]
        fn environment_and_project_roots_keep_closed_independent_shapes() {
            let text =
                "schema_version: '1.0'\nlock: ../renv.lock\nfunctions: {bmi: functions/bmi.yaml}\n";
            let result = environment_schema()
                .prepare_structural(source("spec/environment.yaml", text))
                .unwrap();
            let doc = &result.normalized().document;
            assert_eq!(
                doc.nodes()[doc.field(doc.root(), "lock").unwrap()],
                N::Text("../renv.lock".into())
            );
            assert_eq!(result.source().bytes, text.as_bytes());
            let project = project_schema();
            let text="schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\ncodelists: [../ct.yaml]\nstudy: {id: STUDY01, name: STUDY01, description: Study, protocol_name: Study-01}\nadam:\n  standard: {name: ADaMIG, version: '1.3', status: Final}\n  specs: [adam/adsl.yaml]\n  define: ../data/adam/define.xml\n  documents: [{id: adrg, kind: supplemental, link: adrg.pdf, title: Review guide}]\n  datasets: json\nsdtm: {specs: [sdtm/dm.yaml]}\n";
            let result = project
                .prepare_structural(source("spec/project.yaml", text))
                .unwrap();
            let doc = &result.normalized().document;
            let environments = doc.field(doc.root(), "environments").unwrap();
            assert_eq!(
                doc.nodes()[doc.field(environments, "r").unwrap()],
                N::Text("r/environment.yaml".into())
            );
            let adam = doc.field(doc.root(), "adam").unwrap();
            assert_eq!(
                doc.nodes()[doc.field(adam, "datasets").unwrap()],
                N::Text("json".into())
            );
            assert_eq!(
                doc.nodes()[doc.field(adam, "context").unwrap()],
                N::Text("Submission".into())
            );
            assert_eq!(
                doc.nodes()[doc.field(adam, "xml_lang").unwrap()],
                N::Text("en".into())
            );
            // A section needs only specs; the dataset format has no default.
            let sdtm = doc.field(doc.root(), "sdtm").unwrap();
            for absent in ["standard", "define", "datasets"] {
                assert_eq!(doc.field(sdtm, absent), None);
            }
            assert_eq!(
                doc.nodes()[doc.field(sdtm, "context").unwrap()],
                N::Text("Submission".into())
            );
            assert_eq!(result.source().bytes, text.as_bytes());
            // The retired dataset flag is unknown; the dataset format is closed.
            assert_eq!(
                captured_findings(
                    &project,
                    "schema_version: '1.0'\nadam: {specs: [adam/adsl.yaml], dataset_json: true}\n"
                ),
                [finding(
                    "adam.dataset_json",
                    "unknown_field",
                    "dataset_json",
                    "submission_section_class"
                )]
            );
            assert_eq!(
                captured_findings(
                    &project,
                    "schema_version: '1.0'\nadam: {specs: [adam/adsl.yaml], datasets: xpt}\n"
                )
                .iter()
                .map(|finding| (finding.path.as_str(), finding.definition().condition))
                .collect::<Vec<_>>(),
                [("adam.datasets", "value_not_permitted")]
            );
        }

        #[test]
        fn structural_capture_keeps_invalid_yaml_and_fixed_size_refusals_distinct() {
            let schema = environment_schema();
            assert!(matches!(
                schema.prepare_structural(source("environment.yaml", "schema_version: [unclosed")),
                Err(Error::Decode {
                    error: yamaa_adapters::yaml_decode::DecodeFailure::InvalidYaml { .. },
                    ..
                })
            ));
            let bytes = "x".repeat(yamaa_core::limits::Limits::SPEC_BYTES + 1);
            assert!(
                matches!(schema.prepare_structural(source("environment.yaml", &bytes)),
        Err(Error::Decode { error: yamaa_adapters::yaml_decode::DecodeFailure::Limit(error), .. })
        if error.resource == "spec_bytes" && error.limit == yamaa_core::limits::Limits::SPEC_BYTES
            && error.actual == Some(bytes.len()))
            );
        }

        #[test]
        fn named_root_selection_and_absent_classes_fail_closed() {
            for (modules, other) in [
                (environment_modules(), "project_class"),
                (project_modules(), "environment_class"),
            ] {
                for class in ["missing_class", other] {
                    assert!(matches!(
                        CapturedSchema::admit_root(modules.clone(), 0, class),
                        Err(Error::Bundle(_))
                    ));
                }
                assert!(matches!(
                    CapturedSchema::admit(modules, 0),
                    Err(Error::Bundle(_))
                ));
            }
        }

        #[test]
        fn standalone_function_and_codelist_classes_keep_schema_version_in_schema_only() {
            for (modules, class, text, expected) in [
        (environment_modules(), "function_definition_class", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: []\n", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\nmay_return_missing: false\ncomparison_decimals: 4\ntests: []\n"),
        (project_modules(), "codelist_source_class", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\n", "codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}]}]\n"),
    ] {
        let schema=CapturedSchema::admit_root(modules, 0, class).unwrap();
        let captured=schema.prepare_class(source("independent.yaml", text)).unwrap();
        assert_eq!(captured.source().bytes, text.as_bytes());
        let actual=&captured.normalized().document;
        let expected=decode_yaml(expected.as_bytes()).unwrap().document;
        assert_eq!(logical_tree(actual, actual.root()), logical_tree(&expected, expected.root()));
        assert_eq!(actual.field(actual.root(), "schema_version"), None);
        assert!(captured.normalized().origins.iter().any(|origin| matches!(origin.source, yamaa_core::schema::SchemaSource::Default {..})));
    }
        }

        #[test]
        fn named_classes_reuse_their_root_capture_without_fabricated_versions() {
            for (schema, root, class, text, expected) in [
        (environment_schema(), "environment_class", "function_definition_class", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: []\n", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\nmay_return_missing: false\ncomparison_decimals: 4\ntests: []\n"),
        (project_schema(), "project_class", "codelist_source_class", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\n", "codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}]}]\n"),
    ] {
        let result = schema.prepare_named_class(source("independent.yaml", text), class).unwrap();
        assert!(Arc::ptr_eq(result.schema(), &schema));
        assert_eq!(result.class_name(), class);
        assert_eq!(result.source().bytes, text.as_bytes());
        assert_eq!(result.schema().structure().root_class().name, root);
        let actual = &result.normalized().document;
        let expected = decode_yaml(expected.as_bytes()).unwrap().document;
        assert_eq!(logical_tree(actual, actual.root()), logical_tree(&expected, expected.root()));
        assert!(actual.field(actual.root(), "schema_version").is_none());
    }
        }

        #[test]
        fn named_class_selection_is_closed_and_cannot_bypass_version_admission() {
            let malformed = || source("independent.yaml", "[unclosed");
            for (schema, root, own, other) in [
                (
                    environment_schema(),
                    "environment_class",
                    "function_definition_class",
                    "codelist_source_class",
                ),
                (
                    project_schema(),
                    "project_class",
                    "codelist_source_class",
                    "function_definition_class",
                ),
            ] {
                for class in ["missing_class", "str", "", other] {
                    assert!(
                        matches!(schema.prepare_named_class(malformed(), class), Err(Error::UnknownClass(name)) if name == class)
                    );
                }
                assert!(matches!(
                    schema.prepare_named_class(malformed(), root),
                    Err(Error::VersionedRootRequired)
                ));
                assert!(matches!(
                    schema.prepare_named_class(malformed(), own),
                    Err(Error::Decode { .. })
                ));
            }
        }

        #[test]
        fn class_admission_cannot_bypass_versioned_roots_or_open_function_fields() {
            assert!(matches!(
                environment_schema()
                    .prepare_class(source("environment.yaml", "lock: ../uv.lock\n")),
                Err(Error::VersionedRootRequired)
            ));
            assert!(matches!(
                project_schema().prepare_class(source(
                    "project.yaml",
                    "environments: {python: environment.yaml}\n"
                )),
                Err(Error::VersionedRootRequired)
            ));
            let function =
                CapturedSchema::admit_root(environment_modules(), 0, "function_definition_class")
                    .unwrap();
            let text="schema_version: '1.0'\nfunction: program.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: []\ncontract_version: '1'\nbinding: {call: program.constant}\n";
            let Err(Error::Findings(result)) =
                function.prepare_class(source("function.yaml", text))
            else {
                panic!("retired fields must fail");
            };
            assert_eq!(result.source().bytes, text.as_bytes());
            assert_eq!(
                result
                    .findings()
                    .iter()
                    .map(|f| f.path.as_str())
                    .collect::<Vec<_>>(),
                vec![
                    "<normalization>.schema_version",
                    "<normalization>.contract_version",
                    "<normalization>.binding"
                ]
            );
        }

        #[test]
        fn captured_function_decodes_directly_to_an_admitted_versionless_core_plan() {
            use yamaa_core::{
                project_function::{Case, Definition, Function, Language, Parameter},
                project_function_document,
                temporal::{Date, Precision},
                value::{ColumnType, Value, ValueType},
            };
            let schema =
                CapturedSchema::admit_root(environment_modules(), 0, "function_definition_class")
                    .unwrap();
            let text="function: project.identity\ndescription: Return the date.\nparams: [{name: x, type: date}]\nreturns: date\ntests:\n  - {id: leap-day, covers: [normal, boundary], args: {x: {date: '2024-02-29'}}, result: {date: '2024-02-29'}}\n  - {id: missing-x, covers: [short-circuit-missing:x], args: {x: null}, result: null}\n";
            let captured = schema
                .prepare_class(source("functions/identity.yaml", text))
                .unwrap();
            let document = &captured.normalized().document;
            let decoded =
                project_function_document::decode(document, document.root(), "identity").unwrap();
            let date = Value::Date(Date::new(2024, 2, 29, Precision::Day).unwrap());
            assert_eq!(
                decoded,
                Definition {
                    name: "identity".into(),
                    function: "project.identity".into(),
                    description: "Return the date.".into(),
                    params: vec![Parameter {
                        name: "x".into(),
                        kind: ValueType::Date,
                        required: true,
                        default: None,
                        accepts_missing: false
                    }],
                    returns: ColumnType::Date,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![
                        Case {
                            id: "leap-day".into(),
                            covers: vec!["normal".into(), "boundary".into()],
                            args: vec![("x".into(), date.clone())],
                            result: date
                        },
                        Case {
                            id: "missing-x".into(),
                            covers: vec!["short-circuit-missing:x".into()],
                            args: vec![("x".into(), Value::Missing)],
                            result: Value::Missing
                        },
                    ],
                }
            );
            let function = Function::admit(Language::Python, decoded).unwrap();
            let plan = function.invocation_plan().unwrap();
            assert_eq!(plan.identity().name, "identity");
            assert_eq!(plan.identity().call, "project.identity");
            assert_eq!(plan.signature().parameters()[0].host_name, "x");
            assert_eq!(captured.source().bytes, text.as_bytes());
        }

        #[test]
        fn scalar_decoder_keeps_exact_int_float_boolean_nul_and_temporal_defaults() {
            use yamaa_core::{
                project_function_document,
                temporal::{Date, DateTime, Precision},
                value::Value,
            };
            let schema =
                CapturedSchema::admit_root(environment_modules(), 0, "function_definition_class")
                    .unwrap();
            let text="function: project.identity\ndescription: Exact defaults.\nparams:\n  - {name: integer, type: int, required: false, default: -9223372036854775808}\n  - {name: float, type: float, required: false, default: -0.0}\n  - {name: boolean, type: bool, required: false, default: false}\n  - {name: text, type: str, required: false, default: \"a\\0b\\u96ea\"}\n  - {name: date, type: date, required: false, default: {date: '2024-02-29'}}\n  - {name: datetime, type: datetime, required: false, default: {datetime: '2024-02-29T08:30'}}\n  - {name: missing, type: str, required: false, default: null, accepts_missing: true}\nreturns: str\ntests: []\n";
            let captured = schema.prepare_class(source("function.yaml", text)).unwrap();
            let doc = &captured.normalized().document;
            let decoded = project_function_document::decode(doc, doc.root(), "defaults").unwrap();
            let date = Date::new(2024, 2, 29, Precision::Day).unwrap();
            assert_eq!(
                decoded
                    .params
                    .iter()
                    .map(|p| p.default.clone())
                    .collect::<Vec<_>>(),
                vec![
                    Some(Value::Int(i64::MIN)),
                    Some(Value::float(-0.0)),
                    Some(Value::Bool(false)),
                    Some(Value::Str("a\0b\u{96ea}".into())),
                    Some(Value::Date(date)),
                    Some(Value::DateTime(
                        DateTime::new(date, 8, 30, 0, Precision::Second).unwrap()
                    )),
                    Some(Value::Missing),
                ]
            );
            let Some(Value::Float(float)) = decoded.params[1].default else {
                panic!("float type must remain exact");
            };
            assert_eq!(float.get().to_bits(), 0x8000_0000_0000_0000);
        }

        #[test]
        fn every_invalid_scalar_retains_its_node_without_exposing_a_partial_model() {
            use yamaa_core::project_function_document::{self, Kind};
            let schema =
                CapturedSchema::admit_root(environment_modules(), 0, "function_definition_class")
                    .unwrap();
            let text="function: project.identity\ndescription: Invalid leaves.\nparams: [{name: x, type: int, required: false, default: 9223372036854775808}]\nreturns: date\ntests:\n  - {id: invalid, covers: [normal, boundary], args: {x: -9223372036854775809}, result: {date: '2023-02-29'}}\n  - {id: invalid-time, covers: [boundary], args: {x: 1}, result: {datetime: '2024-01-01T08:30Z'}}\n";
            let captured = schema.prepare_class(source("function.yaml", text)).unwrap();
            let doc = &captured.normalized().document;
            let errors = project_function_document::decode(doc, doc.root(), "invalid").unwrap_err();
            assert_eq!(
                errors
                    .iter()
                    .map(|f| (f.path.as_str(), f.kind))
                    .collect::<Vec<_>>(),
                vec![
                    ("params[0].default", Kind::IntegerRange),
                    ("tests[0].args.x", Kind::IntegerRange),
                    ("tests[0].result", Kind::InvalidDate),
                    ("tests[1].result", Kind::InvalidDateTime),
                ]
            );
            assert_eq!(
                doc.nodes()[errors[0].node],
                N::Integer("9223372036854775808".into())
            );
            assert_eq!(
                doc.nodes()[errors[1].node],
                N::Integer("-9223372036854775809".into())
            );
            assert_eq!(captured.source().bytes, text.as_bytes());
        }

        #[test]
        fn codelist_source_decodes_directly_with_source_standard_and_unread_external_href() {
            use yamaa_core::{
                project_terminology::{
                    Catalogue, Codelist, DataType, External, Item, Source as TerminologySource,
                    Standard,
                },
                project_terminology_document,
                value::Value,
            };
            let schema =
                CapturedSchema::admit_root(project_modules(), 0, "codelist_source_class").unwrap();
            let text="standard: {name: CDISC/NCI, publishing_set: SDTM, version: '2023-12-15'}\ncodelists:\n  - {id: SEX, name: Sex, items: [{value: F, decode: Female, rank: 1}, {value: M, decode: Male, rank: 2}]}\n  - {id: DICT, name: Dictionary, external: {dictionary: MedDRA, version: '27.0', href: '../unread/dictionary'}}\n";
            let captured = schema.prepare_class(source("ct.yaml", text)).unwrap();
            let doc = &captured.normalized().document;
            let source = project_terminology_document::decode(doc, doc.root()).unwrap();
            let expected = TerminologySource {
                standard: Some(Standard {
                    name: "CDISC/NCI".into(),
                    publishing_set: "SDTM".into(),
                    version: "2023-12-15".into(),
                }),
                codelists: vec![
                    Codelist {
                        id: "SEX".into(),
                        name: "Sex".into(),
                        data_type: DataType::Text,
                        extensible: false,
                        alias: None,
                        format_name: None,
                        items: Some(vec![
                            Item {
                                value: Value::Str("F".into()),
                                decode: Some("Female".into()),
                                rank: Some(1),
                                alias: None,
                                extended: false,
                            },
                            Item {
                                value: Value::Str("M".into()),
                                decode: Some("Male".into()),
                                rank: Some(2),
                                alias: None,
                                extended: false,
                            },
                        ]),
                        external: None,
                    },
                    Codelist {
                        id: "DICT".into(),
                        name: "Dictionary".into(),
                        data_type: DataType::Text,
                        extensible: false,
                        alias: None,
                        format_name: None,
                        items: None,
                        external: Some(External {
                            dictionary: "MedDRA".into(),
                            version: "27.0".into(),
                            href: Some("../unread/dictionary".into()),
                        }),
                    },
                ],
            };
            assert_eq!(source, expected);
            assert!(Catalogue::admit(vec![source]).is_ok());
            assert_eq!(captured.source().bytes, text.as_bytes());
        }

        #[test]
        fn source_decoder_retains_every_wide_value_and_rank_failure() {
            use yamaa_core::{project_function_document::Kind, project_terminology_document};
            let schema =
                CapturedSchema::admit_root(project_modules(), 0, "codelist_source_class").unwrap();
            let text="codelists:\n  - {id: BIG, name: Big, data_type: integer, items: [{value: 9223372036854775808, rank: -9223372036854775809}, {value: -9223372036854775809}]}\n";
            let captured = schema.prepare_class(source("ct.yaml", text)).unwrap();
            let doc = &captured.normalized().document;
            let errors = project_terminology_document::decode(doc, doc.root()).unwrap_err();
            assert_eq!(
                errors
                    .iter()
                    .map(|finding| (finding.path.as_str(), finding.kind))
                    .collect::<Vec<_>>(),
                vec![
                    ("codelists[0].items[0].value", Kind::IntegerRange),
                    ("codelists[0].items[0].rank", Kind::IntegerRange),
                    ("codelists[0].items[1].value", Kind::IntegerRange),
                ]
            );
        }

        #[test]
        fn environment_decoder_keeps_the_lock_and_inline_or_path_function_order() {
            use yamaa_core::project_environment_document::{self, Declaration};
            let schema = environment_schema();
            let text="schema_version: '1.0'\nlock: ../uv.lock\nfunctions:\n  first: functions/first.yaml\n  second: {function: project.constant, description: Constant, params: [], returns: int, tests: []}\n";
            let captured = schema
                .prepare_structural(source("environment.yaml", text))
                .unwrap();
            let document = &captured.normalized().document;
            let result = project_environment_document::decode(document, document.root()).unwrap();
            assert_eq!(result.lock.as_deref(), Some("../uv.lock"));
            let functions = result.functions.unwrap();
            assert_eq!(
                functions
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["first", "second"]
            );
            assert_eq!(
                functions[0].declaration,
                Declaration::Path("functions/first.yaml".into())
            );
            let Declaration::Inline(def) = &functions[1].declaration else {
                panic!("inline definition")
            };
            assert_eq!(def.name, "second");
            assert_eq!(def.function, "project.constant");
            assert_eq!(def.comparison_decimals, 4);
            assert_eq!(captured.source().bytes, text.as_bytes());
        }

        #[test]
        fn project_decoder_keeps_environment_links_codelist_order_and_unread_submission_metadata() {
            use yamaa_core::{
                project_document, project_environment_document::Declaration,
                project_function::Language,
            };
            let schema = project_schema();
            let text="schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\ncodelists:\n  - ../terminology.yaml\n  - standard: {name: CDISC/NCI, publishing_set: SDTM, version: '2024-03-29'}\n    codelists: [{id: EXTERNAL, name: External, external: {dictionary: Dict, version: '1', href: '../not-read.xml'}}]\nstudy: {id: Study01, name: Study, description: Study, protocol_name: Protocol}\nadam: {standard: {name: ADaMIG, version: '1.3', status: Final}, specs: [adam/adsl.yaml], define: ../data/adam/define.xml, documents: [{id: adrg, kind: supplemental, link: 'docs/adrg.pdf', title: Guide}]}\n";
            let captured = schema
                .prepare_structural(source("project.yaml", text))
                .unwrap();
            let document = &captured.normalized().document;
            let result = project_document::decode(document, document.root()).unwrap();
            assert_eq!(
                result.environments.python.as_deref(),
                Some("python/environment.yaml")
            );
            assert_eq!(result.environments.r.as_deref(), Some("r/environment.yaml"));
            assert_eq!(
                result.environments.get(Language::Python),
                Some("python/environment.yaml")
            );
            assert_eq!(
                result.environments.get(Language::R),
                Some("r/environment.yaml")
            );
            assert_eq!(result.codelists.len(), 2);
            assert_eq!(
                result.codelists[0].declaration,
                Declaration::Path("../terminology.yaml".into())
            );
            let Declaration::Inline(ct) = &result.codelists[1].declaration else {
                panic!("inline source")
            };
            assert_eq!(ct.standard.as_ref().unwrap().version, "2024-03-29");
            assert_eq!(
                ct.codelists[0].external.as_ref().unwrap().href.as_deref(),
                Some("../not-read.xml")
            );
            let N::Sequence(entries) =
                &document.nodes()[document.field(document.root(), "codelists").unwrap()]
            else {
                panic!("codelist sequence")
            };
            assert_eq!(
                result
                    .codelists
                    .iter()
                    .map(|codelist| codelist.node)
                    .collect::<Vec<_>>(),
                *entries
            );
            // Study and section metadata stay in the document for submission composition.
            assert!(document.field(document.root(), "study").is_some());
            assert!(document.field(document.root(), "adam").is_some());
            assert_eq!(captured.source().bytes, text.as_bytes());
        }

        #[test]
        fn environment_and_project_decoders_preserve_absent_versus_present_empty_sources() {
            use yamaa_core::{project_document, project_environment_document};
            let schema = environment_schema();
            for (text, present) in [
                ("schema_version: '1.0'\n", false),
                ("schema_version: '1.0'\nfunctions: {}\n", true),
            ] {
                let captured = schema
                    .prepare_structural(source("environment.yaml", text))
                    .unwrap();
                let document = &captured.normalized().document;
                let result =
                    project_environment_document::decode(document, document.root()).unwrap();
                assert_eq!(result.functions.is_some(), present);
                assert!(result.functions.into_iter().flatten().next().is_none());
                assert!(result.lock.is_none());
            }
            let schema = project_schema();
            for text in [
                "schema_version: '1.0'\n",
                "schema_version: '1.0'\nenvironments: {}\ncodelists: []\n",
            ] {
                let captured = schema
                    .prepare_structural(source("project.yaml", text))
                    .unwrap();
                let document = &captured.normalized().document;
                let result = project_document::decode(document, document.root()).unwrap();
                assert_eq!(result.environments, Default::default());
                assert!(result.codelists.is_empty());
            }
        }

        #[test]
        fn environment_and_project_decoders_collect_every_independent_inline_scalar_failure() {
            use yamaa_core::json::Value as JsonValue;
            use yamaa_core::{
                project_document, project_environment_document, project_function_document::Kind,
            };
            let wide =
                || JsonValue::Number("9223372036854775808".parse().expect("canonical integer"));
            let text="schema_version: '1.0'\nfunctions:\n  first: {function: project.a, description: A, params: [], returns: int, tests: [{id: wide, covers: [normal], args: {}, result: 9223372036854775808}]}\n  second: {function: project.b, description: B, params: [], returns: date, tests: [{id: bad-date, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n";
            let environment = environment_schema()
                .prepare_structural(source("environment.yaml", text))
                .unwrap();
            let text="schema_version: '1.0'\ncodelists: [{codelists: [{id: WIDE, name: Wide, data_type: integer, items: [{value: 9223372036854775808, rank: 9223372036854775808}]}]}]\n";
            let project = project_schema()
                .prepare_structural(source("project.yaml", text))
                .unwrap();
            let environment_document = &environment.normalized().document;
            let project_document_arena = &project.normalized().document;
            for (document, findings, expected, values) in [
                (
                    environment_document,
                    project_environment_document::decode(
                        environment_document,
                        environment_document.root(),
                    )
                    .unwrap_err(),
                    vec![
                        ("functions.first.tests[0].result", Kind::IntegerRange),
                        ("functions.second.tests[0].result", Kind::InvalidDate),
                    ],
                    vec![wide(), JsonValue::String("2024-02-30".into())],
                ),
                (
                    project_document_arena,
                    project_document::decode(project_document_arena, project_document_arena.root())
                        .unwrap_err(),
                    vec![
                        (
                            "codelists[0].codelists[0].items[0].value",
                            Kind::IntegerRange,
                        ),
                        (
                            "codelists[0].codelists[0].items[0].rank",
                            Kind::IntegerRange,
                        ),
                    ],
                    vec![wide(), wide()],
                ),
            ] {
                assert_eq!(
                    findings
                        .iter()
                        .map(|f| (f.path.as_str(), f.kind))
                        .collect::<Vec<_>>(),
                    expected
                );
                assert!(findings.iter().all(|f| f.node < document.nodes().len()));
                let projected = yamaa_core::project_environment_diagnostics::scalar_diagnostics(
                    document, &findings,
                )
                .unwrap();
                assert_eq!(projected.len(), expected.len());
                for (finding, value) in projected.iter().zip(values) {
                    assert_eq!(
                        (
                            finding.definition().phase,
                            finding.definition().condition,
                            finding.definition().requirement
                        ),
                        (
                            "validation",
                            "project_environment_invalid",
                            Some("REQ-0695")
                        )
                    );
                    assert_eq!(finding.context["value"], value);
                }
            }
        }
    }
    mod structural_preparation {
        //! Independent reports/effects through the live project metadata route.

        use crate::support::project_schemas::candidate_schemas;
        use std::{rc::Rc, sync::Arc};
        use yamaa_adapters::json;
        use yamaa_adapters::{
            project_environment_diagnostics as diagnostics,
            project_source::{
                self, CaptureFailure, Failure, Kind, Origin, ProjectCapturePort, Reply, Request,
            },
            specification_source::{Error, Source},
            yaml_decode::{decode_yaml, DecodeFailure},
        };
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::json::Value;
        use yamaa_core::{
            project_environment::LockKind,
            project_function::Language,
            schema::{Document, DocumentNode as N, SchemaSource},
        };

        /// The caller-supplied project file; it declares its environment link and codelists.
        const PROJECT: &str = "study/project.yaml";
        /// The linked environment file; it declares its lock and function files.
        const ENVIRONMENT: &str = "study/environment.yaml";
        const FUNCTION:&str="function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]\n";
        const CODELIST: &str =
            "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n";
        fn source(name: &str, text: &str) -> Source {
            Source {
                identity: name.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        #[derive(Debug, PartialEq)]
        enum Logical {
            Null,
            Bool(bool),
            Int(String),
            Float(u64),
            Text(String),
            Sequence(Vec<Logical>),
            Mapping(Vec<(Logical, Logical)>),
        }
        fn tree(d: &Document, id: usize) -> Logical {
            match &d.nodes()[id] {
                N::Null => Logical::Null,
                N::Boolean(v) => Logical::Bool(*v),
                N::Integer(v) => Logical::Int(v.clone()),
                N::Float(v) => Logical::Float(v.to_bits()),
                N::Text(v) => Logical::Text(v.clone()),
                N::Sequence(v) => Logical::Sequence(v.iter().map(|&id| tree(d, id)).collect()),
                N::Mapping(v) => {
                    Logical::Mapping(v.iter().map(|&(k, v)| (tree(d, k), tree(d, v))).collect())
                }
            }
        }
        fn issues(rows: Vec<Diagnostic>) -> Value {
            Value::Array(rows.into_iter().map(|row|json!({"phase":row.phase(),"condition":row.condition(),"requirement":row.requirement(),"spec_paths":row.spec_paths,"context":row.context.clone()})).collect())
        }
        fn original_locations(
            actual: &yamaa_adapters::specification_source::CapturedDocument,
            text: &str,
        ) {
            let decoded = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(actual.raw().document, decoded.document);
            assert_eq!(actual.raw().locations, decoded.locations);
        }
        // This is not a language error. No Clone/Debug/Send/Sync requirement can be added
        // by structural preparation or the surrounding live project codec integration.
        struct Opaque {
            value: Box<str>,
            _thread: Rc<()>,
        }
        struct Metadata {
            pending: Vec<(Kind, String, Result<Reply, Opaque>)>,
            trace: Vec<(Kind, String)>,
        }
        impl Metadata {
            fn new(pending: Vec<(Kind, &str, Result<Reply, Opaque>)>) -> Self {
                Self {
                    pending: pending
                        .into_iter()
                        .map(|(kind, path, reply)| (kind, path.into(), reply))
                        .collect(),
                    trace: vec![],
                }
            }
        }
        impl ProjectCapturePort for Metadata {
            type Error = Opaque;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Opaque> {
                self.trace.push((request.kind, request.written.into()));
                let (kind, path, reply) = self.pending.remove(0);
                assert_eq!(kind, request.kind);
                assert_eq!(path, request.written);
                // Each path resolves from the file that writes it.
                let declaring = match request.kind {
                    Kind::Environment | Kind::Codelist => PROJECT,
                    Kind::Lock | Kind::Function => ENVIRONMENT,
                };
                assert_eq!(request.declaring_source, declaring);
                reply
            }
        }
        fn lock() -> Reply {
            Reply::Lock {
                source: source("held/uv.lock", "version = 1\n"),
                kind: LockKind::Uv,
            }
        }
        fn document(name: &str, text: &str) -> Reply {
            Reply::Document(source(name, text))
        }

        #[test]
        fn live_root_full_reports_preserve_original_source_and_stop_before_every_dependency() {
            let schemas = candidate_schemas();
            for (text,expected) in [
        ("schema_version: '99.0'\nparent: parent.yaml\nenvironments: {python: environment.yaml}\ncodelists: [bad.yaml]\n",json!([{ "phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0281","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0","source":"study/project.yaml","entry":"study/project.yaml","environment_path":"$"}}])),
        ("schema_version: '1.0'\nparent: base.yaml\nlanguage: python\nruntime: {language: python}\nversion: old\n",json!([
            {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["parent"],"context":{"field":"parent","class":"project_class","source":"study/project.yaml","entry":"study/project.yaml","environment_path":"$"}},
            {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["language"],"context":{"field":"language","class":"project_class","source":"study/project.yaml","entry":"study/project.yaml","environment_path":"$"}},
            {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["runtime"],"context":{"field":"runtime","class":"project_class","source":"study/project.yaml","entry":"study/project.yaml","environment_path":"$"}},
            {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["version"],"context":{"field":"version","class":"project_class","source":"study/project.yaml","entry":"study/project.yaml","environment_path":"$"}}
        ])),
    ] {
        let entry=source(PROJECT,text);let bytes=entry.bytes.as_ptr();let identity=entry.identity.as_ptr();let mut port=Metadata::new(vec![]);
        let Err(Failure::Root(Error::Findings(captured)))=project_source::prepare(schemas.clone(), entry, Language::Python, &mut port) else{panic!("root structural gate")};
        assert!(port.trace.is_empty());assert!(std::ptr::eq(captured.schema(),schemas.project.as_ref()));assert_eq!(captured.source().bytes.as_ptr(),bytes);assert_eq!(captured.source().identity.as_ptr(),identity);
        assert_eq!(captured.document(),Some(&decode_yaml(text.as_bytes()).unwrap().document));assert_eq!(issues(diagnostics::schema_root(&captured).unwrap()),expected);
    }
        }

        #[test]
        fn live_environment_file_findings_keep_the_project_entry_and_stop_its_lock_and_functions() {
            let schemas = candidate_schemas();
            let root =
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [ct.yaml]\n";
            // Language and study metadata no longer belong to the environment file.
            let linked = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {bad: bad.yaml}\ncodelists: [ct.yaml]\n";
            let failed = source(ENVIRONMENT, linked);
            let bytes = failed.bytes.as_ptr();
            let identity = failed.identity.as_ptr();
            let mut port = Metadata::new(vec![
                (
                    Kind::Environment,
                    "environment.yaml",
                    Ok(Reply::Document(failed)),
                ),
                (
                    Kind::Codelist,
                    "ct.yaml",
                    Ok(document("held/ct.yaml", CODELIST)),
                ),
            ]);
            let Err(Failure::Rejected(rejected)) = project_source::prepare(
                schemas.clone(),
                source(PROJECT, root),
                Language::Python,
                &mut port,
            ) else {
                panic!("linked environment file structural rejection")
            };
            assert_eq!(
                port.trace,
                [
                    (Kind::Environment, "environment.yaml".into()),
                    (Kind::Codelist, "ct.yaml".into())
                ]
            );
            assert!(port.pending.is_empty());
            assert!(rejected.environment_file.is_none());
            assert!(rejected.lock.is_none());
            assert_eq!(rejected.captures.len(), 1);
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(
                rejected.sources[0].origin,
                Origin::Environment(Language::Python)
            );
            assert_eq!(rejected.sources[0].written, "environment.yaml");
            let CaptureFailure::Structural(Error::Findings(captured)) = &rejected.sources[0].error
            else {
                panic!("held environment file findings")
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            assert!(std::ptr::eq(
                captured.schema(),
                schemas.environment.as_ref()
            ));
            assert_eq!(
                captured.document(),
                Some(&decode_yaml(linked.as_bytes()).unwrap().document)
            );
            assert_eq!(
                issues(diagnostics::schema_sources(&rejected).unwrap()),
                json!([
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["language"],"context":{"field":"language","class":"environment_class","source":"study/environment.yaml","entry":"study/project.yaml","environment_path":"environments.python"}},
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["codelists"],"context":{"field":"codelists","class":"environment_class","source":"study/environment.yaml","entry":"study/project.yaml","environment_path":"environments.python"}}
                ])
            );
        }

        #[test]
        fn live_success_preserves_order_allocations_and_independent_default_trees() {
            let root = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [ct/sex.yaml]\n";
            let linked="schema_version: '1.0'\nlock: uv.lock\nfunctions: {external: functions/constant.yaml, inline: {function: project.constant, description: Constant., params: [], returns: int, tests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]}}\n";
            let schemas = candidate_schemas();
            let entry = source(PROJECT, root);
            let root_bytes = entry.bytes.as_ptr();
            let root_identity = entry.identity.as_ptr();
            let file = source(ENVIRONMENT, linked);
            let file_bytes = file.bytes.as_ptr();
            let file_identity = file.identity.as_ptr();
            let function = source("held/function.yaml", FUNCTION);
            let function_pointer = function.bytes.as_ptr();
            let function_identity = function.identity.as_ptr();
            let codelist = source("held/ct.yaml", CODELIST);
            let ct_pointer = codelist.bytes.as_ptr();
            let ct_identity = codelist.identity.as_ptr();
            let mut port = Metadata::new(vec![
                (
                    Kind::Environment,
                    "environment.yaml",
                    Ok(Reply::Document(file)),
                ),
                (Kind::Lock, "uv.lock", Ok(lock())),
                (
                    Kind::Function,
                    "functions/constant.yaml",
                    Ok(Reply::Document(function)),
                ),
                (Kind::Codelist, "ct/sex.yaml", Ok(Reply::Document(codelist))),
            ]);
            let prepared =
                project_source::prepare(schemas.clone(), entry, Language::Python, &mut port)
                    .unwrap_or_else(|_| panic!("complete live metadata"));
            assert_eq!(prepared.root.source().bytes.as_ptr(), root_bytes);
            assert_eq!(prepared.root.source().identity.as_ptr(), root_identity);
            original_locations(&prepared.root, root);
            assert!(Arc::ptr_eq(prepared.root.schema(), &schemas.project));
            let file = prepared.environment_file.as_ref().unwrap();
            assert_eq!(file.source().bytes.as_ptr(), file_bytes);
            assert_eq!(file.source().identity.as_ptr(), file_identity);
            original_locations(file, linked);
            assert!(Arc::ptr_eq(file.schema(), &schemas.environment));
            assert_eq!(prepared.captures.len(), 2);
            let function = &prepared.captures[0].document;
            let codelist = &prepared.captures[1].document;
            assert_eq!(function.source().bytes.as_ptr(), function_pointer);
            assert_eq!(function.source().identity.as_ptr(), function_identity);
            assert_eq!(codelist.source().bytes.as_ptr(), ct_pointer);
            assert_eq!(codelist.source().identity.as_ptr(), ct_identity);
            original_locations(function, FUNCTION);
            original_locations(codelist, CODELIST);
            assert_eq!(function.class_name(), "function_definition_class");
            assert_eq!(codelist.class_name(), "codelist_source_class");
            assert!(Arc::ptr_eq(function.schema(), &schemas.environment));
            assert!(Arc::ptr_eq(codelist.schema(), &schemas.project));
            let expected_function="function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\nmay_return_missing: false\ncomparison_decimals: 4\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]\n";
            let expected_ct="codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}, {value: M, extended: false}]}]\n";
            for (actual, expected) in [(function, expected_function), (codelist, expected_ct)] {
                let expected = decode_yaml(expected.as_bytes()).unwrap().document;
                let d = &actual.normalized().document;
                assert_eq!(tree(d, d.root()), tree(&expected, expected.root()));
                assert!(d.field(d.root(), "schema_version").is_none());
                for origin in &actual.normalized().origins {
                    match origin.source {
                        SchemaSource::Input => {
                            assert!(origin.node < actual.raw().document.nodes().len())
                        }
                        SchemaSource::Default { module, descriptor } => {
                            assert_eq!(
                                actual.schema().structure().descriptors()[descriptor].module,
                                module
                            );
                            assert!(
                                origin.node
                                    < actual.schema().structure().modules()[module]
                                        .document
                                        .nodes()
                                        .len()
                            );
                        }
                    }
                }
            }
            assert_eq!(
                port.trace,
                vec![
                    (Kind::Environment, "environment.yaml".into()),
                    (Kind::Lock, "uv.lock".into()),
                    (Kind::Function, "functions/constant.yaml".into()),
                    (Kind::Codelist, "ct/sex.yaml".into()),
                ]
            );
            assert!(port.pending.is_empty());
            assert_eq!(prepared.origins.functions, ["external", "inline"]);
            assert_eq!(prepared.origins.codelists, [0]);
            // The inline definition's default is normalized in the environment file.
            let document = &file.normalized().document;
            let inline = document
                .field(
                    document.field(document.root(), "functions").unwrap(),
                    "inline",
                )
                .unwrap();
            assert_eq!(
                document.nodes()[document.field(inline, "comparison_decimals").unwrap()],
                N::Integer("4".into())
            );
        }

        #[test]
        fn external_findings_keep_full_original_provenance_and_collect_later_live_sources() {
            let root = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [ct.yaml]\n";
            let linked =
                "schema_version: '1.0'\nlock: uv.lock\nfunctions: {bad: bad.yaml, good: good.yaml}\n";
            let bad =
                FUNCTION.to_owned() + "contract_version: '1'\nbinding: {call: project.constant}\n";
            let schemas = candidate_schemas();
            let failed = source("held/bad.yaml", &bad);
            let bytes = failed.bytes.as_ptr();
            let identity = failed.identity.as_ptr();
            let mut port = Metadata::new(vec![
                (
                    Kind::Environment,
                    "environment.yaml",
                    Ok(document(ENVIRONMENT, linked)),
                ),
                (Kind::Lock, "uv.lock", Ok(lock())),
                (Kind::Function, "bad.yaml", Ok(Reply::Document(failed))),
                (
                    Kind::Function,
                    "good.yaml",
                    Ok(document("held/good.yaml", FUNCTION)),
                ),
                (
                    Kind::Codelist,
                    "ct.yaml",
                    Ok(document("held/ct.yaml", CODELIST)),
                ),
            ]);
            let Err(Failure::Rejected(rejected)) = project_source::prepare(
                schemas.clone(),
                source(PROJECT, root),
                Language::Python,
                &mut port,
            ) else {
                panic!("external structural rejection")
            };
            assert_eq!(
                port.trace
                    .iter()
                    .map(|(_, path)| path.as_str())
                    .collect::<Vec<_>>(),
                [
                    "environment.yaml",
                    "uv.lock",
                    "bad.yaml",
                    "good.yaml",
                    "ct.yaml"
                ]
            );
            assert_eq!(rejected.captures.len(), 2);
            assert_eq!(rejected.sources.len(), 1);
            let CaptureFailure::Structural(Error::Findings(captured)) = &rejected.sources[0].error
            else {
                panic!()
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            assert!(std::ptr::eq(
                captured.schema(),
                schemas.environment.as_ref()
            ));
            assert_eq!(
                captured.document(),
                Some(&decode_yaml(bad.as_bytes()).unwrap().document)
            );
            assert_eq!(
                issues(diagnostics::schema_sources(&rejected).unwrap()),
                json!([
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["<normalization>.contract_version"],"context":{"field":"contract_version","class":"function_definition_class","source":"held/bad.yaml","entry":"study/project.yaml","environment_path":"functions.bad"}},
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["<normalization>.binding"],"context":{"field":"binding","class":"function_definition_class","source":"held/bad.yaml","entry":"study/project.yaml","environment_path":"functions.bad"}}
                ])
            );
        }

        #[test]
        fn live_opaque_capture_identity_and_collection_stay_outside_structural_codec() {
            let root = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [ct.yaml]\n";
            let linked = "schema_version: '1.0'\nlock: uv.lock\nfunctions: {missing: missing.yaml, good: good.yaml}\n";
            let payload = Opaque {
                value: "actual capture error".into(),
                _thread: Rc::new(()),
            };
            let pointer = payload.value.as_ptr();
            let mut port = Metadata::new(vec![
                (
                    Kind::Environment,
                    "environment.yaml",
                    Ok(document(ENVIRONMENT, linked)),
                ),
                (Kind::Lock, "uv.lock", Ok(lock())),
                (Kind::Function, "missing.yaml", Err(payload)),
                (
                    Kind::Function,
                    "good.yaml",
                    Ok(document("held/good.yaml", FUNCTION)),
                ),
                (
                    Kind::Codelist,
                    "ct.yaml",
                    Ok(document("held/ct.yaml", CODELIST)),
                ),
            ]);
            let Err(Failure::Rejected(rejected)) = project_source::prepare(
                candidate_schemas(),
                source(PROJECT, root),
                Language::Python,
                &mut port,
            ) else {
                panic!()
            };
            assert_eq!(
                rejected.sources[0].origin,
                Origin::Function("missing".into())
            );
            let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                panic!()
            };
            assert_eq!(original.value.as_ptr(), pointer);
            assert_eq!(&*original.value, "actual capture error");
            assert_eq!(rejected.captures.len(), 2);
            assert_eq!(port.trace.len(), 5);
            assert!(port.pending.is_empty());
        }

        #[test]
        fn named_predecode_guards_and_physical_query_failure_categories_remain_exact() {
            let schemas = candidate_schemas();
            let malformed = || source("held/invalid.yaml", "[unclosed");
            for (schema, root, own) in [
                (
                    &schemas.environment,
                    "environment_class",
                    "function_definition_class",
                ),
                (&schemas.project, "project_class", "codelist_source_class"),
            ] {
                for name in [
                    "",
                    "str",
                    "function_definition",
                    "codelist_source",
                    "unknown",
                ] {
                    assert!(
                        matches!(schema.prepare_named_class(malformed(),name),Err(Error::UnknownClass(actual)) if actual==name)
                    );
                }
                assert!(matches!(
                    schema.prepare_named_class(malformed(), root),
                    Err(Error::VersionedRootRequired)
                ));
                assert!(matches!(
                    schema.prepare_named_class(malformed(), own),
                    Err(Error::Decode {
                        error: DecodeFailure::InvalidYaml { .. },
                        ..
                    })
                ));
            }
        }

        #[test]
        fn root_and_named_complete_reports_keep_unbounded_integer_context_and_compatibility_shapes()
        {
            use yamaa_adapters::specification_diagnostics::capture_failure;
            let schemas = candidate_schemas();
            let root = "schema_version: 92233720368547758081234567890\n";
            let expected:Value=yamaa_adapters::json_wire::decode("[{\"phase\":\"validation\",\"condition\":\"schema_version_mismatch\",\"requirement\":\"REQ-0281\",\"spec_paths\":[\"schema_version\"],\"context\":{\"expected\":\"1.0\",\"actual\":92233720368547758081234567890}}]").unwrap();
            for (schema, identity) in [
                (&schemas.environment, ENVIRONMENT),
                (&schemas.project, PROJECT),
            ] {
                let entry = source(identity, root);
                let bytes = entry.bytes.as_ptr();
                let error = schema.prepare_structural(entry).unwrap_err();
                let Error::Findings(captured) = &error else {
                    panic!()
                };
                assert_eq!(captured.source().bytes.as_ptr(), bytes);
                assert!(captured.document().is_some());
                assert_eq!(crate::support::findings(capture_failure(&error)), expected);
            }
            let error = schemas
                .environment
                .prepare_named_class(
                    source(
                        "held/function.yaml",
                        &(FUNCTION.to_owned()
                            + "schema_version: '1.0'\ncontract_version: '1'\nbinding: {}\n"),
                    ),
                    "function_definition_class",
                )
                .unwrap_err();
            assert_eq!(
                crate::support::findings(capture_failure(&error)),
                json!([
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["<normalization>.schema_version"],"context":{"field":"schema_version","class":"function_definition_class"}},
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["<normalization>.contract_version"],"context":{"field":"contract_version","class":"function_definition_class"}},
                    {"phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["<normalization>.binding"],"context":{"field":"binding","class":"function_definition_class"}}
                ])
            );
        }

        #[test]
        fn live_codelist_class_failure_keeps_full_raw_source_report_and_later_capture() {
            let root = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [bad.yaml, good.yaml]\n";
            let bad = CODELIST.to_owned() + "retired: value\n";
            let failed = source("held/bad-ct.yaml", &bad);
            let bytes = failed.bytes.as_ptr();
            let identity = failed.identity.as_ptr();
            let schemas = candidate_schemas();
            let mut port = Metadata::new(vec![
                (
                    Kind::Environment,
                    "environment.yaml",
                    Ok(document(
                        ENVIRONMENT,
                        "schema_version: '1.0'\nlock: uv.lock\n",
                    )),
                ),
                (Kind::Lock, "uv.lock", Ok(lock())),
                (Kind::Codelist, "bad.yaml", Ok(Reply::Document(failed))),
                (
                    Kind::Codelist,
                    "good.yaml",
                    Ok(document("held/good-ct.yaml", CODELIST)),
                ),
            ]);
            let Err(Failure::Rejected(rejected)) = project_source::prepare(
                schemas.clone(),
                source(PROJECT, root),
                Language::Python,
                &mut port,
            ) else {
                panic!("external codelist structural rejection")
            };
            assert_eq!(
                port.trace
                    .iter()
                    .map(|(kind, path)| (*kind, path.as_str()))
                    .collect::<Vec<_>>(),
                [
                    (Kind::Environment, "environment.yaml"),
                    (Kind::Lock, "uv.lock"),
                    (Kind::Codelist, "bad.yaml"),
                    (Kind::Codelist, "good.yaml")
                ]
            );
            assert!(port.pending.is_empty());
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.captures.len(), 1);
            let CaptureFailure::Structural(Error::Findings(captured)) = &rejected.sources[0].error
            else {
                panic!()
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            assert!(std::ptr::eq(captured.schema(), schemas.project.as_ref()));
            assert_eq!(
                captured.document(),
                Some(&decode_yaml(bad.as_bytes()).unwrap().document)
            );
            assert_eq!(
                issues(diagnostics::schema_sources(&rejected).unwrap()),
                json!([{ "phase":"validation","condition":"unknown_field","requirement":"REQ-0285","spec_paths":["<normalization>.retired"],"context":{"field":"retired","class":"codelist_source_class","source":"held/bad-ct.yaml","entry":"study/project.yaml","environment_path":"codelists[0]"}}])
            );
        }
    }
    mod validator_r_declarations {
        //! Original R authoring declarations remain typed witnesses when R is absent.
        //! This proves metadata admission only. The mandatory installed R replay owns
        //! lock verification, activation, source binding, values and publication.
        use std::{convert::Infallible, path::PathBuf};
        use yamaa_adapters::{
            project_check,
            project_run::PreparedRun,
            project_source::{self, Kind, ProjectCapturePort, Reply, Request},
            shipped_schema,
            specification_source::Source,
        };
        use yamaa_core::{project_environment::LockKind, project_function::Language};

        /// Serves the authored R environment file the benchmark project links.
        struct DeclaredEnvironment {
            case: PathBuf,
            project: String,
            environment: String,
            environments: usize,
            locks: usize,
        }
        impl ProjectCapturePort for DeclaredEnvironment {
            type Error = Infallible;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                match request.kind {
                    // The R host reads only the `r` link; the Python link stays unopened.
                    Kind::Environment => {
                        assert_eq!(request.declaring_source, self.project);
                        assert_eq!(request.written, "r/environment.yaml");
                        self.environments += 1;
                        Ok(Reply::Document(Source {
                            identity: self.environment.clone(),
                            bytes: std::fs::read(self.case.join(request.written)).unwrap(),
                        }))
                    }
                    Kind::Lock => {
                        // The lock path resolves from the environment file that writes it.
                        assert_eq!(request.declaring_source, self.environment);
                        assert_eq!(request.written, "renv.lock");
                        self.locks += 1;
                        // Static admission borrows only the declared lock format. This authored
                        // codec fixture grants no installed-package or verification evidence.
                        Ok(Reply::Lock {
                            source: Source {
                                identity: "typed-fixture/renv.lock".into(),
                                bytes: b"{}".to_vec(),
                            },
                            kind: LockKind::Renv,
                        })
                    }
                    kind => panic!("unexpected {kind:?} capture"),
                }
            }
        }

        fn replay(name: &str, expected: Option<(&str, &str, &str)>) {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            let case = root.join(name);
            let mut port = DeclaredEnvironment {
                case: case.clone(),
                project: format!("benchmarks/{name}/project.yaml"),
                environment: format!("benchmarks/{name}/r/environment.yaml"),
                environments: 0,
                locks: 0,
            };
            let environment = project_source::prepare(
                shipped_schema::capture_project().unwrap(),
                Source {
                    identity: port.project.clone(),
                    bytes: std::fs::read(case.join("project.yaml")).unwrap(),
                },
                Language::R,
                &mut port,
            )
            .unwrap()
            .into_owned();
            assert_eq!((port.environments, port.locks), (1, 1));
            assert_eq!(environment.environment().host(), Language::R);
            assert_eq!(environment.environment().language(), Some(Language::R));
            assert_eq!(
                environment.environment_file().unwrap().source().identity,
                port.environment
            );
            let document = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(Source {
                    identity: format!("benchmarks/{name}/spec.yaml"),
                    bytes: std::fs::read(case.join("spec.yaml")).unwrap(),
                })
                .unwrap();
            let rows = match PreparedRun::prepare(document, environment) {
                Ok(run) => project_check::prepared(&run),
                Err(run) => project_check::rejected(&run).unwrap(),
            };
            match expected {
                None => assert!(rows.is_empty(), "{name}: {rows:?}"),
                Some((condition, requirement, path)) => {
                    assert_eq!(rows.len(), 1, "{name}: {rows:?}");
                    let issue = &rows[0];
                    assert_eq!(issue.phase(), "validation");
                    assert_eq!(issue.condition(), condition);
                    assert_eq!(issue.requirement(), Some(requirement));
                    assert_eq!(issue.spec_paths, [path]);
                    assert_eq!(
                        yamaa_core::json::Value::Object(issue.context.clone()),
                        yamaa_adapters::json!({"argument":"x", "cause":"missing_required_argument", "function":"project_value"})
                    );
                }
            }
            // No activation, source-data or publication port exists in this replay.
            assert_eq!((port.environments, port.locks), (1, 1));
        }

        #[test]
        fn schema_functions_r_declared_calls_are_statically_admitted() {
            replay("schema-functions", None);
        }
        #[test]
        fn schema_non_finite_r_declared_calls_are_statically_admitted() {
            replay("schema-non-finite", None);
        }
        #[test]
        fn adam_adsl_bmi_r_declared_calls_are_statically_admitted() {
            replay("adam-adsl-bmi", None);
        }
        #[test]
        fn adam_advs_percentiles_r_declared_calls_are_statically_admitted() {
            replay("adam-advs-percentiles", None);
        }
        #[test]
        fn negative_function_contract_r_preserves_the_exact_static_issue() {
            replay(
                "negative-function-contract",
                Some((
                    "invalid_function_argument",
                    "REQ-0700",
                    "columns.RESULT.derivation.function.args",
                )),
            );
        }
    }
}
