use yamaa_core::schema::{
    admit_schema_diagnostics, schema_diagnostic_kind_label, BundleLimits, Document,
    DocumentKind as K, DocumentLimits, DocumentNode as N, NormalizationBudget, NormalizationLimits,
    SchemaContext as C, SchemaDiagnostic, SchemaDiagnosticBudget as Budget,
    SchemaDiagnosticError as E, SchemaDiagnosticReport, SchemaDiagnosticValue as V, SchemaModule,
    SchemaSource, SchemaStructure, ValidationBudget, ValidationLimits,
};

const BIG: &str = "922337203685477580812345678901234567890";
enum Authored {
    Text(&'static str),
    Int(&'static str),
    Bool(bool),
    List(Vec<Authored>),
    Map(Vec<(&'static str, Authored)>),
}
use Authored::{Bool, Int, List, Map, Text};
fn document(value: Authored) -> Document {
    fn push(value: Authored, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(text) => N::Text(text.into()),
            Int(text) => N::Integer(text.into()),
            Bool(value) => N::Boolean(value),
            List(values) => N::Sequence(values.into_iter().map(|v| push(v, nodes)).collect()),
            Map(values) => N::Mapping(
                values
                    .into_iter()
                    .map(|(k, v)| (push(Text(k), nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
    let mut nodes = Vec::new();
    let root = push(value, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn schema() -> SchemaStructure {
    let shared = SchemaModule {
        name: "schema_shared.yaml".into(),
        document: document(Map(vec![
            ("version", Text("1.0")),
            (
                "shared",
                List(vec![Map(vec![(
                    "value",
                    Map(vec![
                        ("type", Text("str")),
                        ("values", List(vec![Text("z"), Text("a"), Text("z")])),
                        ("pattern", Text("^OK$")),
                        ("min_length", Int(BIG)),
                        ("default", Text("bad\"\nλ")),
                    ]),
                )])]),
            ),
        ])),
    };
    let root = SchemaModule {
        name: "schema.yaml".into(),
        document: document(Map(vec![
            ("version", Text("1.0")),
            ("includes", List(vec![Text("schema_shared.yaml")])),
            (
                "root_class",
                List(vec![
                    Map(vec![("fields_from", Text("shared"))]),
                    Map(vec![(
                        "items",
                        Map(vec![("type", Text("list[int]")), ("size", Int(BIG))]),
                    )]),
                    Map(vec![(
                        "required",
                        Map(vec![("type", Text("bool")), ("required", Bool(true))]),
                    )]),
                ]),
            ),
        ])),
    };
    SchemaStructure::admit(vec![shared, root], 1, "root_class", BundleLimits::default()).unwrap()
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
                finding.requirement,
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
    let input = document(Text("bad\"\nλ"));
    let findings = schema
        .validate_descriptor(
            descriptor,
            &input,
            input.root(),
            "value",
            false,
            &mut ValidationBudget::new(ValidationLimits::default()),
        )
        .unwrap();
    let origins = [("source", "held/parent.yaml"), ("entry", "held/entry.yaml")];
    let report = admit_schema_diagnostics(
        &schema,
        Some(&input),
        &findings,
        &origins,
        &mut Budget::new(),
    )
    .unwrap();
    assert!(core::ptr::eq(report.schema(), &schema));
    assert!(core::ptr::eq(report.document().unwrap(), &input));
    assert_eq!(report.findings().as_ptr(), findings.as_ptr());
    assert_eq!(report.source_context().as_ptr(), origins.as_ptr());
    assert_eq!(
        records(&report),
        vec![
            (
                "validation",
                "value",
                "value_not_permitted",
                Some("REQ-0287"),
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
                Some("REQ-0287"),
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
                Some("REQ-0287"),
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

    let items = document(List(vec![Int("1")]));
    let findings = schema
        .validate_descriptor(
            schema.root_class().fields[1].descriptor,
            &items,
            items.root(),
            "items",
            false,
            &mut ValidationBudget::new(ValidationLimits::default()),
        )
        .unwrap();
    let report =
        admit_schema_diagnostics(&schema, Some(&items), &findings, &[], &mut Budget::new())
            .unwrap();
    assert_eq!(
        records(&report),
        vec![(
            "validation",
            "items",
            "invalid_size",
            Some("REQ-0287"),
            vec![("size", V::Integer(BIG))]
        )]
    );
}

#[test]
fn default_references_use_the_explicit_held_declaring_module_arena() {
    let schema = schema();
    let defaults = schema
        .validate_defaults(&mut ValidationBudget::new(ValidationLimits::default()))
        .unwrap();
    let descriptor = schema.root_class().fields[0].descriptor;
    let diagnostics = &defaults
        .iter()
        .find(|d| d.descriptor == descriptor)
        .unwrap()
        .diagnostics;
    let located = &schema.descriptors()[descriptor];
    let input = &schema.modules()[located.module].document;
    let report =
        admit_schema_diagnostics(&schema, Some(input), diagnostics, &[], &mut Budget::new())
            .unwrap();
    assert_eq!(
        records(&report),
        vec![
            (
                "validation",
                "root_class.value.default",
                "value_not_permitted",
                Some("REQ-0287"),
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
                Some("REQ-0287"),
                vec![("value", V::Text("bad\"\nλ")), ("pattern", V::Text("^OK$")),]
            ),
            (
                "validation",
                "root_class.value.default",
                "minimum_length",
                Some("REQ-0287"),
                vec![("minimum", V::Integer(BIG))]
            ),
        ]
    );
    let Err(error) = admit_schema_diagnostics(&schema, None, diagnostics, &[], &mut Budget::new())
    else {
        panic!("arena required")
    };
    assert_eq!(error, E::InvalidContext);
    let other = document(Text("unrelated source"));
    let Err(error) =
        admit_schema_diagnostics(&schema, Some(&other), diagnostics, &[], &mut Budget::new())
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
        let input = Document::new(vec![node], 0, DocumentLimits::default()).unwrap();
        let findings = [issue(
            "scalar",
            vec![
                ("value", C::InputValue(0)),
                ("count", C::Count(usize::MAX)),
                ("absent", C::Null),
            ],
        )];
        let report =
            admit_schema_diagnostics(&schema, Some(&input), &findings, &[], &mut Budget::new())
                .unwrap();
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
                Some("REQ-0287"),
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
    for (kind, value, layer) in [
        (K::Null, "null", "NoneType"),
        (K::Boolean, "bool", "bool"),
        (K::Integer, "int", "int"),
        (K::Float, "float", "float"),
        (K::Text, "str", "str"),
        (K::Sequence, "sequence", "list"),
        (K::Mapping, "mapping", "dict"),
    ] {
        let findings = [issue(
            "kind",
            vec![("value", C::ValueKind(kind)), ("layer", C::LayerKind(kind))],
        )];
        let report =
            admit_schema_diagnostics(&schema, None, &findings, &[], &mut Budget::new()).unwrap();
        assert_eq!(
            records(&report),
            vec![(
                "validation",
                "kind",
                "pattern_mismatch",
                Some("REQ-0287"),
                vec![("value", V::Text(value)), ("layer", V::Text(layer))]
            )]
        );
        assert_eq!(
            schema_diagnostic_kind_label(&C::ValueKind(kind)),
            Some(value)
        );
        assert_eq!(
            schema_diagnostic_kind_label(&C::LayerKind(kind)),
            Some(layer)
        );
    }
    assert_eq!(schema_diagnostic_kind_label(&C::Null), None);
}

#[test]
fn normalized_default_occurrences_keep_the_pass_arena_and_original_origin_authority() {
    let schema = SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", Text("1.0")),
                (
                    "root_class",
                    List(vec![
                        Map(vec![("schema_version", Map(vec![("type", Text("str"))]))]),
                        Map(vec![(
                            "value",
                            Map(vec![("type", Text("str")), ("default", Text("OK"))]),
                        )]),
                        Map(vec![(
                            "constraint",
                            Map(vec![
                                ("type", Text("str")),
                                ("values", List(vec![Text("forbidden")])),
                            ]),
                        )]),
                    ]),
                ),
            ])),
        }],
        0,
        "root_class",
        BundleLimits::default(),
    )
    .unwrap();
    let raw = document(Map(vec![("schema_version", Text("1.0"))]));
    let normalized = schema
        .normalize_document(
            &raw,
            &mut NormalizationBudget::new(NormalizationLimits::default()),
        )
        .unwrap();
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
            &mut ValidationBudget::new(ValidationLimits::default()),
        )
        .unwrap();
    let report = admit_schema_diagnostics(
        &schema,
        Some(&normalized.document),
        &findings,
        &[],
        &mut Budget::new(),
    )
    .unwrap();
    assert!(core::ptr::eq(
        report.document().unwrap(),
        &normalized.document
    ));
    assert_eq!(
        records(&report),
        vec![(
            "validation",
            "value",
            "value_not_permitted",
            Some("REQ-0287"),
            vec![
                ("value", V::Text("OK")),
                ("permitted", V::TextList(&["forbidden".into()]))
            ]
        )]
    );
    assert_eq!(normalized.origins.as_ptr(), origin_pointer);
    let Err(error) =
        admit_schema_diagnostics(&schema, Some(&raw), &findings, &[], &mut Budget::new())
    else {
        panic!("raw arena cannot resolve normalized occurrence")
    };
    assert_eq!(error, E::InvalidContext);
}

#[test]
fn invalid_nodes_descriptors_and_duplicate_keys_refuse_without_a_partial_report() {
    let schema = schema();
    let input = document(List(vec![Text("value")]));
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
        let Err(error) =
            admit_schema_diagnostics(&schema, Some(&input), &findings, &[], &mut Budget::new())
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
        let Err(error) =
            admit_schema_diagnostics(&schema, None, &findings, &origins, &mut Budget::new())
        else {
            panic!("duplicate context")
        };
        assert_eq!(error, E::InvalidContext);
    }
}

#[test]
fn whole_report_and_reused_budget_admission_preserve_exact_quota_and_error_order() {
    let schema = schema();
    let findings = [SchemaDiagnostic {
        path: "p".into(),
        code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
        context: vec![("k", C::Text("v".into()))],
    }];
    // Independent cost: 128 initial + 128 record + 8 path + 110 condition +
    // 50 requirement + 8 context name + 8 value + 2 context field.
    let total = 442;
    let mut budget = Budget::with_limit(total);
    let report = admit_schema_diagnostics(&schema, None, &findings, &[], &mut budget).unwrap();
    assert_eq!(
        records(&report),
        vec![(
            "validation",
            "p",
            "invalid_field_type",
            Some("REQ-0287"),
            vec![("k", V::Text("v"))]
        )]
    );
    assert_eq!(budget.bytes_used(), total);
    let Err(error) = admit_schema_diagnostics(&schema, None, &findings, &[], &mut budget) else {
        panic!("shared budget exhausted")
    };
    assert_eq!(error, E::Limit);
    let Err(error) = admit_schema_diagnostics(
        &schema,
        None,
        &findings,
        &[],
        &mut Budget::with_limit(total - 1),
    ) else {
        panic!("one byte short")
    };
    assert_eq!(error, E::Limit);
    let mut late = findings.to_vec();
    late.extend(findings.iter().cloned());
    let Err(error) =
        admit_schema_diagnostics(&schema, None, &late, &[], &mut Budget::with_limit(total))
    else {
        panic!("whole report refused")
    };
    assert_eq!(error, E::Limit);
    let invalid = [issue("bad", vec![("value", C::InputValue(0))])];
    let Err(error) =
        admit_schema_diagnostics(&schema, None, &invalid, &[], &mut Budget::with_limit(0))
    else {
        panic!("quota first")
    };
    assert_eq!(error, E::Limit);
    let Err(error) = admit_schema_diagnostics(&schema, None, &invalid, &[], &mut Budget::new())
    else {
        panic!("missing arena")
    };
    assert_eq!(error, E::InvalidContext);
    assert_eq!(
        admit_schema_diagnostics(&schema, None, &[], &[], &mut Budget::with_limit(0))
            .unwrap()
            .iter()
            .len(),
        0
    );
}
