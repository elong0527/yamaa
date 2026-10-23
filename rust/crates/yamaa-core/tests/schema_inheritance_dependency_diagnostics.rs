//! Schema contracts: inheritance dependency diagnostics, scalar submission
//! rules, schema bundle, schema composition, schema descriptor, schema
//! diagnostic projection and schema document.
mod schema {
    mod inheritance_dependency_diagnostics {
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{diagnostic::inheritance_dependency_diagnostics, resolution::Issue};

        #[test]
        fn complete_ordered_facts_name_each_cycle_by_its_closed_loop() {
            // One detector names the closed loop for layered and single-file
            // specs alike (#2844, decision 1).
            let issues = [
                Issue::Unknown {
                    column: "VALUE\0é".into(),
                    dependency: "MISSING🙂".into(),
                },
                Issue::Cycle {
                    cycle: vec!["A".into(), "B".into(), "A".into()],
                    paths: vec![
                        "columns.A.derivation.compute".into(),
                        "columns.B.derivation.compute".into(),
                    ],
                },
            ];
            let findings = inheritance_dependency_diagnostics(&issues);
            assert_eq!(findings.len(), 2);
            assert_eq!(findings[0].spec_paths, ["columns.VALUE\0é.derivation"]);
            assert_eq!(
                findings[0].context["dependency"],
                JsonValue::String("MISSING🙂".into())
            );
            assert_eq!(
                findings[1].spec_paths,
                [
                    "columns.A.derivation.compute",
                    "columns.B.derivation.compute"
                ]
            );
            assert_eq!(
                findings[1].context["cycle"],
                JsonValue::Array(
                    ["A", "B", "A"]
                        .into_iter()
                        .map(|name| JsonValue::String(name.into()))
                        .collect()
                )
            );
            assert_eq!(findings[1].definition().condition, "dependency_cycle");
            assert_eq!(findings[1].definition().rule, Some("COLUMN-6"));
            assert!(findings
                .iter()
                .all(|finding| finding.source_span.is_none() && finding.operand_route.is_none()));
        }

        #[test]
        fn later_long_dependency_facts_remain_complete_in_order() {
            let later = "\"\\\0🙂".repeat(1000);
            let issues = [
                Issue::Unknown {
                    column: "VALUE".into(),
                    dependency: "FIRST".into(),
                },
                Issue::Unknown {
                    column: "LATER".into(),
                    dependency: later.clone(),
                },
            ];
            let projected = inheritance_dependency_diagnostics(&issues);
            assert_eq!(projected.len(), 2);
            assert_eq!(projected[0].spec_paths, ["columns.VALUE.derivation"]);
            assert_eq!(
                projected[0].context,
                [
                    ("column".into(), JsonValue::String("VALUE".into())),
                    ("dependency".into(), JsonValue::String("FIRST".into())),
                ]
                .into()
            );
            assert_eq!(projected[1].spec_paths, ["columns.LATER.derivation"]);
            assert_eq!(
                projected[1].context,
                [
                    ("column".into(), JsonValue::String("LATER".into())),
                    ("dependency".into(), JsonValue::String(later)),
                ]
                .into()
            );
            assert_eq!(
                issues[0],
                Issue::Unknown {
                    column: "VALUE".into(),
                    dependency: "FIRST".into()
                }
            );
        }

        #[test]
        fn all_empty_cycle_members_remain_in_the_retained_order() {
            let issues = [Issue::Cycle {
                cycle: vec![String::new(); 1000],
                paths: vec![String::new(); 999],
            }];
            let projected = inheritance_dependency_diagnostics(&issues);
            assert_eq!(projected.len(), 1);
            assert_eq!(projected[0].spec_paths, vec![String::new(); 999]);
            assert_eq!(
                projected[0].context["cycle"],
                JsonValue::Array(vec![JsonValue::String(String::new()); 1000])
            );
        }
    }
    mod scalar_submission_rules {
        use std::collections::BTreeSet;
        use yamaa_core::{
            diagnostic::{ScalarSubmissionRule as Rule, SCALAR_SUBMISSION_RULES},
            front_end::{lowering::sections as environment, v1::project::sections},
            schema::{Document, DocumentNode as N},
            specification_submission_document as spec,
        };

        // Authored invalid scalar occurrences, independent of the decoder and schema.
        #[derive(Default)]
        struct Arena(Vec<N>);
        impl Arena {
            fn node(&mut self, value: N) -> usize {
                let node = self.0.len();
                self.0.push(value);
                node
            }
            fn text(&mut self, value: &str) -> usize {
                self.node(N::Text(value.into()))
            }
            fn map(&mut self, fields: &[(&str, usize)]) -> usize {
                let pairs = fields
                    .iter()
                    .map(|&(key, value)| (self.text(key), value))
                    .collect();
                self.node(N::Mapping(pairs))
            }
            fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
                let fields: Vec<_> = fields
                    .iter()
                    .map(|&(key, value)| (key, self.text(value)))
                    .collect();
                self.map(&fields)
            }
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }

        #[test]
        fn actual_raw_decoder_findings_reach_every_scalar_owner_and_keep_shape_uncited() {
            let mut reached = BTreeSet::new();
            let mut a = Arena::default();
            let study = a.texts(&[
                ("id", "9 bad"),
                ("name", "Study"),
                ("description", "Description"),
                ("protocol_name", "P"),
            ]);
            let guide = a.texts(&[("name", "UNKNOWN"), ("version", "3.4"), ("status", "Final")]);
            let standards = a.node(N::Sequence(vec![guide]));
            let specs = a.node(N::Sequence(vec![]));
            let define = a.node(N::Null);
            let doc = a.texts(&[
                ("id", "bad.id"),
                ("kind", "wrong"),
                ("link", "docs/../escape.pdf"),
                ("title", "Document"),
            ]);
            let documents = a.node(N::Sequence(vec![doc]));
            let language = a.text("e-123456789");
            let section = a.map(&[
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("documents", documents),
                ("xml_lang", language),
            ]);
            let root = a.map(&[("study", study), ("sdtm", section)]);
            let input = a.finish(root);
            let error = sections(&input).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let environment::Cause::Findings(findings) = error.cause else {
                panic!("raw findings")
            };
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| (finding.scalar_rule(), finding.rule()))
                    .collect::<Vec<_>>(),
                [
                    (Some(Rule::DefineId), Some("DEFINE-9")),
                    (Some(Rule::StandardName), Some("DEFINE-6")),
                    (None, None),
                    (Some(Rule::Identifier), Some("SPEC-2")),
                    (None, None),
                    (Some(Rule::Href), Some("DEFINE-13")),
                    (Some(Rule::Language), Some("DEFINE-21")),
                ]
            );
            assert_eq!(findings[2].node, define);
            assert_eq!(findings[2].kind, environment::Kind::ExpectedText);
            assert!(matches!(input.nodes()[findings[2].node], N::Null));
            assert_eq!(findings[4].kind, environment::Kind::InvalidChoice);
            assert_eq!(findings[4].origin.unwrap().container, doc);
            assert_eq!(
                findings[4].path.steps().collect::<Vec<_>>(),
                [
                    environment::Step::Field(environment::FieldName::Sdtm),
                    environment::Step::Field(environment::FieldName::Documents),
                    environment::Step::Index(0),
                    environment::Step::Field(environment::FieldName::Kind),
                ]
            );
            reached.extend(findings.iter().filter_map(|finding| finding.scalar_rule()));

            let mut a = Arena::default();
            let label = a.text("Dataset");
            let class = a.text("wrong");
            let subclass = a.text("wrong");
            let structure = a.text("One record per subject");
            let repeating = a.node(N::Boolean(false));
            let dataset = a.map(&[
                ("label", label),
                ("class", class),
                ("subclass", subclass),
                ("structure", structure),
                ("repeating", repeating),
            ]);
            let origin = a.texts(&[("type", "wrong"), ("source", "wrong")]);
            let method = a.texts(&[("type", "wrong"), ("description", "Derive")]);
            let core = a.text("wrong");
            let data_type = a.text("wrong");
            let submission = a.map(&[
                ("core", core),
                ("data_type", data_type),
                ("origin", origin),
                ("method", method),
            ]);
            let name = a.text("bad-name");
            let kind = a.text("str");
            let column = a.map(&[("name", name), ("type", kind), ("submission", submission)]);
            let null = a.node(N::Null);
            let null_submission = a.map(&[("core", null)]);
            let valid_name = a.text("B");
            let valid_kind = a.text("str");
            let second = a.map(&[
                ("name", valid_name),
                ("type", valid_kind),
                ("submission", null_submission),
            ]);
            let columns = a.node(N::Sequence(vec![column, second]));
            let root = a.map(&[("submission", dataset), ("columns", columns)]);
            let input = a.finish(root);
            let error = spec::decode(&input, root).unwrap_err();
            assert!(std::ptr::eq(error.input, &input));
            let spec::Cause::Findings(findings) = error.cause else {
                panic!("raw findings")
            };
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| (finding.scalar_rule(), finding.rule()))
                    .collect::<Vec<_>>(),
                [
                    (Some(Rule::DatasetClass), Some("SUB-4")),
                    (Some(Rule::DatasetSubclass), Some("SUB-4")),
                    (Some(Rule::Identifier), Some("SPEC-2")),
                    (Some(Rule::Core), Some("SUB-10")),
                    (Some(Rule::DataType), Some("SUB-6")),
                    (Some(Rule::OriginType), Some("SUB-14")),
                    (Some(Rule::OriginSource), Some("SUB-14")),
                    (Some(Rule::MethodType), Some("SUB-21")),
                    (None, None),
                ]
            );
            let raw = &findings[8];
            assert_eq!(raw.kind, spec::Kind::ExpectedText);
            assert_eq!(raw.node, null);
            assert_eq!(raw.container, Some(null_submission));
            assert_eq!(raw.origin.unwrap().field, spec::FieldName::Core);
            assert_eq!(
                raw.path.steps().collect::<Vec<_>>(),
                [
                    spec::Step::Field(spec::FieldName::Columns),
                    spec::Step::Index(1),
                    spec::Step::Field(spec::FieldName::Submission),
                    spec::Step::Field(spec::FieldName::Core),
                ]
            );
            reached.extend(findings.iter().filter_map(|finding| finding.scalar_rule()));
            assert_eq!(reached, SCALAR_SUBMISSION_RULES.iter().copied().collect());
        }
    }
    mod schema_bundle {
        use yamaa_core::schema::{
            BundleError, BundleIssueKind as I, Document, DocumentNode as N, SchemaAliasKind,
            SchemaModule, SchemaShape, SchemaStructure,
        };

        use crate::support::document::Tree as V;

        /// Build explicitly authored decoded schema documents without an interpreter oracle.
        fn module(
            name: &str,
            declarations: Vec<(&'static str, V)>,
            includes: &[&'static str],
        ) -> SchemaModule {
            fn push(value: V, nodes: &mut Vec<N>) -> usize {
                value.append(nodes)
            }
            let mut entries = vec![
                ("version", V::text("1.0")),
                ("includes", V::List(includes.iter().map(V::text).collect())),
            ];
            entries.extend(declarations);
            let mut nodes = Vec::new();
            let root = push(V::map(entries), &mut nodes);
            SchemaModule {
                name: name.into(),
                document: Document::new(nodes, root).unwrap(),
            }
        }

        fn descriptor(kind: &'static str) -> V {
            V::map(vec![("type", V::text(kind))])
        }
        fn reuse(name: &'static str) -> V {
            V::map(vec![("fields_from", V::text(name))])
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, descriptor(kind))])
        }
        fn kinds(error: BundleError) -> Vec<I> {
            match error {
                BundleError::Invalid(issues) => issues.into_iter().map(|i| i.kind).collect(),
                other => panic!("unexpected {other:?}"),
            }
        }

        #[test]
        fn complete_bundle_resolves_forward_names_and_preserves_reused_default_origin() {
            let shared = module(
                "schema_shared.yaml",
                vec![
                    (
                        "shared",
                        V::List(vec![
                            V::map(vec![(
                                "id",
                                V::map(vec![
                                    ("type", V::text("key")),
                                    ("required", V::boolean(true)),
                                ]),
                            )]),
                            V::map(vec![(
                                "label",
                                V::map(vec![
                                    ("type", V::text("str")),
                                    ("default", V::text("untitled")),
                                ]),
                            )]),
                        ]),
                    ),
                    ("key", descriptor("str")),
                    (
                        "recursive",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("recursive"), V::text("str")]),
                        )]),
                    ),
                ],
                &[],
            );
            let root = module(
                "schema.yaml",
                vec![("root_class", V::List(vec![reuse("shared")]))],
                &["schema_shared.yaml"],
            );
            let bundle = SchemaStructure::admit(vec![shared, root], 1, "root_class").unwrap();
            assert_eq!(bundle.version(), "1.0");
            assert_eq!(
                bundle
                    .root_class()
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["id", "label"]
            );
            let fields = &bundle.root_class().fields;
            assert!(bundle.descriptors()[fields[0].descriptor]
                .descriptor
                .required());
            let origin = &bundle.descriptors()[fields[1].descriptor];
            assert_eq!(origin.module, 0);
            assert_eq!(origin.path, "root_class.label");
            assert_eq!(
                bundle.modules()[0].document.nodes()[origin.descriptor.default_node().unwrap()],
                N::Text("untitled".into())
            );
            assert_eq!(
                bundle
                    .aliases()
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>(),
                ["key", "recursive"]
            );
        }

        #[test]
        fn registry_contributions_merge_after_complete_include_loading() {
            let root = module(
                "schema.yaml",
                vec![
                    ("root_class", V::List(vec![field("derive", "expression")])),
                    (
                        "expression",
                        V::map(vec![("registry", V::text("expressions"))]),
                    ),
                    ("expressions", V::map(vec![("source", descriptor("str"))])),
                ],
                &["schema_more.yaml"],
            );
            let more = module(
                "schema_more.yaml",
                vec![(
                    "expressions",
                    V::map(vec![
                        ("literal", descriptor("int")),
                        ("wrapped", V::List(vec![field("value", "str")])),
                    ]),
                )],
                &[],
            );
            let bundle = SchemaStructure::admit(vec![root, more], 0, "root_class").unwrap();
            assert_eq!(bundle.aliases()[0].kind, SchemaAliasKind::Registry(0));
            assert_eq!(
                bundle.registries()[0]
                    .entries
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect::<Vec<_>>(),
                ["source", "literal", "wrapped"]
            );
            assert!(
                matches!(&bundle.registries()[0].entries[2].shape,SchemaShape::Class(fields) if fields[0].name=="value")
            );
        }

        #[test]
        fn include_and_field_reuse_cycles_do_not_become_recursive_aliases() {
            let a = module(
                "schema_a.yaml",
                vec![("root_class", V::List(vec![]))],
                &["schema_b.yaml"],
            );
            let b = module("schema_b.yaml", vec![], &["schema_a.yaml"]);
            assert_eq!(
                kinds(SchemaStructure::admit(vec![a, b], 0, "root_class").unwrap_err()),
                [I::IncludeCycle(vec![
                    "schema_a.yaml".into(),
                    "schema_b.yaml".into(),
                    "schema_a.yaml".into()
                ])]
            );
            let root = module(
                "schema.yaml",
                vec![
                    ("root_class", V::List(vec![reuse("a")])),
                    ("a", V::List(vec![reuse("b")])),
                    ("b", V::List(vec![reuse("a")])),
                ],
                &[],
            );
            assert_eq!(
                kinds(SchemaStructure::admit(vec![root], 0, "root_class").unwrap_err()),
                [I::FieldsFromCycle(vec![
                    "root_class".into(),
                    "a".into(),
                    "b".into(),
                    "a".into()
                ])]
            );
        }

        #[test]
        fn duplicate_reused_fields_and_unknown_types_preserve_stage_order() {
            let root = module(
                "schema.yaml",
                vec![
                    (
                        "root_class",
                        V::List(vec![reuse("common"), field("id", "missing")]),
                    ),
                    ("common", V::List(vec![field("id", "str")])),
                    ("orphan", V::map(vec![])),
                ],
                &[],
            );
            assert_eq!(
                kinds(SchemaStructure::admit(vec![root], 0, "root_class").unwrap_err()),
                [
                    I::DuplicateField("id".into()),
                    I::EmptyRegistry,
                    I::UnreferencedRegistry,
                    I::UnknownType("missing".into())
                ]
            );
        }

        #[test]
        fn doubling_field_reuse_is_rejected_as_a_duplicate_before_expansion() {
            let mut declarations = vec![
                ("version".into(), V::text("1.0")),
                (
                    "root_class".into(),
                    V::List(vec![V::map(vec![("fields_from", V::text("level30"))])]),
                ),
                ("level0".into(), V::List(vec![field("id", "str")])),
            ];
            for level in 1..=30 {
                let inherited = V::map(vec![(
                    "fields_from",
                    V::text(format!("level{}", level - 1)),
                )]);
                declarations.push((
                    format!("level{level}"),
                    V::List(vec![inherited.clone(), inherited]),
                ));
            }
            let module = SchemaModule {
                name: "schema.yaml".into(),
                document: V::Map(declarations).document(),
            };
            let BundleError::Invalid(issues) =
                SchemaStructure::admit(vec![module], 0, "root_class").unwrap_err()
            else {
                panic!("duplicate reuse has a language finding, not a resource quota");
            };
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].module, 0);
            assert_eq!(issues[0].path, "root_class.id");
            assert_eq!(issues[0].kind, I::DuplicateField("id".into()));
        }

        #[test]
        fn malformed_fields_precede_later_duplicate_reuse_in_the_selected_class() {
            let root = module(
                "schema.yaml",
                vec![
                    (
                        "root_class",
                        V::List(vec![
                            V::map(vec![("broken", V::map(vec![]))]),
                            reuse("doubled"),
                        ]),
                    ),
                    ("doubled", V::List(vec![reuse("common"), reuse("common")])),
                    ("common", V::List(vec![field("id", "str")])),
                ],
                &[],
            );
            let BundleError::Invalid(issues) =
                SchemaStructure::admit(vec![root], 0, "root_class").unwrap_err()
            else {
                panic!("malformed declarations must keep their language findings");
            };
            assert_eq!(issues[0].path, "root_class.broken");
            assert_eq!(
                issues[0].kind,
                I::Descriptor(yamaa_core::schema::DescriptorIssue::MissingType)
            );
            assert_eq!(issues[1].path, "root_class.id");
            assert_eq!(issues[1].kind, I::DuplicateField("id".into()));
        }

        #[test]
        fn duplicate_reuse_does_not_hide_later_structural_reference_failures() {
            let root = module(
                "schema.yaml",
                vec![
                    (
                        "root_class",
                        V::List(vec![reuse("common"), reuse("common"), reuse("loop")]),
                    ),
                    ("common", V::List(vec![field("id", "str")])),
                    ("loop", V::List(vec![reuse("loop")])),
                ],
                &[],
            );
            assert_eq!(
                kinds(SchemaStructure::admit(vec![root], 0, "root_class").unwrap_err()),
                [I::FieldsFromCycle(vec![
                    "root_class".into(),
                    "loop".into(),
                    "loop".into(),
                ])]
            );
        }

        #[test]
        fn duplicate_registry_entries_and_closed_include_names_are_rejected() {
            let root = module(
                "schema.yaml",
                vec![
                    ("root_class", V::List(vec![])),
                    ("ops", V::map(vec![("one", descriptor("str"))])),
                ],
                &["schema_more.yaml"],
            );
            let more = module(
                "schema_more.yaml",
                vec![("ops", V::map(vec![("one", descriptor("int"))]))],
                &[],
            );
            assert_eq!(
                kinds(SchemaStructure::admit(vec![root, more], 0, "root_class").unwrap_err()),
                [I::DuplicateRegistryEntry("one".into())]
            );
            for include in [
                "../schema_x.yaml",
                "schema_.yaml",
                "schema_X.yaml",
                "https://host/schema_x.yaml",
                "/schema_x.yaml",
            ] {
                let root = module(
                    "schema.yaml",
                    vec![("root_class", V::List(vec![]))],
                    &[include],
                );
                assert!(matches!(
                    kinds(SchemaStructure::admit(vec![root], 0, "root_class").unwrap_err())
                        .as_slice(),
                    [I::UnsafeInclude { .. }]
                ));
            }
            let root = module(
                "schema.yaml",
                vec![("root_class", V::List(vec![]))],
                &["schema_missing.yaml"],
            );
            assert_eq!(
                kinds(SchemaStructure::admit(vec![root], 0, "root_class").unwrap_err()),
                [I::MissingInclude("schema_missing.yaml".into())]
            );
        }

        #[test]
        fn independent_bundle_admissions_keep_reused_fields() {
            let root = module(
                "schema.yaml",
                vec![
                    ("root_class", V::List(vec![reuse("common")])),
                    ("common", V::List(vec![field("id", "str")])),
                ],
                &[],
            );
            for _ in 0..3 {
                let schema = SchemaStructure::admit(vec![root.clone()], 0, "root_class").unwrap();
                assert_eq!(schema.root_class().fields.len(), 1);
                assert_eq!(schema.root_class().fields[0].name, "id");
            }
        }

        #[test]
        fn cross_module_registry_duplicates_use_exact_decoded_scalar_key_equality() {
            for (left, right) in [
                (V::boolean(true), V::integer("1")),
                (V::integer("1"), V::float(1.0)),
                (V::float(-0.0), V::integer("0")),
                (V::integer("9007199254740992"), V::float(9007199254740992.0)),
            ] {
                let root = module(
                    "schema.yaml",
                    vec![
                        ("root_class", V::List(vec![])),
                        ("ops", V::Pairs(vec![(left, descriptor("str"))])),
                    ],
                    &["schema_more.yaml"],
                );
                let more = module(
                    "schema_more.yaml",
                    vec![("ops", V::Pairs(vec![(right, descriptor("str"))]))],
                    &[],
                );
                assert_eq!(
                    kinds(SchemaStructure::admit(vec![root, more], 0, "root_class").unwrap_err()),
                    [I::DuplicateRegistryEntry("<non-text>".into())]
                );
            }
            let root = module(
                "schema.yaml",
                vec![
                    ("root_class", V::List(vec![])),
                    (
                        "ops",
                        V::Pairs(vec![(V::integer("9007199254740993"), descriptor("str"))]),
                    ),
                ],
                &["schema_more.yaml"],
            );
            let more = module(
                "schema_more.yaml",
                vec![(
                    "ops",
                    V::Pairs(vec![(V::float(9007199254740992.0), descriptor("str"))]),
                )],
                &[],
            );
            let result =
                kinds(SchemaStructure::admit(vec![root, more], 0, "root_class").unwrap_err());
            assert!(!result
                .iter()
                .any(|kind| matches!(kind, I::DuplicateRegistryEntry(_))));
            assert!(result.contains(&I::RegistryEntryName));
        }
    }
    mod schema_composition {
        //! The normalized layers of a 1.x specification compose by
        //! `compose_versioned_layers`, root ancestor first, and the resolved
        //! specification's normalization materializes defaults once (LAYER-5
        //! to LAYER-9).
        use yamaa_core::language::LanguageVersion;
        use yamaa_core::schema::{
            ComposedLayers, Document, DocumentNode as N, NormalizedDocument, SchemaModule,
            SchemaOrigin, SchemaSource, SchemaStructure,
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, V::map(vec![("type", V::text(kind))]))])
        }
        fn required(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(
                name,
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(true)),
                ]),
            )])
        }
        fn schema() -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("value", "patch"),
                                required("yamaa", "str"),
                                field("type", "str"),
                                field("parent", "str"),
                                field("input", "dict[str, dataset_class]"),
                                field("columns", "list[column_class]"),
                                field("rows", "list[row_class]"),
                                field("intermediates", "list[intermediate_class]"),
                                field("windows", "dict[str, window_class]"),
                                field("metadata", "dict[str, str]"),
                                field("literal", "literal"),
                                field("keys", "list[str]"),
                                required("output", "dict"),
                            ]),
                        ),
                        (
                            "dataset_class",
                            V::List(vec![
                                field("path", "str"),
                                field("types", "dict[str, str]"),
                                field("missing", "literal"),
                            ]),
                        ),
                        (
                            "column_class",
                            V::List(vec![
                                required("name", "str"),
                                required("type", "str"),
                                field("label", "str"),
                                field("missing", "literal"),
                                field("mandatory", "required_options"),
                                field("options", "options_class"),
                                field("metadata", "dict[str, str]"),
                                field("items", "list[str]"),
                                // A 1.x member merges recursively, so a
                                // column carries every kind a patch holds.
                                field("key", "match_key"),
                                field("expression", "expression"),
                                field("window", "window_selection"),
                            ]),
                        ),
                        (
                            "row_class",
                            V::List(vec![
                                required("id", "str"),
                                field("options", "options_class"),
                                field("missing", "literal"),
                            ]),
                        ),
                        (
                            "intermediate_class",
                            V::List(vec![
                                required("id", "str"),
                                field("options", "options_class"),
                                field("no_match", "literal"),
                            ]),
                        ),
                        (
                            "patch",
                            V::List(vec![
                                field("label", "str"),
                                field("metadata", "dict[str, str]"),
                                field("options", "options_class"),
                                field("items", "list[str]"),
                                field("key", "match_key"),
                                field("expression", "expression"),
                                field("window", "window_selection"),
                            ]),
                        ),
                        (
                            "options_class",
                            V::List(vec![
                                field("left", "str"),
                                field("right", "str"),
                                V::map(vec![(
                                    "defaulted",
                                    V::map(vec![
                                        ("type", V::text("str")),
                                        ("default", V::text("DEFAULT")),
                                    ]),
                                )]),
                            ]),
                        ),
                        ("required_options", V::List(vec![required("one", "str")])),
                        (
                            "match_key",
                            V::map(vec![("type", V::text("dict[str, str]"))]),
                        ),
                        (
                            "literal",
                            V::map(vec![(
                                "type",
                                V::List(vec![V::text("str"), V::text("null")]),
                            )]),
                        ),
                        (
                            "expression",
                            V::map(vec![("registry", V::text("operations"))]),
                        ),
                        (
                            "operations",
                            V::map(vec![
                                (
                                    "select",
                                    V::List(vec![field("left", "str"), field("right", "str")]),
                                ),
                                (
                                    "literal",
                                    V::map(vec![(
                                        "type",
                                        V::List(vec![V::text("str"), V::text("null")]),
                                    )]),
                                ),
                            ]),
                        ),
                        (
                            "window_selection",
                            V::map(vec![(
                                "type",
                                V::List(vec![V::text("str"), V::text("window_class")]),
                            )]),
                        ),
                        (
                            "window_class",
                            V::List(vec![
                                field("group_by", "list[str]"),
                                field("order_by", "list[str]"),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        fn check(actual: &Document, expected: V) {
            fn same(a: &Document, x: usize, b: &Document, y: usize) {
                match (&a.nodes()[x], &b.nodes()[y]) {
                    (N::Mapping(xs), N::Mapping(ys)) => {
                        assert_eq!(xs.len(), ys.len());
                        for (&(ak, av), &(bk, bv)) in xs.iter().zip(ys) {
                            same(a, ak, b, bk);
                            same(a, av, b, bv);
                        }
                    }
                    (N::Sequence(xs), N::Sequence(ys)) => {
                        assert_eq!(xs.len(), ys.len());
                        for (&x, &y) in xs.iter().zip(ys) {
                            same(a, x, b, y);
                        }
                    }
                    (x, y) => assert_eq!(x, y),
                }
            }
            let expected = document(expected);
            same(actual, actual.root(), &expected, expected.root());
        }
        /// The schema at language version 1.0, which every 1.x layer here
        /// declares.
        fn versioned() -> SchemaStructure {
            schema().at_version(LanguageVersion::new(0))
        }
        /// A 1.x layer as `normalize_versioned_layer` leaves it, each node
        /// its own written occurrence.
        fn normalized(value: V) -> NormalizedDocument {
            let document = document(value);
            let origins = (0..document.nodes().len())
                .map(|node| SchemaOrigin {
                    source: SchemaSource::Input,
                    node,
                    generated: false,
                })
                .collect();
            NormalizedDocument { document, origins }
        }
        fn compose_versioned(layers: Vec<V>) -> ComposedLayers {
            let layers = layers.into_iter().map(normalized).collect::<Vec<_>>();
            versioned().compose_versioned_layers(&layers).unwrap()
        }
        /// The column `A` holding `patch`'s fields, a keyed member whose
        /// fields compose recursively (LAYER-6).
        fn member(patch: V) -> V {
            let V::Map(mut fields) = patch else {
                panic!("a patch is a mapping")
            };
            fields.insert(0, ("name".into(), V::text("A")));
            V::Map(fields)
        }
        fn columns(patch: V) -> V {
            V::map(vec![("columns", V::List(vec![member(patch)]))])
        }
        /// Two layers that each write the column `A`, composed.
        fn compose(old: V, new: V) -> ComposedLayers {
            compose_versioned(vec![columns(old), columns(new)])
        }
        fn check_member(actual: &Document, patch: V) {
            check(actual, columns(patch));
        }

        #[test]
        fn recursive_patches_keep_positions_merge_mappings_replace_lists_and_defer_defaults() {
            let value = compose(
                V::map(vec![
                    ("label", V::text("parent")),
                    (
                        "metadata",
                        V::map(vec![("a", V::text("A")), ("b", V::text("B"))]),
                    ),
                    ("options", V::map(vec![("left", V::text("L"))])),
                    ("items", V::List(vec![V::text("one"), V::text("two")])),
                    (
                        "key",
                        V::map(vec![("a", V::text("A")), ("b", V::text("B"))]),
                    ),
                ]),
                V::map(vec![
                    ("options", V::map(vec![("right", V::text("R"))])),
                    (
                        "metadata",
                        V::map(vec![("a", V::text("new")), ("c", V::text("C"))]),
                    ),
                    ("items", V::List(vec![V::text("three")])),
                    ("key", V::map(vec![("b", V::text("new B"))])),
                ]),
            );
            check_member(
                &value.document,
                V::map(vec![
                    ("label", V::text("parent")),
                    (
                        "metadata",
                        V::map(vec![
                            ("a", V::text("new")),
                            ("b", V::text("B")),
                            ("c", V::text("C")),
                        ]),
                    ),
                    (
                        "options",
                        V::map(vec![("left", V::text("L")), ("right", V::text("R"))]),
                    ),
                    ("items", V::List(vec![V::text("three")])),
                    (
                        "key",
                        V::map(vec![("a", V::text("A")), ("b", V::text("new B"))]),
                    ),
                ]),
            );
        }
        #[test]
        fn layers_keep_keyed_order_replace_ordinary_fields_and_track_written_provenance() {
            let layers = vec![
                normalized(V::map(vec![
                    ("yamaa", V::text("1.0")),
                    ("type", V::text("spec")),
                    ("parent", V::text("parent.yaml")),
                    ("keys", V::List(vec![V::text("A")])),
                    ("metadata", V::map(vec![("old", V::text("old"))])),
                    (
                        "input",
                        V::map(vec![(
                            "DS",
                            V::map(vec![
                                ("path", V::text("parent.csv")),
                                ("types", V::map(vec![("A", V::text("int"))])),
                            ]),
                        )]),
                    ),
                    (
                        "columns",
                        V::List(vec![
                            V::map(vec![
                                ("name", V::text("A")),
                                ("options", V::map(vec![("left", V::text("L"))])),
                            ]),
                            V::map(vec![("name", V::text("B")), ("label", V::text("before"))]),
                        ]),
                    ),
                    (
                        "windows",
                        V::map(vec![
                            (
                                "W",
                                V::map(vec![
                                    ("group_by", V::List(vec![V::text("ID")])),
                                    ("order_by", V::List(vec![V::text("SEQ")])),
                                ]),
                            ),
                            (
                                "KEEP",
                                V::map(vec![("order_by", V::List(vec![V::text("K")]))]),
                            ),
                        ]),
                    ),
                    (
                        "rows",
                        V::List(vec![V::map(vec![
                            ("id", V::text("R")),
                            ("options", V::map(vec![("left", V::text("old"))])),
                        ])]),
                    ),
                ])),
                normalized(V::map(vec![
                    ("metadata", V::map(vec![("new", V::text("new"))])),
                    (
                        "input",
                        V::map(vec![(
                            "DS",
                            V::map(vec![("types", V::map(vec![("B", V::text("str"))]))]),
                        )]),
                    ),
                    (
                        "columns",
                        V::List(vec![
                            V::map(vec![("name", V::text("B")), ("label", V::text("after"))]),
                            V::map(vec![
                                ("name", V::text("A")),
                                ("options", V::map(vec![("right", V::text("R"))])),
                            ]),
                        ]),
                    ),
                    (
                        "windows",
                        V::map(vec![(
                            "W",
                            V::map(vec![("order_by", V::List(vec![V::text("NEW")]))]),
                        )]),
                    ),
                    (
                        "rows",
                        V::List(vec![V::map(vec![
                            ("id", V::text("R")),
                            ("options", V::map(vec![("right", V::text("only"))])),
                        ])]),
                    ),
                ])),
                // A 1.x layer clears an inherited field by listing it
                // (LAYER-9, D18).
                normalized(V::map(vec![
                    ("clear", V::List(vec![V::text("keys")])),
                    (
                        "columns",
                        V::List(vec![V::map(vec![
                            ("name", V::text("C")),
                            ("label", V::text("new")),
                        ])]),
                    ),
                ])),
            ];
            let original = layers.clone();
            let result = versioned().compose_versioned_layers(&layers).unwrap();
            assert_eq!(layers, original);
            check(
                &result.document,
                V::map(vec![
                    ("yamaa", V::text("1.0")),
                    ("type", V::text("spec")),
                    ("metadata", V::map(vec![("new", V::text("new"))])),
                    (
                        "input",
                        V::map(vec![(
                            "DS",
                            V::map(vec![
                                ("path", V::text("parent.csv")),
                                (
                                    "types",
                                    V::map(vec![("A", V::text("int")), ("B", V::text("str"))]),
                                ),
                            ]),
                        )]),
                    ),
                    (
                        "columns",
                        V::List(vec![
                            V::map(vec![
                                ("name", V::text("A")),
                                (
                                    "options",
                                    V::map(vec![("left", V::text("L")), ("right", V::text("R"))]),
                                ),
                            ]),
                            V::map(vec![("name", V::text("B")), ("label", V::text("after"))]),
                            V::map(vec![("name", V::text("C")), ("label", V::text("new"))]),
                        ]),
                    ),
                    (
                        "windows",
                        V::map(vec![
                            (
                                "W",
                                V::map(vec![
                                    ("group_by", V::List(vec![V::text("ID")])),
                                    ("order_by", V::List(vec![V::text("NEW")])),
                                ]),
                            ),
                            (
                                "KEEP",
                                V::map(vec![("order_by", V::List(vec![V::text("K")]))]),
                            ),
                        ]),
                    ),
                    (
                        "rows",
                        V::List(vec![V::map(vec![
                            ("id", V::text("R")),
                            (
                                "options",
                                V::map(vec![("left", V::text("old")), ("right", V::text("only"))]),
                            ),
                        ])]),
                    ),
                ]),
            );
            for (path, layer) in [
                ("columns.A", 0),
                ("columns.A.name", 1),
                ("columns.A.options.left", 0),
                ("columns.A.options.right", 1),
                ("columns.B.label", 1),
                ("columns.C", 2),
                ("windows.W", 0),
                ("windows.W.group_by", 0),
                ("windows.W.order_by", 1),
                ("windows.KEEP", 0),
                ("rows.R.options.left", 0),
                ("rows.R.options.right", 1),
                ("input.DS.path", 0),
                ("input.DS.types.A", 0),
                ("input.DS.types.B", 1),
                ("metadata", 1),
            ] {
                assert_eq!(
                    result
                        .provenance
                        .iter()
                        .find(|p| p.path == path)
                        .unwrap()
                        .layer,
                    layer,
                    "{path}"
                );
            }
            assert!(!result.provenance.iter().any(|p| p.path == "keys"
                || p.path == "metadata.new"
                || p.path == "parent"
                || p.path == "clear"));
        }

        #[test]
        fn malformed_collection_shapes_and_duplicate_identifiers_are_refused() {
            let schema = versioned();
            for value in [
                V::map(vec![("columns", V::text("not a sequence"))]),
                V::map(vec![(
                    "input",
                    V::map(vec![("DS", V::text("not normalized"))]),
                )]),
                V::map(vec![(
                    "columns",
                    V::List(vec![
                        V::map(vec![("name", V::text("A"))]),
                        V::map(vec![("name", V::text("A"))]),
                    ]),
                )]),
                V::map(vec![(
                    "columns",
                    V::List(vec![V::map(vec![("label", V::text("no identity"))])]),
                )]),
            ] {
                assert!(schema
                    .compose_versioned_layers(&[normalized(value)])
                    .is_err());
            }
        }

        #[test]
        fn independent_layer_requests_keep_the_same_version_and_occurrences() {
            let layers = [normalized(V::map(vec![("yamaa", V::text("1.0"))]))];
            let schema = versioned();
            let first = schema.compose_versioned_layers(&layers).unwrap();
            let second = schema.compose_versioned_layers(&layers).unwrap();
            assert_eq!(first, second);
        }

        #[test]
        fn complete_composition_materializes_defaults_once_and_keeps_written_provenance() {
            // The layers write every field the resolved specification
            // requires, since requiredness applies to it (LAYER-7).
            let result = compose_versioned(vec![
                V::map(vec![
                    ("yamaa", V::text("1.0")),
                    ("type", V::text("spec")),
                    ("output", V::map(vec![])),
                    (
                        "columns",
                        V::List(vec![V::map(vec![
                            ("name", V::text("A")),
                            ("type", V::text("int")),
                            ("options", V::map(vec![("defaulted", V::text("AUTHORED"))])),
                        ])]),
                    ),
                ]),
                V::map(vec![(
                    "columns",
                    V::List(vec![
                        V::map(vec![
                            ("name", V::text("A")),
                            ("options", V::map(vec![("left", V::text("L"))])),
                        ]),
                        V::map(vec![
                            ("name", V::text("B")),
                            ("type", V::text("str")),
                            ("options", V::map(vec![])),
                        ]),
                    ]),
                )]),
                V::map(vec![(
                    "columns",
                    V::List(vec![V::map(vec![
                        ("name", V::text("A")),
                        ("options", V::map(vec![("right", V::text("R"))])),
                    ])]),
                )]),
            ]);
            // Composition materializes no default; the resolved
            // specification's normalization does, once, in schema order.
            let resolved = versioned()
                .normalize_resolved_root(&result.document)
                .unwrap();
            check(
                &resolved.document,
                V::map(vec![
                    ("yamaa", V::text("1.0")),
                    ("type", V::text("spec")),
                    (
                        "columns",
                        V::List(vec![
                            V::map(vec![
                                ("name", V::text("A")),
                                ("type", V::text("int")),
                                (
                                    "options",
                                    V::map(vec![
                                        ("left", V::text("L")),
                                        ("right", V::text("R")),
                                        ("defaulted", V::text("AUTHORED")),
                                    ]),
                                ),
                            ]),
                            V::map(vec![
                                ("name", V::text("B")),
                                ("type", V::text("str")),
                                ("options", V::map(vec![("defaulted", V::text("DEFAULT"))])),
                            ]),
                        ]),
                    ),
                    ("output", V::map(vec![])),
                ]),
            );
            assert_eq!(
                result
                    .provenance
                    .iter()
                    .find(|p| p.path == "columns.A.options.defaulted")
                    .unwrap()
                    .layer,
                0
            );
            assert!(!result
                .provenance
                .iter()
                .any(|p| p.path == "columns.B.options.defaulted"));
        }
    }
    mod schema_descriptor {
        use yamaa_core::portable_regex::{CompileError, Resource};
        use yamaa_core::schema::ConstraintViolation as C;
        use yamaa_core::schema::{
            Descriptor, DescriptorError, DescriptorIssue as I, DescriptorSite, Document,
            DocumentNode as N,
        };

        use crate::support::document::Tree as V;

        /// Author decoded documents directly; neither a schema interpreter nor a parser supplies truth.
        fn document(value: V) -> Document {
            value.document()
        }

        /// A class field descriptor, or else a named value type's.
        fn admit(value: V, class_field: bool) -> yamaa_core::schema::DescriptorReport {
            let site = if class_field {
                DescriptorSite::ClassField
            } else {
                DescriptorSite::NamedType
            };
            admit_at(value, site)
        }

        fn admit_at(value: V, site: DescriptorSite) -> yamaa_core::schema::DescriptorReport {
            let d = document(value);
            Descriptor::admit(&d, d.root(), site).unwrap()
        }

        #[test]
        fn a_named_type_declares_a_kind_and_a_class_field_the_version_that_introduced_it() {
            let report = admit_at(
                V::map(vec![
                    ("type", V::text("str")),
                    ("kind", V::text("identifier")),
                ]),
                DescriptorSite::NamedType,
            );
            assert!(report.issues().is_empty());
            assert_eq!(report.into_descriptor().unwrap().kind(), Some("identifier"));
            for kind in yamaa_core::schema::FIELD_KINDS {
                let report = admit_at(
                    V::map(vec![("type", V::text("str")), ("kind", V::text(kind))]),
                    DescriptorSite::NamedType,
                );
                assert_eq!(report.into_descriptor().unwrap().kind(), Some(kind));
            }
            for kind in [V::text("label"), V::text("Identifier"), V::boolean(true)] {
                let report = admit_at(
                    V::map(vec![("type", V::text("str")), ("kind", kind)]),
                    DescriptorSite::NamedType,
                );
                assert_eq!(report.issues(), [I::InvalidKind]);
            }
            let report = admit_at(
                V::map(vec![("type", V::text("str")), ("since", V::text("1.1"))]),
                DescriptorSite::ClassField,
            );
            assert!(report.issues().is_empty());
            assert_eq!(
                report.into_descriptor().unwrap().since(),
                Some(yamaa_core::language::LanguageVersion::new(1))
            );
            for since in [
                V::text("1.01"),
                V::text("2"),
                V::float(1.1),
                V::integer("1"),
            ] {
                let report = admit_at(
                    V::map(vec![("type", V::text("str")), ("since", since)]),
                    DescriptorSite::ClassField,
                );
                assert_eq!(report.issues(), [I::InvalidSince]);
            }
            // `kind` belongs to a named value type and `since` to a class field
            // alone; a registry entry's payload declares neither.
            for (keyword, value, site) in [
                ("kind", "name", DescriptorSite::ClassField),
                ("kind", "name", DescriptorSite::RegistryEntry),
                ("since", "1.0", DescriptorSite::NamedType),
                ("since", "1.0", DescriptorSite::RegistryEntry),
            ] {
                let report = admit_at(
                    V::map(vec![(keyword, V::text(value)), ("type", V::text("str"))]),
                    site,
                );
                assert_eq!(report.issues(), [I::UnknownKeyword { key: 0 }], "{keyword}");
            }
        }

        #[test]
        fn descriptor_union_references_and_explicit_null_default_retain_written_order() {
            let d = document(V::map(vec![
                (
                    "type",
                    V::List(vec![
                        V::text(" recursive "),
                        V::text("dict[str, list[other]]"),
                        V::text("recursive"),
                    ]),
                ),
                ("required", V::boolean(false)),
                ("description", V::text("A field")),
                ("default", V::Scalar(N::Null)),
            ]));
            let report = Descriptor::admit(&d, d.root(), DescriptorSite::ClassField).unwrap();
            assert!(report.issues().is_empty());
            assert_eq!(
                report.references(),
                ["recursive", "str", "other", "recursive"]
            );
            let descriptor = report.into_descriptor().unwrap();
            assert!(!descriptor.required());
            assert_eq!(descriptor.members().len(), 3);
            assert_eq!(d.nodes()[descriptor.default_node().unwrap()], N::Null);
        }

        #[test]
        fn findings_follow_keyword_type_required_description_constraint_order() {
            let report = admit(
                V::map(vec![
                    ("mystery", V::Scalar(N::Null)),
                    ("values", V::boolean(true)),
                    ("type", V::text("int")),
                    ("size", V::integer("-1")),
                    ("required", V::boolean(true)),
                    ("default", V::Scalar(N::Null)),
                    ("description", V::text(" \t")),
                    ("pattern", V::boolean(false)),
                    ("min_length", V::boolean(false)),
                ]),
                true,
            );
            assert_eq!(
                report.issues(),
                [
                    I::UnknownKeyword { key: 0 },
                    I::RequiredDefault,
                    I::Description,
                    I::PatternRequiresString,
                    I::PatternText,
                    I::MinimumRequiresString,
                    I::MinimumNonnegative,
                    I::SizeRequiresCollection,
                    I::SizeNonnegative,
                    I::ValuesRequiresString,
                    I::ValuesTextSequence,
                ]
            );
            assert_eq!(report.references(), ["int"]);
            assert!(report.into_descriptor().is_none());

            let report = admit(V::map(vec![("required", V::boolean(false))]), false);
            assert_eq!(
                report.issues(),
                [I::UnknownKeyword { key: 0 }, I::MissingType]
            );
            assert_eq!(
                admit(V::Scalar(N::Null), false).issues(),
                [I::ExpectedMapping]
            );
        }

        #[test]
        fn invalid_types_and_required_flags_do_not_become_boolean_integer_coercions() {
            for value in [
                V::Scalar(N::Null),
                V::boolean(true),
                V::List(vec![]),
                V::List(vec![V::text("str"), V::boolean(true)]),
            ] {
                assert_eq!(
                    admit(V::map(vec![("type", value)]), false).issues(),
                    [I::InvalidTypeValue]
                );
            }
            assert_eq!(
                admit(
                    V::map(vec![
                        ("type", V::text("str")),
                        ("required", V::integer("1"))
                    ]),
                    true
                )
                .issues(),
                [I::RequiredBoolean]
            );
            let report = admit(
                V::map(vec![(
                    "type",
                    V::List(vec![V::text("missing"), V::text("list[]"), V::text("str")]),
                )]),
                false,
            );
            assert_eq!(report.references(), ["missing", "str"]);
            assert_eq!(
                report.issues(),
                [I::TypeSyntax {
                    member: "list[]".into(),
                    byte: 5
                }]
            );
        }

        #[test]
        fn constraints_retain_empty_choices_unicode_lengths_and_arbitrary_width_bounds() {
            let report = admit(
                V::map(vec![
                    ("type", V::text("str")),
                    ("values", V::List(vec![])),
                    ("min_length", V::integer("3")),
                    ("pattern", V::text("a|bc")),
                ]),
                false,
            );
            assert!(report.issues().is_empty());
            let descriptor = report.into_descriptor().unwrap();
            assert_eq!(descriptor.permitted(), Some([].as_slice()));
            assert!(!descriptor.length_meets_minimum(2));
            assert!(descriptor.length_meets_minimum(3));
            let (text, pattern) = descriptor.pattern().unwrap();
            assert_eq!(text, "a|bc");
            for (value, expected) in [("a", true), ("bc", true), ("abc", false), ("bc\n", false)] {
                assert_eq!(pattern.is_full_match(value), expected);
            }
            let descriptor = admit(
                V::map(vec![
                    ("type", V::text("list[str]")),
                    ("size", V::integer("18446744073709551616")),
                ]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert_eq!(descriptor.size(), Some("18446744073709551616"));
            assert!(!descriptor.length_matches_size(0));
            assert!(!descriptor.length_matches_size(usize::MAX));
            let descriptor = admit(
                V::map(vec![
                    ("type", V::text("str")),
                    ("min_length", V::integer("18446744073709551616")),
                ]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert!(!descriptor.length_meets_minimum(usize::MAX));
        }

        #[test]
        fn invalid_pattern_is_a_finding_but_resource_refusal_is_not() {
            let report = admit(
                V::map(vec![("type", V::text("str")), ("pattern", V::text("["))]),
                false,
            );
            assert!(
                matches!(report.issues(), [I::InvalidPattern { pattern, .. }] if pattern == "[")
            );
            let repaired_by_wrapper = admit(
                V::map(vec![
                    ("type", V::text("str")),
                    ("pattern", V::text("a)|(?:b")),
                ]),
                false,
            );
            assert!(
                matches!(repaired_by_wrapper.issues(), [I::InvalidPattern { pattern, .. }] if pattern == "a)|(?:b")
            );
            let pattern = format!("{}a{}", "(".repeat(129), ")".repeat(129));
            let d = document(V::map(vec![
                ("type", V::text("str")),
                ("pattern", V::text(pattern)),
            ]));
            assert!(matches!(
                Descriptor::admit(&d, d.root(), DescriptorSite::NamedType),
                Err(DescriptorError::Regex(CompileError::Limit {
                    resource: Resource::Depth,
                    limit: 128
                }))
            ));
        }

        #[test]
        fn repeated_invalid_descriptors_keep_the_same_authored_pattern_finding() {
            let d = document(V::map(vec![
                ("type", V::text("str")),
                ("pattern", V::text("[")),
            ]));
            for _ in 0..3 {
                assert!(
                    matches!(Descriptor::admit(&d,d.root(),DescriptorSite::NamedType).unwrap().issues(),[I::InvalidPattern{pattern,..}] if pattern=="[")
                );
            }
        }

        #[test]
        fn independent_descriptor_pattern_admissions_keep_exact_pattern_metadata() {
            let pattern = format!("(a){}b", "(a|aa)".repeat(8));
            let d = document(V::map(vec![
                ("type", V::text("str")),
                ("pattern", V::text(pattern.clone())),
            ]));
            for _ in 0..3 {
                let descriptor = Descriptor::admit(&d, d.root(), DescriptorSite::NamedType)
                    .unwrap()
                    .into_descriptor()
                    .unwrap();
                assert_eq!(descriptor.pattern().unwrap().0, pattern);
            }
        }

        #[test]
        fn repeated_empty_metadata_and_invalid_description_remain_complete() {
            let d = document(V::map(vec![
                ("type", V::text("str")),
                (
                    "values",
                    V::List(vec![V::text(""), V::text(""), V::text("")]),
                ),
            ]));
            for _ in 0..2 {
                let descriptor = Descriptor::admit(&d, d.root(), DescriptorSite::NamedType)
                    .unwrap()
                    .into_descriptor()
                    .unwrap();
                assert_eq!(descriptor.permitted().unwrap(), ["", "", ""]);
            }
            let d = document(V::map(vec![
                ("type", V::text("str")),
                ("description", V::text("    ")),
            ]));
            for _ in 0..2 {
                assert_eq!(
                    Descriptor::admit(&d, d.root(), DescriptorSite::NamedType)
                        .unwrap()
                        .issues(),
                    [I::Description]
                );
            }
        }

        #[test]
        fn value_constraints_report_every_failure_in_declared_contract_order() {
            let descriptor = admit(
                V::map(vec![
                    ("type", V::text("str")),
                    ("min_length", V::integer("2")),
                    ("pattern", V::text("a+")),
                    ("values", V::List(vec![V::text("aa")])),
                ]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert_eq!(
                descriptor.check_constraints(&N::Text("é".into())),
                [C::ValueNotPermitted, C::PatternMismatch, C::MinimumLength]
            );
            assert!(descriptor
                .check_constraints(&N::Text("aa".into()))
                .is_empty());
            let descriptor = admit(
                V::map(vec![
                    ("type", V::text("str")),
                    ("min_length", V::integer("2")),
                ]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert!(descriptor
                .check_constraints(&N::Text("e\u{301}".into()))
                .is_empty());
            assert_eq!(
                descriptor.check_constraints(&N::Text("😀".into())),
                [C::MinimumLength]
            );
            let descriptor = admit(
                V::map(vec![("type", V::text("list")), ("size", V::integer("0"))]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert!(descriptor
                .check_constraints(&N::Sequence(vec![]))
                .is_empty());
            assert_eq!(
                descriptor.check_constraints(&N::Sequence(vec![0])),
                [C::InvalidSize]
            );
        }

        #[test]
        fn long_values_are_checked_against_the_pattern_without_a_work_budget() {
            let descriptor = admit(
                V::map(vec![
                    ("type", V::text("str")),
                    ("values", V::List(vec![V::text("a")])),
                ]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert!(descriptor
                .check_constraints(&N::Text("a".into()))
                .is_empty());
            let descriptor = admit(
                V::map(vec![("type", V::text("str")), ("pattern", V::text("a+"))]),
                false,
            )
            .into_descriptor()
            .unwrap();
            assert!(descriptor
                .check_constraints(&N::Text("a".into()))
                .is_empty());
            // A million scalars, which the retired step budget refused.
            assert!(descriptor
                .check_constraints(&N::Text("a".repeat(1_000_000)))
                .is_empty());
            assert_eq!(
                descriptor.check_constraints(&N::Text("a".repeat(1_000_000) + "b")),
                [C::PatternMismatch]
            );
            assert!(descriptor
                .check_constraints(&N::Text("a".into()))
                .is_empty());
        }
    }
    mod schema_diagnostic_projection {
        use yamaa_core::schema::{
            admit_schema_diagnostics, schema_diagnostic_kind_label, Document, DocumentKind as K,
            DocumentNode as N, SchemaContext as C, SchemaDiagnostic, SchemaDiagnosticError as E,
            SchemaDiagnosticReport, SchemaDiagnosticValue as V, SchemaModule, SchemaSource,
            SchemaStructure,
        };

        const BIG: &str = "922337203685477580812345678901234567890";
        use crate::support::document::Tree as Authored;

        fn document(value: Authored) -> Document {
            value.document()
        }
        fn schema() -> SchemaStructure {
            let shared = SchemaModule {
                name: "schema_shared.yaml".into(),
                document: document(Authored::map(vec![
                    ("version", Authored::text("1.0")),
                    (
                        "shared",
                        Authored::List(vec![Authored::map(vec![(
                            "value",
                            Authored::map(vec![
                                ("type", Authored::text("str")),
                                (
                                    "values",
                                    Authored::List(vec![
                                        Authored::text("z"),
                                        Authored::text("a"),
                                        Authored::text("z"),
                                    ]),
                                ),
                                ("pattern", Authored::text("^OK$")),
                                ("min_length", Authored::integer(BIG)),
                                ("default", Authored::text("bad\"\nλ")),
                            ]),
                        )])]),
                    ),
                ])),
            };
            let root = SchemaModule {
                name: "schema.yaml".into(),
                document: document(Authored::map(vec![
                    ("version", Authored::text("1.0")),
                    (
                        "includes",
                        Authored::List(vec![Authored::text("schema_shared.yaml")]),
                    ),
                    (
                        "root_class",
                        Authored::List(vec![
                            Authored::map(vec![("fields_from", Authored::text("shared"))]),
                            Authored::map(vec![(
                                "items",
                                Authored::map(vec![
                                    ("type", Authored::text("list[int]")),
                                    ("size", Authored::integer(BIG)),
                                ]),
                            )]),
                            Authored::map(vec![(
                                "required",
                                Authored::map(vec![
                                    ("type", Authored::text("bool")),
                                    ("required", Authored::boolean(true)),
                                ]),
                            )]),
                        ]),
                    ),
                ])),
            };
            SchemaStructure::admit(vec![shared, root], 1, "root_class").unwrap()
        }
        fn issue(path: &str, context: Vec<(&'static str, C)>) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: path.into(),
                code: yamaa_core::diagnostic::ConditionCode::SchemaDescriptorPatternMismatch,
                context,
            }
        }
        type Observed<'a> = (
            &'static str,
            &'a str,
            &'static str,
            Option<&'static str>,
            Vec<(&'a str, V<'a>)>,
        );
        fn records<'a>(report: &'a SchemaDiagnosticReport<'a>) -> Vec<Observed<'a>> {
            report
                .iter()
                .map(|finding| {
                    (
                        finding.phase,
                        finding.path,
                        finding.condition,
                        finding.rule,
                        finding.context.collect(),
                    )
                })
                .collect()
        }

        #[test]
        fn actual_constraints_preserve_whole_ordered_report_and_reused_descriptor_facts() {
            let schema = schema();
            let descriptor = schema.root_class().fields[0].descriptor;
            let located = &schema.descriptors()[descriptor];
            assert_eq!(located.module, 0);
            assert_eq!(located.path, "root_class.value");
            let input = document(Authored::text("bad\"\nλ"));
            let findings = schema
                .validate_descriptor(descriptor, &input, input.root(), "value", false)
                .unwrap();
            let origins = [("source", "held/parent.yaml"), ("entry", "held/entry.yaml")];
            let report =
                admit_schema_diagnostics(&schema, Some(&input), &findings, &origins).unwrap();
            assert!(std::ptr::eq(report.schema(), &schema));
            assert!(std::ptr::eq(report.document().unwrap(), &input));
            assert_eq!(report.findings().as_ptr(), findings.as_ptr());
            assert_eq!(report.source_context().as_ptr(), origins.as_ptr());
            assert_eq!(
                records(&report),
                vec![
                    (
                        "validation",
                        "value",
                        "value_not_permitted",
                        Some("SCHEMA-12"),
                        vec![
                            ("value", V::Text("bad\"\nλ")),
                            (
                                "permitted",
                                V::TextList(&["z".into(), "a".into(), "z".into()])
                            ),
                            ("source", V::Text("held/parent.yaml")),
                            ("entry", V::Text("held/entry.yaml")),
                        ]
                    ),
                    (
                        "validation",
                        "value",
                        "pattern_mismatch",
                        Some("SCHEMA-12"),
                        vec![
                            ("value", V::Text("bad\"\nλ")),
                            ("pattern", V::Text("^OK$")),
                            ("source", V::Text("held/parent.yaml")),
                            ("entry", V::Text("held/entry.yaml")),
                        ]
                    ),
                    (
                        "validation",
                        "value",
                        "minimum_length",
                        Some("SCHEMA-12"),
                        vec![
                            ("minimum", V::Integer(BIG)),
                            ("source", V::Text("held/parent.yaml")),
                            ("entry", V::Text("held/entry.yaml")),
                        ]
                    ),
                ]
            );
            let first = report.iter().next().unwrap().context.collect::<Vec<_>>();
            let V::Text(text) = first[0].1 else {
                panic!("text input")
            };
            let N::Text(held) = &input.nodes()[input.root()] else {
                unreachable!()
            };
            assert_eq!(text.as_ptr(), held.as_ptr());
            let V::TextList(values) = first[1].1 else {
                panic!("permitted values")
            };
            assert_eq!(
                values.as_ptr(),
                located.descriptor.permitted().unwrap().as_ptr()
            );
            assert_eq!(first[2].1, V::Text(origins[0].1));
            let V::Text(source) = first[2].1 else {
                unreachable!()
            };
            assert_eq!(source.as_ptr(), origins[0].1.as_ptr());

            let items = document(Authored::List(vec![Authored::integer("1")]));
            let findings = schema
                .validate_descriptor(
                    schema.root_class().fields[1].descriptor,
                    &items,
                    items.root(),
                    "items",
                    false,
                )
                .unwrap();
            let report = admit_schema_diagnostics(&schema, Some(&items), &findings, &[]).unwrap();
            assert_eq!(
                records(&report),
                vec![(
                    "validation",
                    "items",
                    "invalid_size",
                    Some("SCHEMA-12"),
                    vec![("size", V::Integer(BIG))]
                )]
            );
        }

        #[test]
        fn default_references_use_the_explicit_held_declaring_module_arena() {
            let schema = schema();
            let defaults = schema.validate_defaults().unwrap();
            let descriptor = schema.root_class().fields[0].descriptor;
            let diagnostics = &defaults
                .iter()
                .find(|d| d.descriptor == descriptor)
                .unwrap()
                .diagnostics;
            let located = &schema.descriptors()[descriptor];
            let input = &schema.modules()[located.module].document;
            let report = admit_schema_diagnostics(&schema, Some(input), diagnostics, &[]).unwrap();
            assert_eq!(
                records(&report),
                vec![
                    (
                        "validation",
                        "root_class.value.default",
                        "value_not_permitted",
                        Some("SCHEMA-12"),
                        vec![
                            ("value", V::Text("bad\"\nλ")),
                            (
                                "permitted",
                                V::TextList(&["z".into(), "a".into(), "z".into()])
                            ),
                        ]
                    ),
                    (
                        "validation",
                        "root_class.value.default",
                        "pattern_mismatch",
                        Some("SCHEMA-12"),
                        vec![("value", V::Text("bad\"\nλ")), ("pattern", V::Text("^OK$")),]
                    ),
                    (
                        "validation",
                        "root_class.value.default",
                        "minimum_length",
                        Some("SCHEMA-12"),
                        vec![("minimum", V::Integer(BIG))]
                    ),
                ]
            );
            let Err(error) = admit_schema_diagnostics(&schema, None, diagnostics, &[]) else {
                panic!("arena required")
            };
            assert_eq!(error, E::InvalidContext);
            let other = document(Authored::text("unrelated source"));
            let Err(error) = admit_schema_diagnostics(&schema, Some(&other), diagnostics, &[])
            else {
                panic!("wrong node arena")
            };
            assert_eq!(error, E::InvalidContext);
        }

        #[test]
        fn scalar_values_counts_and_both_kind_vocabularies_are_core_owned_and_borrowed() {
            let schema = schema();
            for node in [
                N::Null,
                N::Boolean(true),
                N::Integer(BIG.into()),
                N::Float(-0.0),
                N::Text("\0quoted\"λ".into()),
            ] {
                let input = Document::new(vec![node], 0).unwrap();
                let findings = [issue(
                    "scalar",
                    vec![
                        ("value", C::InputValue(0)),
                        ("count", C::Count(usize::MAX)),
                        ("absent", C::Null),
                    ],
                )];
                let report =
                    admit_schema_diagnostics(&schema, Some(&input), &findings, &[]).unwrap();
                let expected = match &input.nodes()[0] {
                    N::Null => V::Null,
                    N::Boolean(v) => V::Boolean(*v),
                    N::Integer(v) => V::Integer(v),
                    N::Float(v) => V::Float(*v),
                    N::Text(v) => V::Text(v),
                    _ => unreachable!(),
                };
                assert_eq!(
                    records(&report),
                    vec![(
                        "validation",
                        "scalar",
                        "pattern_mismatch",
                        Some("SCHEMA-12"),
                        vec![
                            ("value", expected),
                            ("count", V::Count(usize::MAX)),
                            ("absent", V::Null)
                        ]
                    )]
                );
                if let V::Float(value) = report.iter().next().unwrap().context.next().unwrap().1 {
                    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
                };
            }
            for (kind, value) in [
                (K::Null, "null"),
                (K::Boolean, "bool"),
                (K::Integer, "int"),
                (K::Float, "float"),
                (K::Text, "str"),
                (K::Sequence, "sequence"),
                (K::Mapping, "mapping"),
            ] {
                let findings = [issue("kind", vec![("value", C::ValueKind(kind))])];
                let report = admit_schema_diagnostics(&schema, None, &findings, &[]).unwrap();
                assert_eq!(
                    records(&report),
                    vec![(
                        "validation",
                        "kind",
                        "pattern_mismatch",
                        Some("SCHEMA-12"),
                        vec![("value", V::Text(value))]
                    )]
                );
                assert_eq!(
                    schema_diagnostic_kind_label(&C::ValueKind(kind)),
                    Some(value)
                );
            }
            assert_eq!(schema_diagnostic_kind_label(&C::Null), None);
        }

        #[test]
        fn normalized_default_occurrences_keep_the_pass_arena_and_original_origin_authority() {
            let schema = SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(Authored::map(vec![
                        ("version", Authored::text("1.0")),
                        (
                            "root_class",
                            Authored::List(vec![
                                Authored::map(vec![(
                                    "yamaa",
                                    Authored::map(vec![("type", Authored::text("str"))]),
                                )]),
                                Authored::map(vec![(
                                    "value",
                                    Authored::map(vec![
                                        ("type", Authored::text("str")),
                                        ("default", Authored::text("OK")),
                                    ]),
                                )]),
                                Authored::map(vec![(
                                    "constraint",
                                    Authored::map(vec![
                                        ("type", Authored::text("str")),
                                        (
                                            "values",
                                            Authored::List(vec![Authored::text("forbidden")]),
                                        ),
                                    ]),
                                )]),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            )
            .unwrap();
            // A complete 1.x document, normalized once its header is read.
            let raw = document(Authored::map(vec![("yamaa", Authored::text("1.0"))]));
            let normalized = schema.normalize_root(&raw).unwrap();
            let value = normalized
                .document
                .field(normalized.document.root(), "value")
                .unwrap();
            assert!(matches!(raw.nodes().get(value), Some(N::Mapping(_))));
            assert_eq!(normalized.document.nodes()[value], N::Text("OK".into()));
            assert_eq!(
                normalized.origins[value].source,
                SchemaSource::Default {
                    module: 0,
                    descriptor: schema.root_class().fields[1].descriptor,
                }
            );
            let origin_pointer = normalized.origins.as_ptr();
            let findings = schema
                .validate_descriptor(
                    schema.root_class().fields[2].descriptor,
                    &normalized.document,
                    value,
                    "value",
                    false,
                )
                .unwrap();
            let report =
                admit_schema_diagnostics(&schema, Some(&normalized.document), &findings, &[])
                    .unwrap();
            assert!(std::ptr::eq(
                report.document().unwrap(),
                &normalized.document
            ));
            assert_eq!(
                records(&report),
                vec![(
                    "validation",
                    "value",
                    "value_not_permitted",
                    Some("SCHEMA-12"),
                    vec![
                        ("value", V::Text("OK")),
                        ("permitted", V::TextList(&["forbidden".into()]))
                    ]
                )]
            );
            assert_eq!(normalized.origins.as_ptr(), origin_pointer);
            let Err(error) = admit_schema_diagnostics(&schema, Some(&raw), &findings, &[]) else {
                panic!("raw arena cannot resolve normalized occurrence")
            };
            assert_eq!(error, E::InvalidContext);
        }

        #[test]
        fn invalid_nodes_descriptors_and_duplicate_keys_refuse_without_a_partial_report() {
            let schema = schema();
            let input = document(Authored::List(vec![Authored::text("value")]));
            for context in [
                C::InputValue(usize::MAX),
                C::InputValue(input.root()),
                C::DescriptorValues(usize::MAX),
                C::DescriptorPattern(usize::MAX),
                C::DescriptorMinimum(usize::MAX),
                C::DescriptorSize(usize::MAX),
                C::DescriptorValues(schema.root_class().fields[1].descriptor),
                C::DescriptorPattern(schema.root_class().fields[1].descriptor),
                C::DescriptorMinimum(schema.root_class().fields[1].descriptor),
                C::DescriptorSize(schema.root_class().fields[0].descriptor),
            ] {
                let findings = [
                    issue("earlier", vec![("ok", C::Text("admitted".into()))]),
                    issue("later", vec![("bad", context)]),
                ];
                let Err(error) = admit_schema_diagnostics(&schema, Some(&input), &findings, &[])
                else {
                    panic!("invalid context")
                };
                assert_eq!(error, E::InvalidContext);
            }
            for (fields, origins) in [
                (vec![("value", C::Null), ("value", C::Null)], vec![]),
                (vec![("source", C::Null)], vec![("source", "held")]),
                (vec![], vec![("source", "first"), ("source", "second")]),
            ] {
                let findings = [issue("later", fields)];
                let Err(error) = admit_schema_diagnostics(&schema, None, &findings, &origins)
                else {
                    panic!("duplicate context")
                };
                assert_eq!(error, E::InvalidContext);
            }
        }

        #[test]
        fn repeated_projection_keeps_all_facts_and_missing_arenas_refuse() {
            let schema = schema();
            let findings = [SchemaDiagnostic {
                path: "p".into(),
                code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
                context: vec![("k", C::Text("v".into()))],
            }];
            let report = admit_schema_diagnostics(&schema, None, &findings, &[]).unwrap();
            assert_eq!(
                records(&report),
                vec![(
                    "validation",
                    "p",
                    "invalid_field_type",
                    Some("SCHEMA-12"),
                    vec![("k", V::Text("v"))]
                )]
            );
            assert_eq!(
                records(&admit_schema_diagnostics(&schema, None, &findings, &[]).unwrap()),
                records(&report)
            );
            let late = [findings[0].clone(), findings[0].clone()];
            let repeated = admit_schema_diagnostics(&schema, None, &late, &[]).unwrap();
            assert_eq!(
                records(&repeated),
                [records(&report), records(&report)].concat()
            );
            let invalid = [issue("bad", vec![("value", C::InputValue(0))])];
            let Err(error) = admit_schema_diagnostics(&schema, None, &invalid, &[]) else {
                panic!("missing arena")
            };
            assert_eq!(error, E::InvalidContext);
            assert_eq!(
                admit_schema_diagnostics(&schema, None, &[], &[])
                    .unwrap()
                    .iter()
                    .len(),
                0
            );
        }
    }
    mod schema_document {
        use yamaa_core::schema::{
            Document, DocumentError, DocumentKind, DocumentNode as N, DocumentResource,
        };

        #[test]
        fn decoded_tree_preserves_order_width_and_unicode_scalar_lengths() {
            let nodes = vec![
                N::Text("z".into()),
                N::Integer("9223372036854775808".into()),
                N::Text("a".into()),
                N::Text("e\u{301}😀".into()),
                N::Mapping(vec![(0, 1), (2, 3)]),
            ];
            let document = Document::new(nodes.clone(), 4).unwrap();
            assert_eq!(document.nodes(), nodes);
            assert_eq!(document.nodes()[3].length(), Some(3));
            assert_eq!(document.nodes()[1].kind(), DocumentKind::Integer);
            assert_eq!(document.nodes()[1].kind().name(), "integer");
            assert_eq!(document.field(4, "a"), Some(3));
            assert_eq!(document.field(4, "missing"), None);
        }

        #[test]
        fn source_adapters_share_scalar_identity_and_diagnostic_labels() {
            use yamaa_core::schema::scalar_diagnostic_label;
            let integer = N::Integer("9007199254740993".into());
            let rounded = N::Float(9007199254740992.0);
            assert!(integer.scalar_key() != rounded.scalar_key());
            assert!(N::Boolean(true).scalar_key() == N::Float(1.0).scalar_key());
            for invalid in [
                N::Integer("01".into()),
                N::Float(f64::NAN),
                N::Sequence(vec![]),
            ] {
                assert!(invalid.scalar_key().is_none());
            }
            assert_eq!(
                scalar_diagnostic_label(&integer).as_deref(),
                Some("9007199254740993")
            );
            assert_eq!(
                scalar_diagnostic_label(&N::Float(-0.0)).as_deref(),
                Some("-0.0")
            );
            assert_eq!(
                scalar_diagnostic_label(&N::Float(1e20)).as_deref(),
                Some("1e+20")
            );
            assert!(scalar_diagnostic_label(&N::Float(f64::INFINITY)).is_none());
        }

        #[test]
        fn malformed_arenas_do_not_enter_schema_interpretation() {
            for (nodes, root, error) in [
                (vec![], 0, DocumentError::InvalidRoot),
                (vec![N::Sequence(vec![0])], 0, DocumentError::InvalidChild),
                (
                    vec![N::Null, N::Sequence(vec![0, 0])],
                    1,
                    DocumentError::SharedChild,
                ),
                (vec![N::Null, N::Null], 1, DocumentError::UnreachableNode),
                (vec![N::Float(f64::INFINITY)], 0, DocumentError::NonFinite),
                (
                    vec![N::Sequence(vec![]), N::Null, N::Mapping(vec![(0, 1)])],
                    2,
                    DocumentError::NonScalarKey,
                ),
            ] {
                assert_eq!(Document::new(nodes, root), Err(error));
            }
            for text in ["", "-", "+1", "00", "-0", "01", "1.0", " 1", "1a", "١"] {
                assert_eq!(
                    Document::new(vec![N::Integer(text.into())], 0),
                    Err(DocumentError::InvalidInteger),
                    "{text:?}"
                );
            }
        }

        #[test]
        fn scalar_mapping_key_equality_is_exact_without_lossy_integer_promotion() {
            for (left, right) in [
                (N::Boolean(true), N::Integer("1".into())),
                (N::Float(-0.0), N::Boolean(false)),
                (N::Float(1.0), N::Integer("1".into())),
                (N::Float(-1.0), N::Integer("-1".into())),
                (
                    N::Float(9_007_199_254_740_992.0),
                    N::Integer("9007199254740992".into()),
                ),
                (
                    N::Float(18_446_744_073_709_551_616.0),
                    N::Integer("18446744073709551616".into()),
                ),
                (N::Float(0.5), N::Float(0.5)),
                (N::Null, N::Null),
            ] {
                assert_eq!(
                    Document::new(
                        vec![
                            left,
                            N::Null,
                            right,
                            N::Null,
                            N::Mapping(vec![(0, 1), (2, 3)])
                        ],
                        4
                    ),
                    Err(DocumentError::DuplicateKey)
                );
            }
            for (left, right) in [
                (
                    N::Float(9_007_199_254_740_992.0),
                    N::Integer("9007199254740993".into()),
                ),
                (N::Float(f64::from_bits(1)), N::Integer("0".into())),
                (N::Float(-0.5), N::Float(0.5)),
                (N::Text("1".into()), N::Integer("1".into())),
            ] {
                assert!(Document::new(
                    vec![
                        left,
                        N::Null,
                        right,
                        N::Null,
                        N::Mapping(vec![(0, 1), (2, 3)])
                    ],
                    4
                )
                .is_ok());
            }
        }

        #[test]
        fn fixed_decoded_depth_refuses_the_next_layer_and_independent_documents_stay_exact() {
            let nodes = vec![N::Text("😀".into()), N::Sequence(vec![0])];
            let one = Document::new(nodes.clone(), 1).unwrap();
            let two = Document::new(nodes.clone(), 1).unwrap();
            assert_eq!(one.nodes(), nodes);
            assert_eq!(one, two);
            let mut nodes = vec![N::Null];
            for i in 0..63 {
                nodes.push(N::Sequence(vec![i]));
            }
            assert!(Document::new(nodes.clone(), 63).is_ok());
            nodes.push(N::Sequence(vec![63]));
            assert_eq!(
                Document::new(nodes, 64),
                Err(DocumentError::Limit {
                    resource: DocumentResource::Depth,
                    limit: 64
                })
            );
        }
    }
}
