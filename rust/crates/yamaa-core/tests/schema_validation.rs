use yamaa_core::schema::{
    BundleLimits, ConstraintError, Document, DocumentLimits, DocumentNode as N,
    NormalizationBudget, NormalizationError, NormalizationLimits, NormalizationResource,
    NormalizedDocument, SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaSource,
    SchemaStructure, ValidationBudget, ValidationError, ValidationLimits,
};

enum V {
    Text(&'static str),
    Int(&'static str),
    Float(f64),
    Bool(bool),
    Null,
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
    Pairs(Vec<(V, V)>),
}
use V::{Bool, Float, Int, List, Map, Null, Pairs, Text};

fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(v) => N::Text(v.into()),
            Int(v) => N::Integer(v.into()),
            Float(v) => N::Float(v),
            Bool(v) => N::Boolean(v),
            Null => N::Null,
            List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(Text(k), nodes), push(v, nodes)))
                    .collect(),
            ),
            Pairs(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(k, nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn descriptor(kind: &'static str) -> V {
    Map(vec![("type", Text(kind))])
}
fn field(name: &'static str, kind: &'static str) -> V {
    Map(vec![(name, descriptor(kind))])
}
fn bundle(fields: Vec<V>, declarations: Vec<(&'static str, V)>) -> SchemaStructure {
    let mut fields_with_version = vec![field("schema_version", "str")];
    fields_with_version.extend(fields);
    let mut entries = vec![
        ("version", Text("1.0")),
        ("root_class", List(fields_with_version)),
    ];
    entries.extend(declarations);
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(entries)),
        }],
        0,
        "root_class",
        BundleLimits::default(),
    )
    .unwrap()
}
fn budget() -> ValidationBudget {
    ValidationBudget::new(ValidationLimits::default())
}

#[test]
fn invalid_or_interrupted_default_preparation_never_bypasses_normalization_admission() {
    let mut schema = bundle(
        vec![Map(vec![(
            "value",
            Map(vec![("type", Text("str")), ("default", Int("2"))]),
        )])],
        vec![],
    );
    let input = document(Map(vec![("schema_version", Text("1.0"))]));
    let mut exhausted = ValidationBudget::new(ValidationLimits {
        work: 0,
        ..ValidationLimits::default()
    });
    assert_eq!(
        schema.prepare_defaults(&mut exhausted),
        Err(ValidationError::Constraint(ConstraintError::Work {
            limit: 0
        }))
    );
    assert!(matches!(
        schema.normalize_document(
            &input,
            &mut NormalizationBudget::new(NormalizationLimits::default())
        ),
        Err(NormalizationError::Defaults(_))
    ));
    let findings = schema.prepare_defaults(&mut budget()).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].diagnostics[0].condition, "invalid_field_type");
    assert!(matches!(
        schema.normalize_document(
            &input,
            &mut NormalizationBudget::new(NormalizationLimits::default())
        ),
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
        .validate_descriptor(
            descriptor,
            &input,
            input.root(),
            field,
            fragment,
            &mut budget(),
        )
        .unwrap()
}
fn findings(diagnostics: &[SchemaDiagnostic]) -> Vec<(&str, &str)> {
    diagnostics
        .iter()
        .map(|d| (d.path.as_str(), d.condition))
        .collect()
}

#[test]
fn version_precedes_field_findings_and_classes_keep_schema_then_input_order() {
    let schema = bundle(
        vec![
            Map(vec![(
                "needed",
                Map(vec![("type", Text("str")), ("required", Bool(true))]),
            )]),
            field("count", "int"),
        ],
        vec![],
    );
    let input = document(Map(vec![
        ("unknown", Null),
        ("count", Bool(true)),
        ("schema_version", Text("old")),
    ]));
    let result = schema.validate_document(&input, &mut budget()).unwrap();
    assert_eq!(
        findings(&result),
        [("schema_version", "schema_version_mismatch")]
    );
    assert_eq!(result[0].context[0], ("expected", C::Text("1.0".into())));
    let input = document(Map(vec![
        ("unknown", Null),
        ("count", Bool(true)),
        ("schema_version", Text("1.0")),
    ]));
    let result = schema.validate_document(&input, &mut budget()).unwrap();
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
            ("actual", C::Text("bool".into()))
        ]
    );
    for value in [Null, List(vec![]), Map(vec![])] {
        let result = schema
            .validate_document(&document(value), &mut budget())
            .unwrap();
        assert_eq!(findings(&result), [("$", "invalid_field_type")]);
    }
}

#[test]
fn union_selects_success_then_matching_outer_type_and_recursion_is_value_local() {
    let schema = bundle(
        vec![
            Map(vec![(
                "choice",
                Map(vec![("type", List(vec![Text("int"), Text("record")]))]),
            )]),
            field("tree", "tree_value"),
        ],
        vec![
            (
                "record",
                List(vec![Map(vec![(
                    "id",
                    Map(vec![("type", Text("str")), ("required", Bool(true))]),
                )])]),
            ),
            (
                "tree_value",
                Map(vec![(
                    "type",
                    List(vec![
                        Text("tree_value"),
                        Text("list[tree_value]"),
                        Text("str"),
                    ]),
                )]),
            ),
        ],
    );
    assert!(validate(&schema, "choice", Int("1"), false).is_empty());
    assert_eq!(
        findings(&validate(&schema, "choice", Map(vec![]), false)),
        [("choice.id", "missing_required_field")]
    );
    assert_eq!(
        validate(&schema, "choice", Bool(false), false)[0].context[0],
        ("expected", C::Text("int".into()))
    );
    assert!(validate(
        &schema,
        "tree",
        List(vec![Text("leaf"), List(vec![Text("nested")])]),
        false
    )
    .is_empty());
    let result = validate(
        &schema,
        "tree",
        List(vec![Text("leaf"), List(vec![Bool(true)])]),
        false,
    );
    assert_eq!(findings(&result), [("tree[1][0]", "invalid_field_type")]);
    assert_eq!(
        result[0].context[0],
        ("expected", C::Text("tree_value".into()))
    );
}

#[test]
fn fragments_suppress_nested_requiredness_but_keep_type_and_unknown_field_checks() {
    let schema = bundle(
        vec![field("items", "list[record]")],
        vec![(
            "record",
            List(vec![
                field("name", "str"),
                Map(vec![(
                    "id",
                    Map(vec![("type", Text("int")), ("required", Bool(true))]),
                )]),
            ]),
        )],
    );
    assert_eq!(
        findings(&validate(
            &schema,
            "items",
            List(vec![Map(vec![("name", Text("DM"))])]),
            false
        )),
        [("items.DM.id", "missing_required_field")]
    );
    assert!(validate(
        &schema,
        "items",
        List(vec![Map(vec![("name", Text("DM"))])]),
        true
    )
    .is_empty());
    assert_eq!(
        findings(&validate(
            &schema,
            "items",
            List(vec![Map(vec![
                ("name", Text("DM")),
                ("extra", Null),
                ("id", Bool(false))
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
            Map(vec![
                ("type", Text("str")),
                ("values", List(vec![Text("ok")])),
                ("pattern", Text("[A-Z]+")),
                ("min_length", Int("2")),
            ]),
        )],
    );
    let result = validate(&schema, "kind", Text("\u{e9}"), false);
    assert_eq!(
        findings(&result),
        [
            ("kind", "value_not_permitted"),
            ("kind", "pattern_mismatch"),
            ("kind", "minimum_length")
        ]
    );
    assert!(result.iter().all(|d| d.requirement == Some("REQ-0012")));
    let result = validate(&schema, "kind", Bool(true), false);
    assert_eq!(findings(&result), [("kind", "invalid_field_type")]);
    assert_eq!(result[0].requirement, Some("REQ-0287"));
    assert!(validate(
        &schema,
        "number",
        Int("99999999999999999999999999999999999999999999999999"),
        false
    )
    .is_empty());
    assert!(!validate(&schema, "number", Bool(false), false).is_empty());
}

#[test]
fn defaults_resolve_forward_aliases_and_return_the_declaring_descriptor() {
    let schema = bundle(
        vec![
            Map(vec![(
                "good",
                Map(vec![("type", Text("later")), ("default", Text("valid"))]),
            )]),
            Map(vec![(
                "bad",
                Map(vec![("type", Text("later")), ("default", Int("9"))]),
            )]),
        ],
        vec![("later", descriptor("str"))],
    );
    let result = schema.validate_defaults(&mut budget()).unwrap();
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
                Map(vec![("type", List(vec![Text("str"), Text("expression")]))]),
            ),
            ("expression", Map(vec![("registry", Text("operations"))])),
            (
                "operations",
                Map(vec![
                    ("source", descriptor("str")),
                    ("literal", descriptor("int")),
                ]),
            ),
        ],
    );
    assert!(validate(
        &schema,
        "derive",
        Map(vec![("source", Text("DM.AGE"))]),
        false
    )
    .is_empty());
    assert_eq!(
        findings(&validate(&schema, "derive", Map(vec![]), false)),
        [("derive", "invalid_operation_count")]
    );
    assert_eq!(
        findings(&validate(
            &schema,
            "derive",
            Map(vec![("absent", Null)]),
            false
        )),
        [("derive.absent", "unknown_operation")]
    );
    let result = validate(&schema, "derive", Int("4"), false);
    assert_eq!(findings(&result), [("derive", "bare_derivation_scalar")]);
    assert_eq!(result[0].requirement, Some("REQ-0320"));
}

#[test]
fn typed_dictionary_keys_have_independent_key_and_value_paths() {
    let schema = bundle(vec![field("mapping", "dict[str, int]")], vec![]);
    for (key, spelling, valuepath) in [
        (Float(0.0), "0.0", "mapping.0.0"),
        (Float(-0.0), "-0.0", "mapping.-0.0"),
        (Float(1.0), "1.0", "mapping.1.0"),
        (Float(1e-5), "1e-05", "mapping.1e-05"),
        (Float(1e16), "1e+16", "mapping.1e+16"),
        (Float(f64::from_bits(1)), "5e-324", "mapping.5e-324"),
        (Int("42"), "42", "mapping[42]"),
        (Bool(true), "True", "mapping[True]"),
        (Null, "None", "mapping.None"),
    ] {
        let result = validate(&schema, "mapping", Pairs(vec![(key, Bool(false))]), false);
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
fn failed_union_attempts_consume_shared_budget_and_fresh_attempts_are_independent() {
    let schema = bundle(
        vec![Map(vec![(
            "choice",
            Map(vec![("type", List(vec![Text("int"), Text("str")]))]),
        )])],
        vec![],
    );
    let descriptor = schema.root_class().fields[1].descriptor;
    let input = document(Text("valid"));
    let mut shared = budget();
    assert!(schema
        .validate_descriptor(
            descriptor,
            &input,
            input.root(),
            "choice",
            false,
            &mut shared
        )
        .unwrap()
        .is_empty());
    assert_eq!(shared.diagnostics_used(), 1);
    let work = shared.work_used();
    assert!(schema
        .validate_descriptor(
            descriptor,
            &input,
            input.root(),
            "choice",
            false,
            &mut shared
        )
        .unwrap()
        .is_empty());
    assert_eq!(shared.diagnostics_used(), 2);
    assert_eq!(shared.work_used(), work * 2);
    for (limits, expected) in [
        (
            ValidationLimits {
                work: 0,
                ..Default::default()
            },
            ValidationError::Constraint(ConstraintError::Work { limit: 0 }),
        ),
        (
            ValidationLimits {
                diagnostics: 0,
                ..Default::default()
            },
            ValidationError::Diagnostics { limit: 0 },
        ),
        (
            ValidationLimits {
                diagnostic_text_bytes: 0,
                ..Default::default()
            },
            ValidationError::DiagnosticText { limit: 0 },
        ),
        (
            ValidationLimits {
                depth: 0,
                ..Default::default()
            },
            ValidationError::Depth { limit: 0 },
        ),
    ] {
        assert_eq!(
            schema.validate_descriptor(
                descriptor,
                &input,
                input.root(),
                "choice",
                false,
                &mut ValidationBudget::new(limits)
            ),
            Err(expected)
        );
        assert!(schema
            .validate_descriptor(
                descriptor,
                &input,
                input.root(),
                "choice",
                false,
                &mut budget()
            )
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
        .normalize_descriptor(
            descriptor,
            &input,
            input.root(),
            fragment,
            &mut NormalizationBudget::new(NormalizationLimits::default()),
        )
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
fn collection_normalization_prioritizes_canonical_lists_even_when_t_is_a_list() {
    let schema = bundle(
        vec![
            Map(vec![(
                "simple",
                Map(vec![("type", List(vec![Text("str"), Text("list[str]")]))]),
            )]),
            Map(vec![(
                "nested",
                Map(vec![(
                    "type",
                    List(vec![Text("list[str]"), Text("list[list[str]]")]),
                )]),
            )]),
            Map(vec![(
                "larger",
                Map(vec![(
                    "type",
                    List(vec![Text("str"), Text("list[str]"), Text("null")]),
                )]),
            )]),
        ],
        vec![],
    );
    let result = normalize(&schema, "simple", Text("DM"), false);
    assert_value(&result.document, List(vec![Text("DM")]));
    assert!(result.origins[result.document.root()].generated);
    assert!(result
        .origins
        .iter()
        .all(|o| o.source == SchemaSource::Input));
    assert_value(
        &normalize(&schema, "simple", List(vec![Text("DM"), Text("AE")]), false).document,
        List(vec![Text("DM"), Text("AE")]),
    );
    assert_value(
        &normalize(&schema, "nested", List(vec![Text("DM")]), false).document,
        List(vec![List(vec![Text("DM")])]),
    );
    assert_value(
        &normalize(&schema, "nested", List(vec![List(vec![Text("DM")])]), false).document,
        List(vec![List(vec![Text("DM")])]),
    );
    assert_value(
        &normalize(&schema, "nested", List(vec![]), false).document,
        List(vec![]),
    );
    assert_value(
        &normalize(&schema, "larger", Text("DM"), false).document,
        Text("DM"),
    );
}

#[test]
fn class_shorthand_materializes_defaults_but_fragments_keep_only_authored_values() {
    let schema = bundle(
        vec![Map(vec![(
            "item",
            Map(vec![("type", List(vec![Text("str"), Text("record")]))]),
        )])],
        vec![(
            "record",
            List(vec![
                Map(vec![(
                    "enabled",
                    Map(vec![("type", Text("bool")), ("default", Bool(true))]),
                )]),
                Map(vec![(
                    "name",
                    Map(vec![("type", Text("str")), ("required", Bool(true))]),
                )]),
                field("optional", "int"),
            ]),
        )],
    );
    let result = normalize(&schema, "item", Text("DM"), false);
    assert_value(
        &result.document,
        Map(vec![("name", Text("DM")), ("enabled", Bool(true))]),
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
        &normalize(&schema, "item", Text("DM"), true).document,
        Map(vec![("name", Text("DM"))]),
    );
    assert_value(
        &normalize(&schema, "item", Map(vec![("name", Text("DM"))]), false).document,
        Map(vec![("enabled", Bool(true)), ("name", Text("DM"))]),
    );
    assert_value(
        &normalize(&schema, "item", Map(vec![]), true).document,
        Map(vec![]),
    );
}

#[test]
fn derivation_calls_redispatch_through_registry_and_class_while_case_results_remain_sources() {
    let schema = bundle(
        vec![
            field("derive", "derivation"),
            field("result", "case_result"),
        ],
        vec![
            (
                "derivation",
                Map(vec![(
                    "type",
                    List(vec![Text("str"), Text("expression"), Text("handled")]),
                )]),
            ),
            (
                "case_result",
                Map(vec![("type", List(vec![Text("str"), Text("expression")]))]),
            ),
            ("expression", Map(vec![("registry", Text("operations"))])),
            (
                "handled",
                List(vec![
                    Map(vec![(
                        "value",
                        Map(vec![("type", Text("expression")), ("required", Bool(true))]),
                    )]),
                    Map(vec![(
                        "fallback",
                        Map(vec![("type", Text("int")), ("default", Int("0"))]),
                    )]),
                ]),
            ),
            (
                "operations",
                Map(vec![
                    ("source", descriptor("str")),
                    ("to_number", descriptor("expression")),
                ]),
            ),
        ],
    );
    assert_value(
        &normalize(&schema, "derive", Text("DM.AGE"), false).document,
        Map(vec![
            ("value", Map(vec![("source", Text("DM.AGE"))])),
            ("fallback", Int("0")),
        ]),
    );
    assert_value(
        &normalize(&schema, "derive", Text("  to_number( DM.AGE )  "), false).document,
        Map(vec![
            (
                "value",
                Map(vec![("to_number", Map(vec![("source", Text("DM.AGE"))]))]),
            ),
            ("fallback", Int("0")),
        ]),
    );
    assert_value(
        &normalize(&schema, "derive", Text("to_number(DM.AGE)"), true).document,
        Map(vec![(
            "value",
            Map(vec![("to_number", Map(vec![("source", Text("DM.AGE"))]))]),
        )]),
    );
    assert_value(
        &normalize(&schema, "result", Text("to_number(DM.AGE)"), false).document,
        Map(vec![("source", Text("to_number(DM.AGE)"))]),
    );
    for source in ["nested(call(DM.AGE))", "1invalid(DM.AGE)", "op (DM.AGE)"] {
        assert_value(
            &normalize(&schema, "derive", Text(source), false).document,
            Map(vec![
                ("value", Map(vec![("source", Text(source))])),
                ("fallback", Int("0")),
            ]),
        );
    }
}

#[test]
fn recursive_defaults_are_resource_refusals_instead_of_silently_truncated_objects() {
    let schema = bundle(
        vec![field("tree", "record")],
        vec![(
            "record",
            List(vec![Map(vec![(
                "child",
                Map(vec![("type", Text("record")), ("default", Map(vec![]))]),
            )])]),
        )],
    );
    let input = document(Map(vec![]));
    let descriptor = schema.root_class().fields[1].descriptor;
    let limits = NormalizationLimits {
        depth: 24,
        ..Default::default()
    };
    assert_eq!(
        schema.normalize_descriptor(
            descriptor,
            &input,
            input.root(),
            false,
            &mut NormalizationBudget::new(limits)
        ),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Depth,
            limit: 24
        })
    );
    assert_value(
        &normalize(&schema, "tree", Map(vec![]), true).document,
        Map(vec![]),
    );
}

#[test]
fn normalization_validates_written_constraints_and_defaults_before_allocating_output() {
    let schema = bundle(
        vec![Map(vec![(
            "name",
            Map(vec![
                ("type", Text("str")),
                ("values", List(vec![Text("ok")])),
            ]),
        )])],
        vec![],
    );
    let input = document(Text("bad"));
    let mut budget = NormalizationBudget::new(NormalizationLimits::default());
    assert!(matches!(
        schema.normalize_descriptor(
            schema.root_class().fields[1].descriptor,
            &input,
            input.root(),
            false,
            &mut budget
        ),
        Err(NormalizationError::Invalid(_))
    ));
    assert_eq!(budget.storage_used(), (0, 0, 0));
    let invalid_defaults = bundle(
        vec![Map(vec![(
            "bad",
            Map(vec![("type", Text("str")), ("default", Int("1"))]),
        )])],
        vec![],
    );
    assert!(matches!(
        invalid_defaults.normalize_descriptor(
            invalid_defaults.root_class().fields[1].descriptor,
            &document(Text("valid")),
            0,
            true,
            &mut budget
        ),
        Err(NormalizationError::Defaults(_))
    ));
    assert_eq!(budget.storage_used(), (0, 0, 0));
}

#[test]
fn normalization_storage_limits_span_calls_without_mutating_sources() {
    let schema = bundle(
        vec![Map(vec![(
            "value",
            Map(vec![("type", List(vec![Text("str"), Text("list[str]")]))]),
        )])],
        vec![],
    );
    let input = document(Text("DM"));
    let before = input.clone();
    let descriptor = schema.root_class().fields[1].descriptor;
    let limits = NormalizationLimits {
        storage: DocumentLimits {
            nodes: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut shared = NormalizationBudget::new(limits);
    assert!(schema
        .normalize_descriptor(descriptor, &input, input.root(), false, &mut shared)
        .is_ok());
    assert_eq!(shared.storage_used(), (2, 2, 1));
    assert_eq!(
        schema.normalize_descriptor(descriptor, &input, input.root(), false, &mut shared),
        Err(NormalizationError::Limit {
            resource: NormalizationResource::Nodes,
            limit: 2
        })
    );
    assert_eq!(input, before);
    assert!(schema
        .normalize_descriptor(
            descriptor,
            &input,
            input.root(),
            false,
            &mut NormalizationBudget::new(limits)
        )
        .is_ok());
    for (storage, resource) in [
        (
            DocumentLimits {
                text_bytes: 0,
                ..Default::default()
            },
            NormalizationResource::TextBytes,
        ),
        (
            DocumentLimits {
                edges: 0,
                ..Default::default()
            },
            NormalizationResource::Edges,
        ),
    ] {
        let limits = NormalizationLimits {
            storage,
            ..Default::default()
        };
        assert_eq!(
            schema.normalize_descriptor(
                descriptor,
                &input,
                input.root(),
                false,
                &mut NormalizationBudget::new(limits)
            ),
            Err(NormalizationError::Limit { resource, limit: 0 })
        );
    }
}

#[test]
fn whole_document_normalization_keeps_schema_order_and_nested_default_provenance() {
    let schema = bundle(
        vec![
            Map(vec![(
                "groups",
                Map(vec![
                    ("type", Text("dict[str, list[record]]")),
                    (
                        "default",
                        Map(vec![("DM", List(vec![Map(vec![("name", Text("AGE"))])]))]),
                    ),
                ]),
            )]),
            field("explicit", "str"),
        ],
        vec![(
            "record",
            List(vec![
                field("name", "str"),
                Map(vec![(
                    "enabled",
                    Map(vec![("type", Text("bool")), ("default", Bool(true))]),
                )]),
            ]),
        )],
    );
    let input = document(Map(vec![
        ("explicit", Text("written")),
        ("schema_version", Text("1.0")),
    ]));
    let result = schema
        .normalize_document(
            &input,
            &mut NormalizationBudget::new(NormalizationLimits::default()),
        )
        .unwrap();
    assert_value(
        &result.document,
        Map(vec![
            ("schema_version", Text("1.0")),
            (
                "groups",
                Map(vec![(
                    "DM",
                    List(vec![Map(vec![
                        ("name", Text("AGE")),
                        ("enabled", Bool(true)),
                    ])]),
                )]),
            ),
            ("explicit", Text("written")),
        ]),
    );
    assert_eq!(result.origins.len(), result.document.nodes().len());
    for (node, origin) in result.document.nodes().iter().zip(&result.origins) {
        if matches!(node, N::Text(text) if text == "AGE") || matches!(node, N::Boolean(true)) {
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
        Map(vec![("DM", List(vec![Map(vec![("name", Text("AGE"))])]))]),
        true,
    );
    assert_value(
        &fragment.document,
        Map(vec![("DM", List(vec![Map(vec![("name", Text("AGE"))])]))]),
    );
    assert!(fragment
        .origins
        .iter()
        .all(|origin| origin.source == SchemaSource::Input));
}

#[test]
fn shorthand_checks_the_written_member_without_reinterpreting_generated_fields() {
    let schema = bundle(
        vec![Map(vec![(
            "item",
            Map(vec![("type", List(vec![Text("str"), Text("record")]))]),
        )])],
        vec![(
            "record",
            List(vec![Map(vec![(
                "name",
                Map(vec![
                    ("type", Text("str")),
                    ("required", Bool(true)),
                    ("values", List(vec![Text("canonical-only")])),
                ]),
            )])]),
        )],
    );
    assert_value(
        &normalize(&schema, "item", Text("written-string"), false).document,
        Map(vec![("name", Text("written-string"))]),
    );
    let input = document(Map(vec![("name", Text("written-string"))]));
    let result = schema.normalize_descriptor(
        schema.root_class().fields[1].descriptor,
        &input,
        input.root(),
        false,
        &mut NormalizationBudget::new(NormalizationLimits::default()),
    );
    assert!(
        matches!(result, Err(NormalizationError::Invalid(ref findings)) if findings[0].condition == "value_not_permitted")
    );
}

#[test]
fn malformed_compound_names_and_ids_remain_ordered_validation_findings() {
    let schema = bundle(
        vec![field("items", "list[record]")],
        vec![(
            "record",
            List(vec![field("name", "str"), field("id", "int")]),
        )],
    );
    let result = validate(
        &schema,
        "items",
        List(vec![
            Map(vec![(
                "name",
                List(vec![Text("DM"), Int("2"), Bool(true), Null, Float(-0.0)]),
            )]),
            Map(vec![(
                "id",
                Map(vec![("inner", List(vec![Text("value")]))]),
            )]),
            Map(vec![("name", List(vec![])), ("id", Int("10"))]),
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
            ("actual", C::Text("sequence".into()))
        ]
    );
}

#[test]
fn compound_path_quotes_escape_only_the_required_characters_without_normalization() {
    let schema = bundle(
        vec![field("items", "list[record]")],
        vec![("record", List(vec![field("name", "str")]))],
    );
    let result = validate(
        &schema,
        "items",
        List(vec![Map(vec![(
            "name",
            List(vec![
                Text("single'quote"),
                Text("both'\"quotes"),
                Text("\\\n\r\t\0"),
                Text("\u{e9} e\u{301} \u{1f600}"),
                Text("\u{a0}\u{2028}\u{202e}\u{e000}\u{0378}\u{10ffff}"),
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
fn compound_path_allocation_is_charged_before_large_invalid_labels_are_built() {
    let schema = bundle(
        vec![field("items", "list[record]")],
        vec![("record", List(vec![field("name", "str")]))],
    );
    let input = document(List(vec![Map(vec![(
        "name",
        List(vec![Text("oversized-label")]),
    )])]));
    let descriptor = schema.root_class().fields[1].descriptor;
    let limits = ValidationLimits {
        diagnostic_text_bytes: 5,
        ..Default::default()
    };
    assert_eq!(
        schema.validate_descriptor(
            descriptor,
            &input,
            input.root(),
            "items",
            false,
            &mut ValidationBudget::new(limits)
        ),
        Err(ValidationError::DiagnosticText { limit: 5 })
    );
    assert_eq!(
        schema
            .validate_descriptor(
                descriptor,
                &input,
                input.root(),
                "items",
                false,
                &mut budget()
            )
            .unwrap()[0]
            .condition,
        "invalid_field_type"
    );
}
