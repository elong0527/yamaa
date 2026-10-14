//! Independent logical truth and physical codec effects for live metadata preparation.
use std::rc::Rc;
use yamaa_core::schema::{
    BundleLimits, ConstraintError, Document, DocumentLimits, DocumentNode as N,
    NormalizationBudget, NormalizationError, NormalizationLimits, NormalizationResource,
    SchemaContext as C, SchemaDiagnostic, SchemaModule, SchemaOrigin, SchemaSource,
    SchemaStructure, ValidationError, ValidationLimits,
};
use yamaa_engine::{
    project_preparation::Source,
    structural_preparation::{self as application, DecodePort, Decoded, Error, Limits, Selection},
};

#[derive(Clone)]
enum V {
    Text(&'static str),
    Int(&'static str),
    Bool(bool),
    List(Vec<V>),
    Map(Vec<(&'static str, V)>),
}
use V::{Bool, Int, List, Map, Text};
fn document(value: V) -> Document {
    fn push(value: V, nodes: &mut Vec<N>) -> usize {
        let node = match value {
            Text(s) => N::Text(s.into()),
            Int(s) => N::Integer(s.into()),
            Bool(v) => N::Boolean(v),
            List(items) => N::Sequence(items.into_iter().map(|v| push(v, nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(Text(k), nodes), push(v, nodes)))
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
#[derive(Debug, PartialEq)]
enum Logical {
    Text(String),
    Int(String),
    Bool(bool),
    List(Vec<Logical>),
    Map(Vec<(String, Logical)>),
}
fn logical(d: &Document, id: usize) -> Logical {
    match &d.nodes()[id] {
        N::Text(v) => Logical::Text(v.clone()),
        N::Integer(v) => Logical::Int(v.clone()),
        N::Boolean(v) => Logical::Bool(*v),
        N::Sequence(items) => Logical::List(items.iter().map(|&id| logical(d, id)).collect()),
        N::Mapping(items) => Logical::Map(
            items
                .iter()
                .map(|&(k, v)| {
                    let N::Text(k) = &d.nodes()[k] else { panic!() };
                    (k.clone(), logical(d, v))
                })
                .collect(),
        ),
        _ => panic!(),
    }
}
fn field(name: &'static str, kind: &'static str, required: bool) -> V {
    Map(vec![(
        name,
        Map(vec![("type", Text(kind)), ("required", Bool(required))]),
    )])
}
fn schema(bad_default: bool) -> SchemaStructure {
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(Map(vec![
                ("version", Text("1.0")),
                (
                    "environment_class",
                    List(vec![
                        field("schema_version", "str", true),
                        field("language", "str", false),
                    ]),
                ),
                (
                    "function_definition_class",
                    List(vec![
                        field("function", "str", true),
                        field("returns", "str", true),
                        Map(vec![(
                            "flag",
                            Map(vec![
                                ("type", Text("bool")),
                                ("default", if bad_default { Int("7") } else { Bool(false) }),
                            ]),
                        )]),
                    ]),
                ),
                (
                    "versioned_other",
                    List(vec![field("schema_version", "str", false)]),
                ),
                (
                    "function_alias",
                    Map(vec![("type", Text("function_definition_class"))]),
                ),
            ])),
        }],
        0,
        "environment_class",
        BundleLimits::default(),
    )
    .unwrap()
}
fn source() -> Source {
    Source {
        identity: "held/function.yaml".into(),
        bytes: b"original captured representation".to_vec(),
    }
}
fn function() -> Document {
    document(Map(vec![
        ("function", Text("program.constant")),
        ("returns", Text("int")),
    ]))
}
// Intentionally no Clone/Debug/Send/Sync: the actual allocation must move through
// the new application rather than be interpreted, copied or formatted.
struct Opaque {
    value: Box<str>,
    _thread: Rc<()>,
}
struct Codec {
    reply: Option<Result<Decoded<Vec<usize>>, Opaque>>,
    calls: Vec<(usize, usize)>,
}
impl Codec {
    fn success(document: Document) -> Self {
        Self {
            reply: Some(Ok(Decoded {
                locations: (0..document.nodes().len()).collect(),
                document,
            })),
            calls: vec![],
        }
    }
    fn raised(value: Opaque) -> Self {
        Self {
            reply: Some(Err(value)),
            calls: vec![],
        }
    }
}
impl DecodePort for Codec {
    type Error = Opaque;
    type Locations = Vec<usize>;
    fn decode(&mut self, source: &Source) -> Result<Decoded<Vec<usize>>, Opaque> {
        self.calls.push((
            source.bytes.as_ptr() as usize,
            source.identity.as_ptr() as usize,
        ));
        self.reply.take().expect("codec must execute once")
    }
}
fn unwrap<T>(value: Result<T, Error<Opaque>>) -> T {
    value.unwrap_or_else(|_| panic!("unexpected preparation failure"))
}
fn unknown(path: &str, name: &str, class: &str) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: path.into(),
        code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownField,
        context: vec![
            ("field", C::Text(name.into())),
            ("class", C::Text(class.into())),
        ],
    }
}

#[test]
fn named_guards_precede_decode_source_admission_defaults_and_every_work_charge() {
    let schema = schema(true);
    let limits = Limits {
        class_name_bytes: 32,
        normalization: NormalizationLimits {
            validation: ValidationLimits {
                work: 0,
                diagnostics: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    };
    for name in ["missing", "str", "function_alias", ""] {
        let mut codec = Codec::success(function());
        let error = application::prepare(
            &schema,
            source(),
            Selection::NamedClass(name),
            limits,
            &mut codec,
        );
        assert!(matches!(error,Err(Error::UnknownClass(actual)) if actual==name));
        assert!(codec.calls.is_empty());
        assert!(codec.reply.is_some());
    }
    let mut codec = Codec::success(function());
    let wide = "X".repeat(33);
    assert!(matches!(
        application::prepare(
            &schema,
            source(),
            Selection::NamedClass(&wide),
            limits,
            &mut codec
        ),
        Err(Error::ClassNameLimit)
    ));
    assert!(codec.calls.is_empty());
    for name in ["environment_class", "versioned_other"] {
        let mut codec = Codec::success(function());
        assert!(matches!(
            application::prepare(
                &schema,
                source(),
                Selection::NamedClass(name),
                limits,
                &mut codec
            ),
            Err(Error::VersionedRootRequired)
        ));
        assert!(codec.calls.is_empty());
    }
}

#[test]
fn physical_codec_runs_once_and_original_opaque_error_precedes_query_and_defaults() {
    let schema = schema(true);
    for selection in [
        Selection::StructuralRoot,
        Selection::NamedClass("function_definition_class"),
    ] {
        let value = Opaque {
            value: "original opaque codec failure".into(),
            _thread: Rc::new(()),
        };
        let pointer = value.value.as_ptr();
        let entry = source();
        let bytes = entry.bytes.as_ptr() as usize;
        let identity = entry.identity.as_ptr() as usize;
        let mut codec = Codec::raised(value);
        let limits = Limits {
            normalization: NormalizationLimits {
                validation: ValidationLimits {
                    work: 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let Err(Error::Decode(original)) =
            application::prepare(&schema, entry, selection, limits, &mut codec)
        else {
            panic!()
        };
        assert_eq!(original.value.as_ptr(), pointer);
        assert_eq!(&*original.value, "original opaque codec failure");
        assert_eq!(codec.calls, [(bytes, identity)]);
        assert!(codec.reply.is_none());
    }
}

#[test]
fn success_moves_every_original_allocation_and_complete_default_origin_vector() {
    let schema = schema(false);
    let entry = source();
    let bytes = entry.bytes.as_ptr();
    let identity = entry.identity.as_ptr();
    let raw = function();
    let expected_raw = raw.clone();
    let raw_pointer = raw.nodes().as_ptr();
    let mut codec = Codec::success(raw);
    let location_pointer = codec
        .reply
        .as_ref()
        .unwrap()
        .as_ref()
        .ok()
        .unwrap()
        .locations
        .as_ptr();
    let result = unwrap(application::prepare(
        &schema,
        entry,
        Selection::NamedClass("function_definition_class"),
        Limits::default(),
        &mut codec,
    ));
    assert_eq!(result.source().bytes.as_ptr(), bytes);
    assert_eq!(result.source().identity.as_ptr(), identity);
    assert_eq!(result.raw().nodes().as_ptr(), raw_pointer);
    assert_eq!(result.locations().as_ptr(), location_pointer);
    assert_eq!(result.raw(), &expected_raw);
    assert_eq!(result.class_name(), "function_definition_class");
    assert_eq!(codec.calls.len(), 1);
    let expected = document(Map(vec![
        ("function", Text("program.constant")),
        ("returns", Text("int")),
        ("flag", Bool(false)),
    ]));
    let normalized = &result.normalized().document;
    assert_eq!(
        logical(normalized, normalized.root()),
        logical(&expected, expected.root())
    );
    assert!(normalized
        .field(normalized.root(), "schema_version")
        .is_none());
    let descriptor = schema
        .class_named("function_definition_class")
        .unwrap()
        .fields
        .iter()
        .find(|f| f.name == "flag")
        .unwrap()
        .descriptor;
    let default = schema.descriptors()[descriptor]
        .descriptor
        .default_node()
        .unwrap();
    let origin = SchemaOrigin {
        source: SchemaSource::Default {
            module: 0,
            descriptor,
        },
        node: default,
        generated: false,
    };
    // Complete independently declared origin truth, aligned to independently
    // checked output nodes. Authored pair IDs are 0..3 and root ID is 4.
    let input = |node| SchemaOrigin {
        source: SchemaSource::Input,
        node,
        generated: false,
    };
    assert_eq!(
        result.normalized().origins,
        [
            input(0),
            input(1),
            input(2),
            input(3),
            origin,
            SchemaOrigin {
                generated: true,
                ..origin
            },
            input(4)
        ]
    );
    let (source, raw, locations, class_name, normalized) = result.into_parts();
    assert_eq!(source.bytes.as_ptr(), bytes);
    assert_eq!(source.identity.as_ptr(), identity);
    assert_eq!(raw.nodes().as_ptr(), raw_pointer);
    assert_eq!(locations.as_ptr(), location_pointer);
    assert_eq!(class_name, "function_definition_class");
    assert_eq!(normalized.origins.len(), 7);
}

#[test]
fn semantic_failures_move_raw_arena_and_full_ordered_findings_without_partial_output() {
    let schema = schema(false);
    for (selection, raw, expected) in [
        (
            Selection::StructuralRoot,
            document(Map(vec![
                ("schema_version", Int("92233720368547758081234567890")),
                ("extra", Text("late")),
            ])),
            vec![SchemaDiagnostic {
                path: "schema_version".into(),
                code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                context: vec![
                    ("expected", C::Text("1.0".into())),
                    ("actual", C::InputValue(1)),
                ],
            }],
        ),
        (
            Selection::NamedClass("function_definition_class"),
            document(Map(vec![
                ("function", Text("program.constant")),
                ("returns", Text("int")),
                ("schema_version", Text("1.0")),
                ("second", Text("extra")),
            ])),
            vec![
                unknown(
                    "<normalization>.schema_version",
                    "schema_version",
                    "function_definition_class",
                ),
                unknown(
                    "<normalization>.second",
                    "second",
                    "function_definition_class",
                ),
            ],
        ),
    ] {
        let pointer = raw.nodes().as_ptr();
        let raw_expected = raw.clone();
        let entry = source();
        let source_pointer = entry.bytes.as_ptr();
        let identity = entry.identity.as_ptr();
        let mut codec = Codec::success(raw);
        let Err(Error::Findings(held)) =
            application::prepare(&schema, entry, selection, Limits::default(), &mut codec)
        else {
            panic!()
        };
        assert_eq!(held.document().nodes().as_ptr(), pointer);
        assert_eq!(held.document(), &raw_expected);
        assert_eq!(held.source().bytes.as_ptr(), source_pointer);
        assert_eq!(held.source().identity.as_ptr(), identity);
        assert_eq!(held.findings(), expected);
        assert_eq!(codec.calls.len(), 1);
        let (source, document, findings) = held.into_parts();
        assert_eq!(source.bytes.as_ptr(), source_pointer);
        assert_eq!(document.nodes().as_ptr(), pointer);
        assert_eq!(findings, expected);
    }
}

#[test]
fn structural_root_has_no_named_name_guard_and_never_admits_a_model_or_window() {
    let schema = schema(false);
    let raw = document(Map(vec![
        ("schema_version", Text("1.0")),
        ("language", Text("python")),
    ]));
    let expected = raw.clone();
    let pointer = raw.nodes().as_ptr();
    let mut codec = Codec::success(raw);
    let result = unwrap(application::prepare(
        &schema,
        source(),
        Selection::StructuralRoot,
        Limits {
            class_name_bytes: 0,
            ..Default::default()
        },
        &mut codec,
    ));
    assert_eq!(result.raw().nodes().as_ptr(), pointer);
    assert_eq!(result.class_name(), "environment_class");
    let normalized = &result.normalized().document;
    assert_eq!(
        logical(normalized, normalized.root()),
        logical(&expected, expected.root())
    );
    assert!(normalized.field(normalized.root(), "domain").is_none());
    assert_eq!(codec.calls.len(), 1);
    assert_eq!(
        result.normalized().origins,
        (0..expected.nodes().len())
            .map(|node| SchemaOrigin {
                source: SchemaSource::Input,
                node,
                generated: false
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn query_work_and_normalization_share_one_scope_and_keep_failure_categories() {
    let schema = schema(false);
    let class = "function_definition_class";
    // Independent closed single-name query has one type, one parsed node and
    // two scans of the 25-byte class name: 1+(len+1)+1+len = 53.
    let query_work = 2 * class.len() + 3;
    assert_eq!(query_work, 53);
    for work in [query_work - 1, query_work] {
        let mut codec = Codec::success(function());
        let limits = Limits {
            normalization: NormalizationLimits {
                validation: ValidationLimits {
                    work,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let error = application::prepare(
            &schema,
            source(),
            Selection::NamedClass(class),
            limits,
            &mut codec,
        );
        if work < query_work {
            assert!(
                matches!(error,Err(Error::Validation(ValidationError::Constraint(ConstraintError::Work{limit})) ) if limit==work)
            );
        } else {
            assert!(
                matches!(error,Err(Error::Normalize(NormalizationError::Validation(ValidationError::Constraint(ConstraintError::Work{limit})))) if limit==work)
            );
        }
        assert_eq!(codec.calls.len(), 1);
    }
    let mut admitted = NormalizationBudget::new(NormalizationLimits::default());
    let types = schema
        .parse_query_types(&[class.into()], admitted.validation_scope())
        .unwrap();
    schema
        .normalize_types(&types, &function(), function().root(), false, &mut admitted)
        .unwrap();
    let exact = admitted.work_used();
    // The preceding independently counted query boundary establishes phase
    // precedence; this measured inclusive total pins current policy compatibility.
    for work in [exact - 1, exact] {
        let mut codec = Codec::success(function());
        let limits = Limits {
            normalization: NormalizationLimits {
                validation: ValidationLimits {
                    work,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let result = application::prepare(
            &schema,
            source(),
            Selection::NamedClass(class),
            limits,
            &mut codec,
        );
        if work == exact {
            unwrap(result);
        } else {
            assert!(
                matches!(result,Err(Error::Normalize(NormalizationError::Validation(ValidationError::Constraint(ConstraintError::Work{limit})))) if limit==work)
            );
        }
    }
}

#[test]
fn defaults_keep_their_post_decode_post_query_checkpoint_and_original_typed_payload() {
    let schema = schema(true);
    for selection in [
        Selection::StructuralRoot,
        Selection::NamedClass("function_definition_class"),
    ] {
        let raw = document(Map(vec![("schema_version", Text("99.0"))]));
        let mut codec = Codec::success(raw);
        let Err(Error::Normalize(NormalizationError::Defaults(defaults))) =
            application::prepare(&schema, source(), selection, Limits::default(), &mut codec)
        else {
            panic!()
        };
        assert_eq!(defaults.len(), 1);
        assert_eq!(
            defaults[0].diagnostics,
            vec![SchemaDiagnostic {
                path: "function_definition_class.flag.default".into(),
                code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
                context: vec![
                    ("expected", C::Text("bool".into())),
                    (
                        "actual",
                        C::ValueKind(yamaa_core::schema::DocumentKind::Integer)
                    )
                ]
            }]
        );
        assert_eq!(codec.calls.len(), 1);
    }
    let mut codec = Codec::success(function());
    let limits = Limits {
        normalization: NormalizationLimits {
            validation: ValidationLimits {
                work: 0,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        application::prepare(
            &schema,
            source(),
            Selection::NamedClass("function_definition_class"),
            limits,
            &mut codec
        ),
        Err(Error::Validation(ValidationError::Constraint(
            ConstraintError::Work { limit: 0 }
        )))
    ));
}

#[test]
fn storage_and_late_whole_finding_limits_remain_explicit_refusals() {
    let schema = schema(false);
    let exact = document(Map(vec![
        ("function", Text("program.constant")),
        ("returns", Text("int")),
        ("flag", Bool(false)),
    ]));
    let bytes = exact
        .nodes()
        .iter()
        .map(|n| match n {
            N::Text(v) => v.len(),
            _ => 0,
        })
        .sum::<usize>();
    let edges = 6;
    for (resource, maximum) in [
        (NormalizationResource::Nodes, 7),
        (NormalizationResource::TextBytes, bytes),
        (NormalizationResource::Edges, edges),
    ] {
        for allowance in [maximum - 1, maximum] {
            let mut limits = Limits::default();
            match resource {
                NormalizationResource::Nodes => limits.normalization.storage.nodes = allowance,
                NormalizationResource::TextBytes => {
                    limits.normalization.storage.text_bytes = allowance
                }
                NormalizationResource::Edges => limits.normalization.storage.edges = allowance,
                _ => unreachable!(),
            };
            let mut codec = Codec::success(function());
            let result = application::prepare(
                &schema,
                source(),
                Selection::NamedClass("function_definition_class"),
                limits,
                &mut codec,
            );
            if allowance == maximum {
                unwrap(result);
            } else {
                assert!(
                    matches!(result,Err(Error::Normalize(NormalizationError::Limit{resource:actual,limit})) if actual==resource && limit==allowance)
                );
            }
            assert_eq!(codec.calls.len(), 1);
        }
    }
    let raw = document(Map(vec![
        ("schema_version", Text("1.0")),
        ("first", Text("extra")),
        ("second", Text("extra")),
    ]));
    let mut codec = Codec::success(raw);
    let limits = Limits {
        normalization: NormalizationLimits {
            validation: ValidationLimits {
                diagnostics: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        application::prepare(
            &schema,
            source(),
            Selection::StructuralRoot,
            limits,
            &mut codec
        ),
        Err(Error::Normalize(NormalizationError::Validation(
            ValidationError::Diagnostics { limit: 1 }
        )))
    ));
    assert_eq!(codec.calls.len(), 1);
}

#[test]
fn late_diagnostic_text_refusal_keeps_the_complete_failed_report_unavailable() {
    let schema = schema(false);
    let limits = Limits {
        normalization: NormalizationLimits {
            validation: ValidationLimits {
                diagnostic_text_bytes: 512,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let first = document(Map(vec![
        ("schema_version", Text("1.0")),
        ("first", Text("extra")),
    ]));
    let mut codec = Codec::success(first);
    let Err(Error::Findings(failure)) = application::prepare(
        &schema,
        source(),
        Selection::StructuralRoot,
        limits,
        &mut codec,
    ) else {
        panic!("the first complete finding fits")
    };
    assert_eq!(
        failure.findings(),
        &[unknown("first", "first", "environment_class")]
    );
    assert_eq!(codec.calls.len(), 1);
    // The later authored key alone exceeds the entire existing text allowance.
    // The first finding is fully admitted; the second refuses the whole report.
    let wide: &'static str = Box::leak("second".repeat(100).into_boxed_str());
    let both = document(Map(vec![
        ("schema_version", Text("1.0")),
        ("first", Text("extra")),
        (wide, Text("extra")),
    ]));
    let mut codec = Codec::success(both);
    assert!(matches!(
        application::prepare(
            &schema,
            source(),
            Selection::StructuralRoot,
            limits,
            &mut codec
        ),
        Err(Error::Normalize(NormalizationError::Validation(
            ValidationError::DiagnosticText { limit: 512 }
        )))
    ));
    assert_eq!(codec.calls.len(), 1);
}
