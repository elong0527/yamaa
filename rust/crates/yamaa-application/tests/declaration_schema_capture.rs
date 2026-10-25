#![cfg(feature = "yaml")]
//! Declaration contracts: schema capture, schema declaration fixture, schema
//! diagnostic projection, the shipped schema route, static declarations,
//! string template syntax fixture and validator R declarations.
mod declaration {
    mod schema_capture {
        use std::sync::Arc;
        use yamaa_adapters::json;
        use yamaa_adapters::{
            document::{self, Closures},
            specification_diagnostics::{capture_failure, Rejection},
            specification_source::{CapturedSchema, Error, Source},
            yaml_decode::SourcePosition,
        };
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::schema::{
            BundleError, BundleIssue, BundleIssueKind, DocumentNode as N, NormalizationError,
            SchemaSource,
        };
        const SHARED:&str="version: '1.0'\nincludes: []\nshared:\n  - id: {type: str, required: true}\n  - label: {type: str, default: untitled}\n";
        const ROOT:&str="version: '1.0'\nincludes: [schema_s.yaml]\nroot_class:\n  - yamaa: {type: str}\n  - type: {type: str}\n  - fields_from: shared\n";
        fn source(identity: &str, bytes: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.as_bytes().to_vec(),
            }
        }
        /// Admit `raw`, a 1.x document declaring `type: spec`, against
        /// `schema` as the closure of its type.
        fn admit(
            schema: &Arc<CapturedSchema>,
            raw: Source,
        ) -> Result<document::AdmittedDocument, Error> {
            let closures = Closures::shipped()
                .unwrap()
                .with(FileType::Spec, Arc::clone(schema));
            document::decode(raw, FileType::Spec, Some(LANGUAGE_VERSION), &closures)?.admit()
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
                let descriptor = schema.structure().root_class().fields[3].descriptor;
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
                let raw = source("domain.yaml", "yamaa: '1.0'\ntype: spec\nid: ID01\n");
                let raw_pointer = raw.bytes.as_ptr();
                let prepared = admit(&schema, raw).unwrap();
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
                    finding["rule"] = json!(match finding["condition"].as_str().unwrap() {
                        "non_ascii_source" => "SPEC-8",
                        "invalid_text" => "VALUE-11",
                        "invalid_yaml" => "SCHEMA-3",
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
                    yamaa_core::application_issue::Step::Capture,
                    yamaa_core::application_issue::Fault::SchemaBundle,
                ))
            );
            let schema=CapturedSchema::admit(vec![source("r","version: '1.0'\nincludes: []\nroot_class:\n  - yamaa: {type: str}\n  - type: {type: str}\n  - value: {type: int, default: invalid}\n")],0).unwrap();
            assert!(matches!(
                admit(&schema, source("root.yaml", "yamaa: '1.0'\ntype: spec\n")),
                Err(Error::Normalize(NormalizationError::Defaults(_)))
            ));
        }

        #[test]
        fn fixed_yaml_file_size_boundary_precedes_physical_decoding() {
            use yamaa_adapters::yaml_decode::DecodeFailure;
            use yamaa_core::limits::{LimitExceeded, Limits};
            let prefix = "version: '1.0'\nincludes: []\nroot_class: [{name: {type: str}}]\n#";
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
                    {"phase":"validation", "condition":"missing_required_field", "rule":"SCHEMA-13",
                        "spec_paths":["needed"], "context":{"field":"needed", "class":"root_class"}},
                    {"phase":"validation", "condition":"invalid_field_type", "rule":"SCHEMA-12",
                        "spec_paths":["count"], "context":{"expected":"int", "actual":"bool"}},
                    {"phase":"validation", "condition":"unknown_field", "rule":"SCHEMA-13",
                        "spec_paths":["extra"], "context":{"field":"extra", "class":"root_class", "identifier":"extra"}}
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
            "rule":"SCHEMA-12", "spec_paths":["items[1]"],
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
        fn a_named_root_class_uses_the_complete_authored_include_closure() {
            let sources = vec![
        (
            "schema_entry.yaml".into(),
            "version: '1.0'\nincludes: [schema_shared.yaml]\nentry_class:\n  - language: {type: str, values: [python, r]}\n  - lock: {type: project_path}\n".into(),
        ),
        (
            "schema_shared.yaml".into(),
            "version: '1.0'\nproject_path: {type: str, min_length: 1}\n".into(),
        ),
    ];
            let observed = fixture::observe_input(
                fixture::SchemaInput::Modules(sources),
                "language: false\nlock: owned.lock\n",
                Some("entry_class"),
            );
            assert_eq!(
                observed,
                json!({"issues": [{"phase":"validation", "condition":"invalid_field_type",
        "rule":"SCHEMA-12", "spec_paths":["language"],
        "context":{"expected":"str", "actual":"bool"}}]})
            );
        }
    }
    mod schema_diagnostic_projection {
        use std::sync::Arc;
        use yamaa_adapters::json;
        use yamaa_adapters::{
            document::{self, Closures},
            specification_diagnostics::capture_failure,
            specification_source::{CapturedSchema, Error, Source},
        };
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};

        const BIG: &str = "922337203685477580812345678901234567890";
        fn source(identity: &str, text: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        /// The refusal of `raw`, a 1.x document declaring `type: spec`,
        /// admitted against `schema` as the closure of its type.
        fn refused(schema: &Arc<CapturedSchema>, raw: Source) -> Error {
            let closures = Closures::shipped()
                .unwrap()
                .with(FileType::Spec, Arc::clone(schema));
            document::decode(raw, FileType::Spec, Some(LANGUAGE_VERSION), &closures)
                .unwrap()
                .admit()
                .unwrap_err()
        }

        #[test]
        fn captured_structural_projection_preserves_complete_ordered_constraint_outcome_and_authority(
        ) {
            let schema = CapturedSchema::admit_root(vec![source("schema.yaml", &format!(
        "version: '1.0'\npayload:\n  - yamaa: {{type: str}}\n  - type: {{type: str}}\n  - alpha: {{type: str, values: [z, a, z], pattern: '^OK$', min_length: {BIG}}}\n  - items: {{type: 'list[int]', size: {BIG}}}\n"))],
        0, "payload").unwrap();
            let raw = "yamaa: '1.0'\ntype: spec\nalpha: \"bad\\\"\\n\\u03BB\"\nitems: [1]\n";
            let error = refused(&schema, source("held/entry.yaml", raw));
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
        {{"phase":"validation","condition":"value_not_permitted","rule":"SCHEMA-12","spec_paths":["alpha"],"context":{{"value":"bad\"\nλ","permitted":["z","a","z"]}}}},
        {{"phase":"validation","condition":"pattern_mismatch","rule":"SCHEMA-12","spec_paths":["alpha"],"context":{{"value":"bad\"\nλ","pattern":"^OK$"}}}},
        {{"phase":"validation","condition":"minimum_length","rule":"SCHEMA-12","spec_paths":["alpha"],"context":{{"minimum":{BIG}}}}},
        {{"phase":"validation","condition":"invalid_size","rule":"SCHEMA-12","spec_paths":["items"],"context":{{"size":{BIG}}}}}
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
        "version: '1.0'\npayload:\n  - yamaa: {type: str}\n  - type: {type: str}\n  - first: {type: str, values: [OK]}\n  - last: {type: str, values: [OK]}\n")],
        0, "payload").unwrap();
            let raw = format!(
                "yamaa: '1.0'\ntype: spec\nfirst: '{}'\nlast: '{}'\n",
                "x".repeat(1_400_000),
                "y".repeat(1_400_000)
            );
            let error = refused(&schema, source("held/entry.yaml", &raw));
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
                {"phase":"validation","condition":"value_not_permitted","rule":"SCHEMA-12","spec_paths":["first"],"context":{"value":"x".repeat(1_400_000),"permitted":["OK"]}},
                {"phase":"validation","condition":"value_not_permitted","rule":"SCHEMA-12","spec_paths":["last"],"context":{"value":"y".repeat(1_400_000),"permitted":["OK"]}}
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
        //! The shipped 1.0 specification route, before any host effect.
        use yamaa_adapters::{shipped_schema, specification_source::Source};
        use yamaa_core::inheritance::Source as Identity;
        use yamaa_core::resolver::{Captured, Failure, Name, Resolver};

        struct NoParents;
        impl Resolver for NoParents {
            type Error = ();
            fn inspect(&mut self, _: Name<'_>) -> Result<Identity, Self::Error> {
                panic!("entry rejection or standalone preparation must not resolve a parent")
            }
            fn capture(&mut self, _: Name<'_>) -> Result<Captured, Self::Error> {
                panic!("entry rejection or standalone preparation must not read a parent")
            }
            fn failure(&self, _: Name<'_>, _: &Self::Error) -> Option<Failure> {
                None
            }
            fn rebase(
                &mut self,
                _: &Identity,
                _: &Identity,
                _: &str,
            ) -> Result<String, Self::Error> {
                panic!("entry rejection or standalone preparation must not rebase a parent")
            }
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
                    finding["rule"] = json!(match finding["condition"].as_str().unwrap() {
                        "non_ascii_source" => "SPEC-8",
                        "invalid_text" => "VALUE-11",
                        "invalid_yaml" => "SCHEMA-3",
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
    }
    mod static_declarations {
        use yamaa_application::{specification_check, specification_run::PreparedRun};

        #[test]
        fn arithmetic_predicates_are_admitted_at_every_predicate_site() {
            // The three predicate sites from #2204: a case `when`, a root `filter`
            // and an `assert.require`. Arithmetic over numeric operands belongs to
            // the one expression language, so none of them is refused.
            let head = "yamaa: '1.0'\ntype: spec\ndomain: ADVS\nkeys: [USUBJID]\ninput: {VS: {path: never-read.csv, types: {SBPSUP: float, SBPSTD: float}}}\noutput: {path: advs.csv, columns: [USUBJID, OHYPO]}\ncolumns:\n  - {name: USUBJID, type: str, derivation: VS.USUBJID}\n  - {name: SBPSUP, type: float, derivation: VS.SBPSUP}\n  - {name: SBPSTD, type: float, derivation: VS.SBPSTD}\n";
            let rows = |extra: &str| {
                let document =
                    crate::support::specification_run::prepare(format!("{head}{extra}").as_bytes());
                specification_check::issues(&PreparedRun::prepare(document).unwrap())
            };
            for (extra, path) in [
                (
                    "  - name: OHYPO\n    type: str\n    derivation:\n      case:\n        - {when: 'SBPSTD - SBPSUP <= -20', then: \"'Y'\"}\n        - {otherwise: \"'N'\"}\n",
                    "columns.OHYPO.derivation.case[0].when",
                ),
                (
                    "  - {name: OHYPO, type: str, derivation: \"'N'\"}\nfilter: 'VS.SBPSTD - VS.SBPSUP < 0'\n",
                    "filter",
                ),
                (
                    "  - {name: OHYPO, type: str, derivation: \"'N'\"}\nverifications:\n  - {assert: {when: \"OHYPO = 'Y'\", require: 'SBPSTD - SBPSUP <= -20'}}\n",
                    "verifications[0].assert.require",
                ),
            ] {
                let rows = rows(extra);
                assert!(rows.is_empty(), "{path}: {rows:?}");
            }
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
            "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{DM: never-read.csv}}\noutput: {{path: result.csv, columns: [ID, B, F]}}\ncolumns:\n  - {{name: ID, type: str, derivation: DM.ID}}\n  - {{name: B, type: int, derivation: 2}}\n  - {{name: F, type: str, derivation: {{flag: {{condition: 'B > 1', false_value: {}}}}}}}\n"
        , yamaa_adapters::json_wire::encode(&false_value));
                let document = crate::support::specification_run::prepare(text.as_bytes());
                let run = PreparedRun::prepare(document).unwrap();
                let rows = specification_check::issues(&run);
                assert_eq!(
                    rows.len(),
                    1,
                    "{}: {rows:?}",
                    yamaa_adapters::json_wire::encode(&false_value)
                );
                assert_eq!(rows[0].condition(), "missing_value_required");
                assert_eq!(rows[0].rule(), Some("EXPR-8"));
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
                    "rule":"TEXT-3",
                    "context":{"reason":"invalid_placeholder","placeholder":"A + B"},
                    "position":{"byte":2,"character":1},
                    "span":{"byte_start":2,"byte_end":9,"character_start":1,"character_end":8}
                })
            );
        }
    }
    mod validator_r_declarations {
        //! Original R authoring declarations remain typed witnesses when R is absent.
        //! This proves metadata admission only. The mandatory installed R replay owns
        //! lock verification, activation, source binding, values and publication.
        use crate::support::Edit as _;
        use std::{convert::Infallible, path::PathBuf};
        use yamaa_adapters::{
            document::Closures,
            project_source::{self, Kind},
            specification_source::Source,
        };
        use yamaa_application::{specification_check, specification_run::PreparedRun};
        use yamaa_core::project_function::Language;
        use yamaa_core::resolver::{Captured, Failure, File, Located, Name, Resolver};

        /// Serves the function files and the R lock the benchmark project
        /// names (D8); every path resolves from the project file.
        struct DeclaredProject {
            case: PathBuf,
            project: String,
            functions: usize,
            locks: usize,
            /// The bytes of the file last located.
            held: Vec<u8>,
        }
        impl DeclaredProject {
            /// The held source the double serves for the file a project names.
            fn answer(
                &mut self,
                kind: Kind,
                declaring: &str,
                written: &str,
            ) -> Result<Source, Infallible> {
                assert_eq!(declaring, self.project);
                match kind {
                    Kind::Function => {
                        self.functions += 1;
                        Ok(Source {
                            identity: self.project.edit("project.yaml", written),
                            bytes: std::fs::read(self.case.join(written)).unwrap(),
                        })
                    }
                    // The R host reads only its own lock; the Python lock stays unopened.
                    Kind::Lock => {
                        assert_eq!(written, "r/renv.lock");
                        self.locks += 1;
                        // Static admission borrows only the declared lock format. This authored
                        // codec fixture grants no installed-package or verification evidence.
                        Ok(Source {
                            identity: "typed-fixture/renv.lock".into(),
                            bytes: crate::support::ports::RENV_LOCK.as_bytes().to_vec(),
                        })
                    }
                    kind => panic!("unexpected {kind:?} capture"),
                }
            }
        }
        /// A file is served when it is located; its capture reads the bytes served.
        impl Resolver for DeclaredProject {
            type Error = Infallible;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Self::Error> {
                let Name::Written {
                    file: File::Project(kind),
                    declaring,
                    written,
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
                let source = self.answer(kind, declaring, written)?;
                self.held = source.bytes;
                Ok(Located {
                    display_path: source.identity.clone(),
                    identity: source.identity,
                })
            }
            fn capture(&mut self, _: Name<'_>) -> Result<Captured, Self::Error> {
                Ok(Captured::resource(
                    std::mem::take(&mut self.held).into(),
                    true,
                ))
            }
            fn failure(&self, _: Name<'_>, _: &Self::Error) -> Option<Failure> {
                None
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Self::Error> {
                panic!("a project file is never rebased")
            }
        }

        /// Replay one specification of a case, `spec`, against its project.
        fn replay(name: &str, spec: &str, expected: Option<(&str, &str, &str)>) {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            let case = root.join(name);
            let mut port = DeclaredProject {
                case: case.clone(),
                project: format!("benchmarks/{name}/project.yaml"),
                functions: 0,
                locks: 0,
                held: vec![],
            };
            let environment = project_source::prepare(
                Closures::shipped().unwrap(),
                Source {
                    identity: port.project.clone(),
                    bytes: std::fs::read(case.join("project.yaml")).unwrap(),
                },
                Language::R,
                &mut port,
            )
            .unwrap()
            .into_owned();
            // The project's one function file and its R lock.
            assert_eq!((port.functions, port.locks), (1, 1));
            assert_eq!(environment.environment().host(), Some(Language::R));
            assert_eq!(environment.environment().language(), Some(Language::R));
            let document = crate::support::specification_run::prepare(
                &std::fs::read(case.join(spec)).unwrap(),
            );
            let rows = match PreparedRun::prepare_with_environment(document, environment) {
                Ok(run) => specification_check::issues(&run),
                Err(run) => specification_check::rejected(&run).unwrap(),
            };
            match expected {
                None => assert!(rows.is_empty(), "{name}: {rows:?}"),
                Some((condition, rule, path)) => {
                    assert_eq!(rows.len(), 1, "{name}: {rows:?}");
                    let issue = &rows[0];
                    assert_eq!(issue.phase(), "validation");
                    assert_eq!(issue.condition(), condition);
                    assert_eq!(issue.rule(), Some(rule));
                    assert_eq!(issue.spec_paths, [path]);
                    assert_eq!(
                        yamaa_core::json::Value::Object(issue.context.clone()),
                        yamaa_adapters::json!({"argument":"x", "cause":"missing_required_argument", "function":"project_value"})
                    );
                }
            }
            // No activation, source-data or publication port exists in this replay.
            assert_eq!((port.functions, port.locks), (1, 1));
        }

        #[test]
        fn schema_functions_r_declared_calls_are_statically_admitted() {
            replay("schema-functions", "spec.yaml", None);
        }
        #[test]
        fn schema_non_finite_r_declared_calls_are_statically_admitted() {
            replay("schema-non-finite", "spec.yaml", None);
        }
        #[test]
        fn adam_adsl_bmi_r_declared_calls_are_statically_admitted() {
            replay("adam-adsl-bmi", "spec.yaml", None);
        }
        #[test]
        fn adam_advs_percentiles_r_declared_calls_are_statically_admitted() {
            replay("adam-advs-percentiles", "spec_function.yaml", None);
        }
        #[test]
        fn negative_function_contract_r_preserves_the_exact_static_issue() {
            replay(
                "negative-function-contract",
                "spec.yaml",
                Some((
                    "invalid_function_argument",
                    "FUNC-5",
                    "columns.RESULT.derivation.function.args",
                )),
            );
        }
    }
}
