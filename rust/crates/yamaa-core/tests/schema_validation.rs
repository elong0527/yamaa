//! Schema contracts: schema validation. The cases that validate a document
//! or a layer against a schema and compare its findings are
//! `fixtures/diagnostics/schema.json`; each test here proves a seam the
//! replay cannot reach, which its first comment names (#3798).
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
        /// A root class headed by the 1.x `yamaa`, which a complete 1.x
        /// document is normalized against by `normalize_root` once its
        /// header is read (SCHEMA-2).
        fn bundle(fields: Vec<V>, declarations: Vec<(&'static str, V)>) -> SchemaStructure {
            let mut fields_with_version = vec![field("yamaa", "str")];
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

        /// Seam: the schema's defaults state, `prepare_defaults` and the refusal
        /// `normalize_root` keeps before and after it, which no reader's pass reports.
        #[test]
        fn invalid_default_preparation_never_bypasses_normalization_admission() {
            let mut schema = bundle(
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

        /// Seam: the decoded kind a finding's `actual` holds
        /// (`SchemaContext::ValueKind`) and `DocumentKind::name()`'s vocabulary, which
        /// the readers' projection relabels.
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

        /// Seam: `validate_defaults` and the index of the descriptor each of its
        /// findings returns.
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

        /// Seam: mapping keys that are not text (floats, integers, Booleans and null),
        /// which a case's JSON document cannot write.
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

        /// Seam: repeated passes over one value give one result; a case makes one
        /// call.
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

        /// Seam: the bundle's admission issues (`BundleIssueKind`) and a normalized
        /// document with its origins.
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

        /// Seam: the normalized document, the origin of each default it fills, and
        /// normalization's own refusal at `<normalization>`.
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

        /// Seam: the normalized document a bare string expands to.
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

        /// Seam: normalization's depth limit, a resource refusal that is no finding,
        /// and the normalized fragment.
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

        /// Seam: which `NormalizationError` normalization refuses with.
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

        /// Seam: repeated normalization gives equal results and leaves its input
        /// unchanged.
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

        /// Seam: the normalized document and the provenance of each default node it
        /// holds.
        #[test]
        fn whole_document_normalization_keeps_schema_order_and_nested_default_provenance() {
            let schema = bundle(
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

        /// Seam: the normalized document of a shorthand and normalization's refusal of
        /// its written long form.
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

        /// Seam: the spelling `quoted_diagnostic_text` returns, a function no pass
        /// reports.
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
