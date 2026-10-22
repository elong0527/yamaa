//! Schema contracts: schema validation.
mod schema {
    mod schema_validation {
        use yamaa_core::schema::{
            BundleError, BundleIssueKind, DescriptorIssue, Document, DocumentKind,
            DocumentNode as N, NormalizationError, NormalizationResource, NormalizedDocument,
            SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaSource, SchemaStructure,
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        fn descriptor(kind: &'static str) -> V {
            V::map(vec![("type", V::text(kind))])
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, descriptor(kind))])
        }
        fn bundle(fields: Vec<V>, declarations: Vec<(&'static str, V)>) -> SchemaStructure {
            bundle_headed("schema_version", fields, declarations)
        }
        /// A root class headed by the 1.x `yamaa`, which a complete 1.x
        /// document is normalized against by `normalize_root` once its
        /// header is read (SCHEMA-2).
        fn bundle_v1(fields: Vec<V>, declarations: Vec<(&'static str, V)>) -> SchemaStructure {
            bundle_headed("yamaa", fields, declarations)
        }
        fn bundle_headed(
            header: &'static str,
            fields: Vec<V>,
            declarations: Vec<(&'static str, V)>,
        ) -> SchemaStructure {
            let mut fields_with_version = vec![field(header, "str")];
            fields_with_version.extend(fields);
            let mut entries = vec![
                ("version", V::text("1.0")),
                ("root_class", V::List(fields_with_version)),
            ];
            entries.extend(declarations);
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(entries)),
                }],
                0,
                "root_class",
            )
            .unwrap()
        }

        #[test]
        fn invalid_default_preparation_never_bypasses_normalization_admission() {
            let mut schema = bundle_v1(
                vec![V::map(vec![(
                    "value",
                    V::map(vec![("type", V::text("str")), ("default", V::integer("2"))]),
                )])],
                vec![],
            );
            let input = document(V::map(vec![("yamaa", V::text("1.0"))]));
            assert!(matches!(
                schema.normalize_root(&input),
                Err(NormalizationError::Defaults(_))
            ));
            let findings = schema.prepare_defaults().unwrap();
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].diagnostics[0].definition().condition,
                "invalid_field_type"
            );
            assert!(matches!(
                schema.normalize_root(&input),
                Err(NormalizationError::Defaults(_))
            ));
        }
        fn validate(
            schema: &SchemaStructure,
            field: &str,
            value: V,
            fragment: bool,
        ) -> Vec<SchemaDiagnostic> {
            let input = document(value);
            let descriptor = schema
                .root_class()
                .fields
                .iter()
                .find(|f| f.name == field)
                .unwrap()
                .descriptor;
            schema
                .validate_descriptor(descriptor, &input, input.root(), field, fragment)
                .unwrap()
        }
        fn findings(diagnostics: &[SchemaDiagnostic]) -> Vec<(&str, &str)> {
            diagnostics
                .iter()
                .map(|d| (d.path.as_str(), d.definition().condition))
                .collect()
        }

        #[test]
        fn incompatible_decoded_values_keep_all_portable_kinds() {
            let schema = bundle(vec![field("value", "int"), field("text", "str")], vec![]);
            for (value, kind, name, field) in [
                (V::Scalar(N::Null), DocumentKind::Null, "null", "value"),
                (V::boolean(true), DocumentKind::Boolean, "boolean", "value"),
                (V::integer("7"), DocumentKind::Integer, "integer", "text"),
                (V::float(1.5), DocumentKind::Float, "float", "value"),
                (V::text("x"), DocumentKind::Text, "text", "value"),
                (V::List(vec![]), DocumentKind::Sequence, "sequence", "value"),
                (V::map(vec![]), DocumentKind::Mapping, "mapping", "value"),
            ] {
                let result = validate(&schema, field, value, false);
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].context[1], ("actual", C::ValueKind(kind)));
                assert_eq!(kind.name(), name);
            }
            let input = document(V::Scalar(N::Null));
            let descriptor = schema.root_class().fields[1].descriptor;
            let result = schema
                .validate_descriptor(descriptor, &input, input.root(), "value", false)
                .unwrap();
            assert_eq!(
                result[0].context[1],
                ("actual", C::ValueKind(DocumentKind::Null))
            );
        }

        /// 0.5 only (#3447): `validate_document` checks `schema_version`
        /// against the bundle's version before any field, as SCHEMA-2
        /// states for a 0.5 file. A 1.x header is read before any schema
        /// (SCHEMA-2), and the class order of a complete 1.x document is
        /// the next test, which SCHEMA-13 can cite once this one goes.
        #[test]
        fn version_precedes_field_findings_and_classes_keep_schema_then_input_order() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "needed",
                        V::map(vec![
                            ("type", V::text("str")),
                            ("required", V::boolean(true)),
                        ]),
                    )]),
                    field("count", "int"),
                ],
                vec![],
            );
            let input = document(V::map(vec![
                ("unknown", V::Scalar(N::Null)),
                ("count", V::boolean(true)),
                ("schema_version", V::text("old")),
            ]));
            let result = schema.validate_document(&input).unwrap();
            assert_eq!(
                findings(&result),
                [("schema_version", "schema_version_mismatch")]
            );
            assert_eq!(result[0].context[0], ("expected", C::Text("1.0".into())));
            let input = document(V::map(vec![
                ("unknown", V::Scalar(N::Null)),
                ("count", V::boolean(true)),
                ("schema_version", V::text("1.0")),
            ]));
            let result = schema.validate_document(&input).unwrap();
            assert_eq!(
                findings(&result),
                [
                    ("needed", "missing_required_field"),
                    ("count", "invalid_field_type"),
                    ("unknown", "unknown_field"),
                ]
            );
            assert_eq!(
                result[1].context,
                [
                    ("expected", C::Text("int".into())),
                    ("actual", C::ValueKind(DocumentKind::Boolean))
                ]
            );
            for value in [V::Scalar(N::Null), V::List(vec![]), V::map(vec![])] {
                let result = schema.validate_document(&document(value)).unwrap();
                assert_eq!(findings(&result), [("$", "invalid_field_type")]);
            }
        }

        #[test]
        fn classes_keep_schema_then_input_order_in_a_complete_document() {
            let schema = bundle_v1(
                vec![
                    V::map(vec![(
                        "needed",
                        V::map(vec![
                            ("type", V::text("str")),
                            ("required", V::boolean(true)),
                        ]),
                    )]),
                    field("count", "int"),
                ],
                vec![],
            );
            let invalid = |value: V| match schema.normalize_root(&document(value)) {
                Err(NormalizationError::Invalid(found)) => found,
                other => panic!("expected findings, got {other:?}"),
            };
            let result = invalid(V::map(vec![
                ("unknown", V::Scalar(N::Null)),
                ("count", V::boolean(true)),
                ("yamaa", V::text("1.0")),
            ]));
            assert_eq!(
                findings(&result),
                [
                    ("needed", "missing_required_field"),
                    ("count", "invalid_field_type"),
                    ("unknown", "unknown_field"),
                ]
            );
            assert_eq!(
                result[1].context,
                [
                    ("expected", C::Text("int".into())),
                    ("actual", C::ValueKind(DocumentKind::Boolean))
                ]
            );
            // The header reader refuses an empty root before this schema.
            for value in [V::Scalar(N::Null), V::List(vec![])] {
                assert_eq!(findings(&invalid(value)), [("$", "invalid_field_type")]);
            }
        }

        #[test]
        fn union_selects_success_then_matching_outer_type_and_recursion_is_value_local() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "choice",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("int"), V::text("record")]),
                        )]),
                    )]),
                    field("tree", "tree_value"),
                ],
                vec![
                    (
                        "record",
                        V::List(vec![V::map(vec![(
                            "id",
                            V::map(vec![
                                ("type", V::text("str")),
                                ("required", V::boolean(true)),
                            ]),
                        )])]),
                    ),
                    (
                        "tree_value",
                        V::map(vec![(
                            "type",
                            V::List(vec![
                                V::text("tree_value"),
                                V::text("tree_branch"),
                                V::text("str"),
                            ]),
                        )]),
                    ),
                    (
                        "tree_branch",
                        V::map(vec![("type", V::text("list[tree_value]"))]),
                    ),
                ],
            );
            assert!(validate(&schema, "choice", V::integer("1"), false).is_empty());
            assert_eq!(
                findings(&validate(&schema, "choice", V::map(vec![]), false)),
                [("choice.id", "missing_required_field")]
            );
            assert_eq!(
                validate(&schema, "choice", V::boolean(false), false)[0].context[0],
                ("expected", C::Text("int".into()))
            );
            assert!(validate(
                &schema,
                "tree",
                V::List(vec![V::text("leaf"), V::List(vec![V::text("nested")])]),
                false
            )
            .is_empty());
            let result = validate(
                &schema,
                "tree",
                V::List(vec![V::text("leaf"), V::List(vec![V::boolean(true)])]),
                false,
            );
            assert_eq!(findings(&result), [("tree[1][0]", "invalid_field_type")]);
            assert_eq!(
                result[0].context[0],
                ("expected", C::Text("tree_value".into()))
            );
        }

        fn required_field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(
                name,
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(true)),
                ]),
            )])
        }
        fn operations_registry() -> Vec<(&'static str, V)> {
            vec![
                (
                    "expression",
                    V::map(vec![("registry", V::text("operations"))]),
                ),
                (
                    "operations",
                    V::map(vec![
                        ("source", descriptor("str")),
                        ("literal", descriptor("int")),
                    ]),
                ),
            ]
        }
        fn handled_bundle() -> SchemaStructure {
            let mut declarations = vec![
                (
                    "derivation",
                    V::map(vec![(
                        "type",
                        V::List(vec![
                            V::text("str"),
                            V::text("expression"),
                            V::text("handled_expression_class"),
                        ]),
                    )]),
                ),
                (
                    "handled_expression_class",
                    V::List(vec![
                        required_field("value", "expression"),
                        field("unconvertible", "int"),
                    ]),
                ),
            ];
            declarations.extend(operations_registry());
            bundle(vec![field("derive", "derivation")], declarations)
        }

        #[test]
        fn union_reports_inside_the_member_declaring_the_written_keys() {
            // Handled derivation: the written keys belong to
            // handled_expression_class, not to the expression registry
            // member tried before it, so the finding lands inside `value`.
            let schema = handled_bundle();
            let result = validate(
                &schema,
                "derive",
                V::map(vec![
                    ("value", V::map(vec![("literal", V::text("x"))])),
                    ("unconvertible", V::integer("0")),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [("derive.value.literal", "invalid_field_type")]
            );

            // Typed intermediate derivation: the written keys belong to
            // intermediate_binding_class, not to the untyped member tried
            // before it, so the finding lands inside `derivation`.
            let mut declarations = vec![
                (
                    "intermediate_derivation",
                    V::map(vec![(
                        "type",
                        V::List(vec![
                            V::text("untyped_intermediate_derivation"),
                            V::text("intermediate_binding_class"),
                        ]),
                    )]),
                ),
                (
                    "untyped_intermediate_derivation",
                    V::map(vec![(
                        "type",
                        V::List(vec![
                            V::text("str"),
                            V::text("expression"),
                            V::text("intermediate_expression_class"),
                        ]),
                    )]),
                ),
                (
                    "intermediate_expression_class",
                    V::List(vec![required_field("value", "expression")]),
                ),
                (
                    "intermediate_binding_class",
                    V::List(vec![
                        required_field("type", "str"),
                        required_field("derivation", "untyped_intermediate_derivation"),
                        field("unconvertible", "int"),
                    ]),
                ),
            ];
            declarations.extend(operations_registry());
            let schema = bundle(
                vec![field("binding", "intermediate_derivation")],
                declarations,
            );
            let result = validate(
                &schema,
                "binding",
                V::map(vec![
                    ("type", V::text("str")),
                    ("derivation", V::map(vec![("literal", V::text("x"))])),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [("binding.derivation.literal", "invalid_field_type")]
            );
        }

        #[test]
        fn union_keeps_the_previous_fallback_when_no_member_declares_the_keys() {
            let schema = handled_bundle();
            // An empty mapping declares no written keys.
            assert_eq!(
                findings(&validate(&schema, "derive", V::map(vec![]), false)),
                [("derive", "invalid_operation_count")]
            );
            // A key that no member declares keeps the registry fallback.
            assert_eq!(
                findings(&validate(
                    &schema,
                    "derive",
                    V::map(vec![("absent", V::Scalar(N::Null))]),
                    false
                )),
                [("derive.absent", "unknown_operation")]
            );
        }

        #[test]
        fn case_otherwise_reports_inside_the_otherwise_member() {
            let schema = case_bundle("expressions");
            let result = validate(
                &schema,
                "derive",
                case(vec![
                    branch(V::integer(1)),
                    otherwise(V::map(vec![("literal", V::text("x"))])),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [("derive.case[1].otherwise.literal", "invalid_field_type")]
            );
        }

        #[test]
        fn fragments_suppress_nested_requiredness_but_keep_type_and_unknown_field_checks() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![(
                    "record",
                    V::List(vec![
                        field("name", "str"),
                        V::map(vec![(
                            "id",
                            V::map(vec![
                                ("type", V::text("int")),
                                ("required", V::boolean(true)),
                            ]),
                        )]),
                    ]),
                )],
            );
            assert_eq!(
                findings(&validate(
                    &schema,
                    "items",
                    V::List(vec![V::map(vec![("name", V::text("DM"))])]),
                    false
                )),
                [("items.DM.id", "missing_required_field")]
            );
            assert!(validate(
                &schema,
                "items",
                V::List(vec![V::map(vec![("name", V::text("DM"))])]),
                true
            )
            .is_empty());
            assert_eq!(
                findings(&validate(
                    &schema,
                    "items",
                    V::List(vec![V::map(vec![
                        ("name", V::text("DM")),
                        ("extra", V::Scalar(N::Null)),
                        ("id", V::boolean(false))
                    ])]),
                    true
                )),
                [
                    ("items.DM.id", "invalid_field_type"),
                    ("items.DM.extra", "unknown_field"),
                ]
            );
        }

        #[test]
        fn constraints_keep_order_unicode_lengths_and_alias_requirements_after_type_success() {
            let schema = bundle(
                vec![field("kind", "column_type"), field("number", "float")],
                vec![(
                    "column_type",
                    V::map(vec![
                        ("type", V::text("str")),
                        ("values", V::List(vec![V::text("ok")])),
                        ("pattern", V::text("[A-Z]+")),
                        ("min_length", V::integer("2")),
                    ]),
                )],
            );
            let result = validate(&schema, "kind", V::text("\u{e9}"), false);
            assert_eq!(
                findings(&result),
                [
                    ("kind", "value_not_permitted"),
                    ("kind", "pattern_mismatch"),
                    ("kind", "minimum_length")
                ]
            );
            // A type outside the closed list is VALUE-1's; the other
            // constraints are the schema language's.
            assert_eq!(
                result
                    .iter()
                    .map(|d| d.definition().rule)
                    .collect::<Vec<_>>(),
                [Some("VALUE-1"), Some("SCHEMA-12"), Some("SCHEMA-12")]
            );
            let result = validate(&schema, "kind", V::boolean(true), false);
            assert_eq!(findings(&result), [("kind", "invalid_field_type")]);
            assert_eq!(result[0].definition().rule, Some("SCHEMA-12"));
            assert!(validate(
                &schema,
                "number",
                V::integer("99999999999999999999999999999999999999999999999999"),
                false
            )
            .is_empty());
            assert!(!validate(&schema, "number", V::boolean(false), false).is_empty());
        }

        #[test]
        fn defaults_resolve_forward_aliases_and_return_the_declaring_descriptor() {
            let schema = bundle(
                vec![
                    V::map(vec![(
                        "good",
                        V::map(vec![
                            ("type", V::text("later")),
                            ("default", V::text("valid")),
                        ]),
                    )]),
                    V::map(vec![(
                        "bad",
                        V::map(vec![
                            ("type", V::text("later")),
                            ("default", V::integer("9")),
                        ]),
                    )]),
                ],
                vec![("later", descriptor("str"))],
            );
            let result = schema.validate_defaults().unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(
                schema.descriptors()[result[0].descriptor].path,
                "root_class.bad"
            );
            assert_eq!(
                findings(&result[0].diagnostics),
                [("root_class.bad.default", "invalid_field_type")]
            );
            assert_eq!(
                result[0].diagnostics[0].context[0],
                ("expected", C::Text("later".into()))
            );
        }

        #[test]
        fn registry_and_bare_derivation_diagnostics_preserve_paths_and_context() {
            let schema = bundle(
                vec![field("derive", "derivation")],
                vec![
                    (
                        "derivation",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("str"), V::text("expression")]),
                        )]),
                    ),
                    (
                        "expression",
                        V::map(vec![("registry", V::text("operations"))]),
                    ),
                    (
                        "operations",
                        V::map(vec![
                            ("source", descriptor("str")),
                            ("literal", descriptor("int")),
                        ]),
                    ),
                ],
            );
            assert!(validate(
                &schema,
                "derive",
                V::map(vec![("literal", V::integer("4"))]),
                false
            )
            .is_empty());
            // The bare string is the only spelling of a plain source (EXPR-5).
            let restated = validate(
                &schema,
                "derive",
                V::map(vec![("source", V::text("DM.AGE"))]),
                false,
            );
            assert_eq!(findings(&restated), [("derive", "redundant_long_form")]);
            assert_eq!(
                restated[0].context,
                [
                    ("field", C::Text("source".into())),
                    ("shorthand", C::InputValue(1)),
                ]
            );
            assert_eq!(restated[0].definition().rule, Some("SCHEMA-15"));
            assert_eq!(
                findings(&validate(&schema, "derive", V::map(vec![]), false)),
                [("derive", "invalid_operation_count")]
            );
            assert_eq!(
                findings(&validate(
                    &schema,
                    "derive",
                    V::map(vec![("absent", V::Scalar(N::Null))]),
                    false
                )),
                [("derive.absent", "unknown_operation")]
            );
            let result = validate(&schema, "derive", V::integer("4"), false);
            assert_eq!(findings(&result), [("derive", "bare_derivation_scalar")]);
            assert_eq!(result[0].definition().rule, Some("EXPR-5"));
        }

        fn case_bundle(registry: &'static str) -> SchemaStructure {
            let required = |kind: &'static str| {
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(true)),
                ])
            };
            bundle(
                vec![field("derive", "expression")],
                vec![
                    ("expression", V::map(vec![("registry", V::text(registry))])),
                    (
                        registry,
                        V::map(vec![
                            ("case", descriptor("list[case_item_class]")),
                            ("literal", descriptor("int")),
                        ]),
                    ),
                    (
                        "case_item_class",
                        V::map(vec![(
                            "type",
                            V::List(vec![
                                V::text("case_branch_class"),
                                V::text("case_otherwise_class"),
                            ]),
                        )]),
                    ),
                    (
                        "case_branch_class",
                        V::List(vec![
                            V::map(vec![("when", required("str"))]),
                            V::map(vec![("then", required("case_result"))]),
                        ]),
                    ),
                    (
                        "case_otherwise_class",
                        V::List(vec![V::map(vec![("otherwise", required("case_result"))])]),
                    ),
                    (
                        "case_result",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("int"), V::text("expression")]),
                        )]),
                    ),
                ],
            )
        }
        fn branch(then: V) -> V {
            V::map(vec![("when", V::text("AGE >= 65")), ("then", then)])
        }
        fn otherwise(value: V) -> V {
            V::map(vec![("otherwise", value)])
        }
        fn case(items: Vec<V>) -> V {
            V::map(vec![("case", V::List(items))])
        }

        #[test]
        fn case_item_order_is_a_req_0339_finding_after_every_item_matched_its_class() {
            let schema = case_bundle("expressions");
            for items in [
                vec![branch(V::integer(1)), otherwise(V::integer(0))],
                vec![branch(V::integer(1))],
                vec![
                    branch(V::integer(1)),
                    branch(V::integer(2)),
                    otherwise(V::integer(0)),
                ],
            ] {
                assert!(validate(&schema, "derive", case(items), false).is_empty());
            }
            let no_branch = "at least one when/then item";
            let not_last = "otherwise only as the last item";
            for (items, expected) in [
                (
                    vec![otherwise(V::integer(0))],
                    vec![("derive.case", no_branch)],
                ),
                (vec![], vec![("derive.case", no_branch)]),
                (
                    vec![otherwise(V::integer(0)), branch(V::integer(1))],
                    vec![("derive.case[0]", not_last)],
                ),
                (
                    vec![
                        branch(V::integer(1)),
                        otherwise(V::integer(0)),
                        otherwise(V::integer(2)),
                    ],
                    vec![("derive.case[1]", not_last)],
                ),
                (
                    vec![otherwise(V::integer(0)), otherwise(V::integer(2))],
                    vec![("derive.case", no_branch), ("derive.case[0]", not_last)],
                ),
                (
                    vec![
                        branch(case(vec![otherwise(V::integer(1))])),
                        otherwise(V::integer(0)),
                    ],
                    vec![("derive.case[0].then.case", no_branch)],
                ),
            ] {
                for fragment in [false, true] {
                    let result = validate(&schema, "derive", case(items.clone()), fragment);
                    let actual: Vec<_> = result
                        .iter()
                        .map(|d| {
                            let definition = d.definition();
                            assert_eq!(
                                (definition.phase, definition.condition, definition.rule),
                                ("validation", "invalid_field_type", Some("EXPR-7"))
                            );
                            let [("expected", C::Text(text))] = d.context.as_slice() else {
                                panic!("expected context: {d:?}");
                            };
                            (d.path.as_str(), text.as_str())
                        })
                        .collect();
                    assert_eq!(actual, expected, "fragment={fragment}");
                }
            }
            // Item findings come first; the order rule waits for well-formed items.
            let result = validate(
                &schema,
                "derive",
                case(vec![otherwise(V::text("N"))]),
                false,
            );
            assert!(!result.is_empty());
            assert!(
                result.iter().all(|d| d.definition().rule != Some("EXPR-7")),
                "{result:?}"
            );
            // The rule belongs to the expression registry's case, not to a like-named entry.
            let other = case_bundle("operations");
            assert!(validate(
                &other,
                "derive",
                case(vec![otherwise(V::integer(0))]),
                false
            )
            .is_empty());
        }

        #[test]
        fn typed_dictionary_keys_have_independent_key_and_value_paths() {
            let schema = bundle(vec![field("mapping", "dict[str, int]")], vec![]);
            for (key, spelling, valuepath) in [
                (V::float(0.0), "0.0", "mapping.0.0"),
                (V::float(-0.0), "-0.0", "mapping.-0.0"),
                (V::float(1.0), "1.0", "mapping.1.0"),
                (V::float(1e-5), "1e-05", "mapping.1e-05"),
                (V::float(1e16), "1e+16", "mapping.1e+16"),
                (V::float(f64::from_bits(1)), "5e-324", "mapping.5e-324"),
                (V::integer("42"), "42", "mapping[42]"),
                (V::boolean(true), "True", "mapping[True]"),
                (V::Scalar(N::Null), "None", "mapping.None"),
            ] {
                let result = validate(
                    &schema,
                    "mapping",
                    V::Pairs(vec![(key, V::boolean(false))]),
                    false,
                );
                assert_eq!(
                    findings(&result),
                    [
                        (
                            format!("mapping.key({spelling})").as_str(),
                            "invalid_field_type"
                        ),
                        (valuepath, "invalid_field_type")
                    ]
                );
            }
        }

        #[test]
        fn independent_union_attempts_do_not_publish_failed_alternative_findings() {
            let schema = bundle(
                vec![V::map(vec![(
                    "choice",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("int"), V::text("str")]),
                    )]),
                )])],
                vec![],
            );
            let descriptor = schema.root_class().fields[1].descriptor;
            let input = document(V::text("valid"));
            for _ in 0..3 {
                assert!(schema
                    .validate_descriptor(descriptor, &input, input.root(), "choice", false)
                    .unwrap()
                    .is_empty());
            }
        }

        fn normalize(
            schema: &SchemaStructure,
            field: &str,
            value: V,
            fragment: bool,
        ) -> NormalizedDocument {
            let input = document(value);
            let descriptor = schema
                .root_class()
                .fields
                .iter()
                .find(|f| f.name == field)
                .unwrap()
                .descriptor;
            schema
                .normalize_descriptor(descriptor, &input, input.root(), fragment)
                .unwrap()
        }

        /// Compare independently authored trees, including mapping order, without arena layout coupling.
        fn assert_value(actual: &Document, expected: V) {
            fn compare(a: &Document, ai: usize, b: &Document, bi: usize) {
                match (&a.nodes()[ai], &b.nodes()[bi]) {
                    (N::Sequence(aids), N::Sequence(bids)) => {
                        assert_eq!(aids.len(), bids.len());
                        for (&ai, &bi) in aids.iter().zip(bids) {
                            compare(a, ai, b, bi);
                        }
                    }
                    (N::Mapping(aids), N::Mapping(bids)) => {
                        assert_eq!(aids.len(), bids.len());
                        for (&(ak, av), &(bk, bv)) in aids.iter().zip(bids) {
                            compare(a, ak, b, bk);
                            compare(a, av, b, bv);
                        }
                    }
                    (a, b) => assert_eq!(a, b),
                }
            }
            let expected = document(expected);
            compare(actual, actual.root(), &expected, expected.root());
        }

        #[test]
        fn single_or_list_unions_are_bundle_errors_and_lists_normalize_as_written() {
            // One spelling per meaning: a union may not offer a type and a list
            // of it, so no bare value is ever wrapped into a list (SCHEMA-10).
            let declaration = |members: Vec<&'static str>| {
                V::map(vec![(
                    "type",
                    V::List(members.into_iter().map(V::text).collect()),
                )])
            };
            let Err(BundleError::Invalid(issues)) = SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root_class",
                            V::List(vec![
                                field("schema_version", "str"),
                                V::map(vec![("simple", declaration(vec!["str", "list[str]"]))]),
                                V::map(vec![(
                                    "nested",
                                    declaration(vec!["list[str]", "list[list[str]]"]),
                                )]),
                                V::map(vec![(
                                    "larger",
                                    declaration(vec!["str", "list[str]", "null"]),
                                )]),
                                V::map(vec![("plain", declaration(vec!["str", "list[int]"]))]),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root_class",
            ) else {
                panic!("a single-or-list union must be refused");
            };
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| (issue.path.as_str(), &issue.kind))
                    .collect::<Vec<_>>(),
                [
                    (
                        "root_class.simple",
                        &BundleIssueKind::Descriptor(DescriptorIssue::SingleOrList {
                            member: "str".into()
                        })
                    ),
                    (
                        "root_class.nested",
                        &BundleIssueKind::Descriptor(DescriptorIssue::SingleOrList {
                            member: "list[str]".into()
                        })
                    ),
                    (
                        "root_class.larger",
                        &BundleIssueKind::Descriptor(DescriptorIssue::SingleOrList {
                            member: "str".into()
                        })
                    ),
                ]
            );
            let schema = bundle(vec![field("names", "list[str]")], vec![]);
            let result = normalize(&schema, "names", V::List(vec![V::text("DM")]), false);
            assert_value(&result.document, V::List(vec![V::text("DM")]));
            assert!(result.origins.iter().all(|o| !o.generated));
            assert_eq!(
                findings(&validate(&schema, "names", V::text("DM"), false)),
                [("names", "invalid_field_type")]
            );
        }

        #[test]
        fn class_shorthand_materializes_defaults_but_fragments_keep_only_authored_values() {
            let schema = bundle(
                vec![V::map(vec![(
                    "item",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("str"), V::text("record")]),
                    )]),
                )])],
                vec![(
                    "record",
                    V::List(vec![
                        V::map(vec![(
                            "enabled",
                            V::map(vec![
                                ("type", V::text("bool")),
                                ("default", V::boolean(true)),
                            ]),
                        )]),
                        V::map(vec![(
                            "name",
                            V::map(vec![
                                ("type", V::text("str")),
                                ("required", V::boolean(true)),
                            ]),
                        )]),
                        field("optional", "int"),
                    ]),
                )],
            );
            let result = normalize(&schema, "item", V::text("DM"), false);
            assert_value(
                &result.document,
                V::map(vec![("name", V::text("DM")), ("enabled", V::boolean(true))]),
            );
            let enabled = result
                .document
                .field(result.document.root(), "enabled")
                .unwrap();
            assert!(matches!(
                result.origins[enabled].source,
                SchemaSource::Default { module: 0, .. }
            ));
            assert!(!result.origins[enabled].generated);
            assert_value(
                &normalize(&schema, "item", V::text("DM"), true).document,
                V::map(vec![("name", V::text("DM"))]),
            );
            assert_value(
                &normalize(
                    &schema,
                    "item",
                    V::map(vec![("name", V::text("DM")), ("optional", V::integer("1"))]),
                    false,
                )
                .document,
                V::map(vec![
                    ("enabled", V::boolean(true)),
                    ("name", V::text("DM")),
                    ("optional", V::integer("1")),
                ]),
            );
            assert_value(
                &normalize(&schema, "item", V::map(vec![]), true).document,
                V::map(vec![]),
            );
            // A written long form that holds only the shorthand's field restates
            // it (SCHEMA-15), in a complete value and in a fragment alike.
            for fragment in [false, true] {
                let input = document(V::map(vec![("name", V::text("DM"))]));
                let descriptor = schema.root_class().fields[1].descriptor;
                let Err(NormalizationError::Invalid(findings)) =
                    schema.normalize_descriptor(descriptor, &input, input.root(), fragment)
                else {
                    panic!("a restated shorthand must fail");
                };
                assert_eq!(
                    findings
                        .iter()
                        .map(|d| (d.path.as_str(), d.definition().condition))
                        .collect::<Vec<_>>(),
                    [("<normalization>", "redundant_long_form")]
                );
                assert_eq!(
                    findings[0].context,
                    [
                        ("field", C::Text("name".into())),
                        ("shorthand", C::InputValue(1)),
                    ]
                );
            }
        }

        #[test]
        fn bare_derivation_strings_are_source_references_never_calls() {
            let schema = bundle(
                vec![
                    field("derive", "derivation"),
                    field("result", "case_result"),
                ],
                vec![
                    (
                        "derivation",
                        V::map(vec![(
                            "type",
                            V::List(vec![
                                V::text("str"),
                                V::text("expression"),
                                V::text("handled"),
                            ]),
                        )]),
                    ),
                    (
                        "case_result",
                        V::map(vec![(
                            "type",
                            V::List(vec![V::text("str"), V::text("expression")]),
                        )]),
                    ),
                    (
                        "expression",
                        V::map(vec![("registry", V::text("operations"))]),
                    ),
                    (
                        "handled",
                        V::List(vec![
                            V::map(vec![(
                                "value",
                                V::map(vec![
                                    ("type", V::text("expression")),
                                    ("required", V::boolean(true)),
                                ]),
                            )]),
                            V::map(vec![(
                                "fallback",
                                V::map(vec![
                                    ("type", V::text("int")),
                                    ("default", V::integer("0")),
                                ]),
                            )]),
                        ]),
                    ),
                    (
                        "operations",
                        V::map(vec![
                            ("source", descriptor("str")),
                            ("to_number", descriptor("expression")),
                        ]),
                    ),
                ],
            );
            assert_value(
                &normalize(&schema, "derive", V::text("DM.AGE"), false).document,
                V::map(vec![
                    ("value", V::map(vec![("source", V::text("DM.AGE"))])),
                    ("fallback", V::integer("0")),
                ]),
            );
            // A string spelled like a call is still only a source reference
            // (EXPR-5); the source's own type then refuses it.
            for source in [
                "to_number(DM.AGE)",
                "  to_number( DM.AGE )  ",
                "nested(call(DM.AGE))",
                "1invalid(DM.AGE)",
                "op (DM.AGE)",
            ] {
                assert_value(
                    &normalize(&schema, "derive", V::text(source), false).document,
                    V::map(vec![
                        ("value", V::map(vec![("source", V::text(source))])),
                        ("fallback", V::integer("0")),
                    ]),
                );
                assert_value(
                    &normalize(&schema, "derive", V::text(source), true).document,
                    V::map(vec![("value", V::map(vec![("source", V::text(source))]))]),
                );
                assert_value(
                    &normalize(&schema, "result", V::text(source), false).document,
                    V::map(vec![("source", V::text(source))]),
                );
            }
        }

        #[test]
        fn recursive_defaults_are_resource_refusals_instead_of_silently_truncated_objects() {
            let schema = bundle(
                vec![field("tree", "record")],
                vec![(
                    "record",
                    V::List(vec![V::map(vec![(
                        "child",
                        V::map(vec![
                            ("type", V::text("record")),
                            ("default", V::map(vec![])),
                        ]),
                    )])]),
                )],
            );
            let input = document(V::map(vec![]));
            let descriptor = schema.root_class().fields[1].descriptor;
            assert_eq!(
                schema.normalize_descriptor(descriptor, &input, input.root(), false),
                Err(NormalizationError::Limit {
                    resource: NormalizationResource::Depth,
                    limit: 128
                })
            );
            assert_value(
                &normalize(&schema, "tree", V::map(vec![]), true).document,
                V::map(vec![]),
            );
        }

        #[test]
        fn normalization_validates_written_constraints_and_defaults_before_allocating_output() {
            let schema = bundle(
                vec![V::map(vec![(
                    "name",
                    V::map(vec![
                        ("type", V::text("str")),
                        ("values", V::List(vec![V::text("ok")])),
                    ]),
                )])],
                vec![],
            );
            let input = document(V::text("bad"));
            assert!(matches!(
                schema.normalize_descriptor(
                    schema.root_class().fields[1].descriptor,
                    &input,
                    input.root(),
                    false
                ),
                Err(NormalizationError::Invalid(_))
            ));
            let invalid_defaults = bundle(
                vec![V::map(vec![(
                    "bad",
                    V::map(vec![("type", V::text("str")), ("default", V::integer("1"))]),
                )])],
                vec![],
            );
            assert!(matches!(
                invalid_defaults.normalize_descriptor(
                    invalid_defaults.root_class().fields[1].descriptor,
                    &document(V::text("valid")),
                    0,
                    true
                ),
                Err(NormalizationError::Defaults(_))
            ));
        }

        #[test]
        fn independent_normalization_keeps_complete_results_and_original_input() {
            let schema = bundle(
                vec![V::map(vec![(
                    "value",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("str"), V::text("record")]),
                    )]),
                )])],
                vec![(
                    "record",
                    V::List(vec![V::map(vec![(
                        "name",
                        V::map(vec![
                            ("type", V::text("str")),
                            ("required", V::boolean(true)),
                        ]),
                    )])]),
                )],
            );
            let input = document(V::text("DM"));
            let before = input.clone();
            let descriptor = schema.root_class().fields[1].descriptor;
            let first = schema
                .normalize_descriptor(descriptor, &input, input.root(), false)
                .unwrap();
            let second = schema
                .normalize_descriptor(descriptor, &input, input.root(), false)
                .unwrap();
            assert_eq!(first, second);
            assert_eq!(input, before);
            assert_value(&first.document, V::map(vec![("name", V::text("DM"))]));
        }

        #[test]
        fn whole_document_normalization_keeps_schema_order_and_nested_default_provenance() {
            let schema = bundle_v1(
                vec![
                    V::map(vec![(
                        "groups",
                        V::map(vec![
                            ("type", V::text("dict[str, list[record]]")),
                            (
                                "default",
                                V::map(vec![(
                                    "DM",
                                    V::List(vec![V::map(vec![("name", V::text("AGE"))])]),
                                )]),
                            ),
                        ]),
                    )]),
                    field("explicit", "str"),
                ],
                vec![(
                    "record",
                    V::List(vec![
                        field("name", "str"),
                        V::map(vec![(
                            "enabled",
                            V::map(vec![
                                ("type", V::text("bool")),
                                ("default", V::boolean(true)),
                            ]),
                        )]),
                    ]),
                )],
            );
            let input = document(V::map(vec![
                ("explicit", V::text("written")),
                ("yamaa", V::text("1.0")),
            ]));
            let result = schema.normalize_root(&input).unwrap();
            assert_value(
                &result.document,
                V::map(vec![
                    ("yamaa", V::text("1.0")),
                    (
                        "groups",
                        V::map(vec![(
                            "DM",
                            V::List(vec![V::map(vec![
                                ("name", V::text("AGE")),
                                ("enabled", V::boolean(true)),
                            ])]),
                        )]),
                    ),
                    ("explicit", V::text("written")),
                ]),
            );
            assert_eq!(result.origins.len(), result.document.nodes().len());
            for (node, origin) in result.document.nodes().iter().zip(&result.origins) {
                if matches!(node, N::Text(text) if text == "AGE")
                    || matches!(node, N::Boolean(true))
                {
                    let SchemaSource::Default { module, descriptor } = origin.source else {
                        panic!("lost default provenance")
                    };
                    assert_eq!(
                        *node,
                        schema.modules()[module].document.nodes()[origin.node]
                    );
                    assert!(schema.descriptors()[descriptor]
                        .descriptor
                        .default_node()
                        .is_some());
                }
            }
            let fragment = normalize(
                &schema,
                "groups",
                V::map(vec![(
                    "DM",
                    V::List(vec![V::map(vec![("name", V::text("AGE"))])]),
                )]),
                true,
            );
            assert_value(
                &fragment.document,
                V::map(vec![(
                    "DM",
                    V::List(vec![V::map(vec![("name", V::text("AGE"))])]),
                )]),
            );
            assert!(fragment
                .origins
                .iter()
                .all(|origin| origin.source == SchemaSource::Input));
        }

        #[test]
        fn shorthand_checks_the_written_member_without_reinterpreting_generated_fields() {
            let schema = bundle(
                vec![V::map(vec![(
                    "item",
                    V::map(vec![(
                        "type",
                        V::List(vec![V::text("str"), V::text("record")]),
                    )]),
                )])],
                vec![(
                    "record",
                    V::List(vec![V::map(vec![(
                        "name",
                        V::map(vec![
                            ("type", V::text("str")),
                            ("required", V::boolean(true)),
                            ("values", V::List(vec![V::text("canonical-only")])),
                        ]),
                    )])]),
                )],
            );
            assert_value(
                &normalize(&schema, "item", V::text("written-string"), false).document,
                V::map(vec![("name", V::text("written-string"))]),
            );
            let input = document(V::map(vec![("name", V::text("written-string"))]));
            let result = schema.normalize_descriptor(
                schema.root_class().fields[1].descriptor,
                &input,
                input.root(),
                false,
            );
            assert!(
                matches!(result, Err(NormalizationError::Invalid(ref findings)) if findings[0].definition().condition == "value_not_permitted")
            );
        }

        #[test]
        fn malformed_compound_names_and_ids_remain_ordered_validation_findings() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![(
                    "record",
                    V::List(vec![field("name", "str"), field("id", "int")]),
                )],
            );
            let result = validate(
                &schema,
                "items",
                V::List(vec![
                    V::map(vec![(
                        "name",
                        V::List(vec![
                            V::text("DM"),
                            V::integer("2"),
                            V::boolean(true),
                            V::Scalar(N::Null),
                            V::float(-0.0),
                        ]),
                    )]),
                    V::map(vec![(
                        "id",
                        V::map(vec![("inner", V::List(vec![V::text("value")]))]),
                    )]),
                    V::map(vec![("name", V::List(vec![])), ("id", V::integer("10"))]),
                ]),
                false,
            );
            assert_eq!(
                findings(&result),
                [
                    (
                        "items.['DM', 2, True, None, -0.0].name",
                        "invalid_field_type"
                    ),
                    ("items.{'inner': ['value']}.id", "invalid_field_type"),
                    ("items.[].name", "invalid_field_type"),
                ]
            );
            assert_eq!(
                result[0].context,
                [
                    ("expected", C::Text("str".into())),
                    ("actual", C::ValueKind(DocumentKind::Sequence))
                ]
            );
        }

        #[test]
        fn compound_path_quotes_escape_only_the_required_characters_without_normalization() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![("record", V::List(vec![field("name", "str")]))],
            );
            let result = validate(
                &schema,
                "items",
                V::List(vec![V::map(vec![(
                    "name",
                    V::List(vec![
                        V::text("single'quote"),
                        V::text("both'\"quotes"),
                        V::text("\\\n\r\t\0"),
                        V::text("\u{e9} e\u{301} \u{1f600}"),
                        V::text("\u{a0}\u{2028}\u{202e}\u{e000}\u{0378}\u{10ffff}"),
                    ]),
                )])]),
                false,
            );
            assert_eq!(findings(&result), [(
        "items.[\"single'quote\", 'both\\'\"quotes', '\\\\\\n\\r\\t\\x00', '\u{e9} e\u{301} \u{1f600}', '\\xa0\\u2028\\u202e\\ue000\\u0378\\U0010ffff'].name",
        "invalid_field_type",
    )]);
        }

        #[test]
        fn invalid_compound_labels_keep_the_complete_authored_path() {
            let schema = bundle(
                vec![field("items", "list[record]")],
                vec![("record", V::List(vec![field("name", "str")]))],
            );
            let input = document(V::List(vec![V::map(vec![(
                "name",
                V::List(vec![V::text("oversized-label")]),
            )])]));
            let descriptor = schema.root_class().fields[1].descriptor;
            let result = schema
                .validate_descriptor(descriptor, &input, input.root(), "items", false)
                .unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].definition().condition, "invalid_field_type");
            assert_eq!(result[0].path, "items.['oversized-label'].name");
        }

        #[test]
        fn quoted_diagnostic_text_preserves_each_complete_unicode_spelling() {
            use yamaa_core::schema::quoted_diagnostic_text;
            assert_eq!(quoted_diagnostic_text("a'b"), "\"a'b\"");
            assert_eq!(quoted_diagnostic_text("\u{a0}\n\u{1f600}"), "'\\xa0\\n😀'");
            for value in ["a", "b", "c"] {
                assert_eq!(quoted_diagnostic_text(value), format!("'{value}'"));
            }
        }
    }
}
