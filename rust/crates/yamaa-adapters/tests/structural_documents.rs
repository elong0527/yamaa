use std::sync::Arc;
use yamaa_adapters::{
    specification_source::{CapturedSchema, Error, Limits, Source},
    yaml_decode::{decode_yaml, DecodeLimits},
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
        N::Sequence(items) => {
            LogicalTree::Sequence(items.iter().map(|&id| logical_tree(document, id)).collect())
        }
        N::Mapping(items) => LogicalTree::Mapping(
            items
                .iter()
                .map(|&(key, value)| (logical_tree(document, key), logical_tree(document, value)))
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
fn modules() -> Vec<Source> {
    vec![
        source(
            "schema_environment.yaml",
            include_str!("fixtures/environment-candidate/schema_environment.yaml"),
        ),
        source(
            "schema_shared.yaml",
            include_str!("fixtures/environment-candidate/schema_shared.yaml"),
        ),
    ]
}
fn environment_schema(limits: Limits) -> Arc<CapturedSchema> {
    CapturedSchema::admit_root(modules(), 0, "environment_class", limits).unwrap()
}
fn finding(path: &str, condition: &'static str, field: &str, class: &str) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: path.into(),
        condition,
        requirement: None,
        context: vec![
            ("field", C::Text(field.into())),
            ("class", C::Text(class.into())),
        ],
    }
}
fn captured_findings(schema: &Arc<CapturedSchema>, text: &str) -> Vec<SchemaDiagnostic> {
    let Err(Error::Findings(result)) =
        schema.prepare_structural(source("spec/environment.yaml", text))
    else {
        panic!("expected complete structural findings");
    };
    assert_eq!(result.source().bytes, text.as_bytes());
    assert!(result.document().is_some());
    result.findings().to_vec()
}

#[test]
fn structural_environment_owns_schema_source_raw_tree_and_explicit_defaults() {
    let mut original = modules();
    let retained = original.iter().map(|s| s.bytes.clone()).collect::<Vec<_>>();
    let schema =
        CapturedSchema::admit_root(original.clone(), 0, "environment_class", Limits::default())
            .unwrap();
    for source in &mut original {
        source.bytes.clear();
        source.identity.clear();
    }
    for (module, bytes) in schema.sources().iter().zip(retained) {
        assert_eq!(module.bytes, bytes);
    }
    let text="schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions:\n  bmi:\n    function: projectbmi.bmi\n    description: Body mass index.\n    params: [{name: weight, type: float}]\n    returns: float\n    tests:\n      - {id: normal, covers: [normal], args: {weight: 70.0}, result: 70.0}\n";
    let captured = schema
        .prepare_structural(source("spec/environment.yaml", text))
        .unwrap();
    assert!(Arc::ptr_eq(captured.schema(), &schema));
    assert_eq!(captured.source().identity, "spec/environment.yaml");
    assert_eq!(captured.source().bytes, text.as_bytes());
    let raw = decode_yaml(text.as_bytes(), DecodeLimits::default()).unwrap();
    assert_eq!(captured.raw().document, raw.document);
    assert_eq!(captured.raw().locations, raw.locations);
    let expected="schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions:\n  bmi:\n    function: projectbmi.bmi\n    description: Body mass index.\n    params: [{name: weight, type: float, required: true, accepts_missing: false}]\n    returns: float\n    may_return_missing: false\n    comparison_decimals: 4\n    tests:\n      - {id: normal, covers: [normal], args: {weight: 70.0}, result: 70.0}\n";
    let expected = decode_yaml(expected.as_bytes(), DecodeLimits::default())
        .unwrap()
        .document;
    let actual = &captured.normalized().document;
    assert_eq!(
        logical_tree(actual, actual.root()),
        logical_tree(&expected, expected.root())
    );
    assert_eq!(captured.normalized().origins.len(), expected.nodes().len());
}

#[test]
fn environment_root_does_not_create_a_specification_or_require_domain_data() {
    let schema = environment_schema(Limits::default());
    let captured = schema
        .prepare_structural(source("environment.yaml", "schema_version: '1.0'\n"))
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

#[test]
fn environment_parents_and_retired_runtime_fields_are_complete_schema_findings() {
    let schema = environment_schema(Limits::default());
    assert_eq!(captured_findings(&schema,"schema_version: '1.0'\nparents: [base.yaml]\nruntime: {language: python}\nversion: old\n"),vec![
        finding("parents","unknown_field","parents","environment_class"),
        finding("runtime","unknown_field","runtime","environment_class"),
        finding("version","unknown_field","version","environment_class"),
    ]);
}

#[test]
fn version_priority_precedes_every_other_environment_finding() {
    let schema = environment_schema(Limits::default());
    assert_eq!(
        captured_findings(
            &schema,
            "schema_version: '99.0'\nparents: [base.yaml]\nruntime: {}\n"
        ),
        vec![SchemaDiagnostic {
            path: "schema_version".into(),
            condition: "schema_version_mismatch",
            requirement: None,
            context: vec![
                ("expected", C::Text("1.0".into())),
                ("actual", C::InputValue(1))
            ],
        }]
    );
}

#[test]
fn language_functions_and_submission_keep_closed_independent_root_shapes() {
    let schema = environment_schema(Limits::default());
    let text="schema_version: '1.0'\nlanguage: r\nlock: ../renv.lock\nfunctions: {bmi: functions/bmi.yaml}\ncodelists: [../ct.yaml]\nstudy: {id: STUDY01, name: STUDY01, description: Study, protocol_name: Study-01}\nadam:\n  standard: {name: ADaMIG, version: '1.3', status: Final}\n  specs: [adam/adsl.yaml]\n  define: ../data/adam/define.xml\n  documents: [{id: adrg, kind: supplemental, link: adrg.pdf, title: Review guide}]\n";
    let result = schema
        .prepare_structural(source("spec/environment.yaml", text))
        .unwrap();
    let doc = &result.normalized().document;
    let adam = doc.field(doc.root(), "adam").unwrap();
    assert_eq!(
        doc.nodes()[doc.field(adam, "dataset_json").unwrap()],
        N::Boolean(false)
    );
    assert_eq!(
        doc.nodes()[doc.field(adam, "context").unwrap()],
        N::Text("Submission".into())
    );
    assert_eq!(
        doc.nodes()[doc.field(adam, "xml_lang").unwrap()],
        N::Text("en".into())
    );
    assert_eq!(result.source().bytes, text.as_bytes());
}

#[test]
fn structural_capture_keeps_decode_failures_and_limits_distinct() {
    let schema = environment_schema(Limits::default());
    assert!(matches!(
        schema.prepare_structural(source("environment.yaml", "schema_version: [unclosed")),
        Err(Error::Decode { .. })
    ));
    let schema = environment_schema(Limits {
        captured_bytes: modules().iter().map(|s| s.bytes.len()).sum(),
        identity_bytes: 64,
        ..Limits::default()
    });
    assert!(matches!(
        schema.prepare_structural(source(&"x".repeat(65), "schema_version: '1.0'")),
        Err(Error::Limit("identity_bytes"))
    ));
    let bytes = "x".repeat(20000);
    assert!(matches!(
        schema.prepare_structural(source("environment.yaml", &bytes)),
        Err(Error::Limit("captured_bytes"))
    ));
}

#[test]
fn named_root_selection_is_bounded_and_absent_classes_fail_closed() {
    assert!(matches!(
        CapturedSchema::admit_root(
            modules(),
            0,
            &"x".repeat(17),
            Limits {
                identity_bytes: 16,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("root_class"))
    ));
    assert!(matches!(
        CapturedSchema::admit_root(modules(), 0, "missing_class", Limits::default()),
        Err(Error::Bundle(_))
    ));
    assert!(matches!(
        CapturedSchema::admit(modules(), 0, Limits::default()),
        Err(Error::Bundle(_))
    ));
}

#[test]
fn standalone_function_and_codelist_classes_keep_schema_version_in_schema_only() {
    for (class, text, expected) in [
        ("function_definition_class", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: []\n", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\nmay_return_missing: false\ncomparison_decimals: 4\ntests: []\n"),
        ("codelist_source_class", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\n", "codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}]}]\n"),
    ] {
        let schema=CapturedSchema::admit_root(modules(), 0, class, Limits::default()).unwrap();
        let captured=schema.prepare_class(source("independent.yaml", text)).unwrap();
        assert_eq!(captured.source().bytes, text.as_bytes());
        let actual=&captured.normalized().document;
        let expected=decode_yaml(expected.as_bytes(), DecodeLimits::default()).unwrap().document;
        assert_eq!(logical_tree(actual, actual.root()), logical_tree(&expected, expected.root()));
        assert_eq!(actual.field(actual.root(), "schema_version"), None);
        assert!(captured.normalized().origins.iter().any(|origin| matches!(origin.source, yamaa_core::schema::SchemaSource::Default {..})));
    }
}

#[test]
fn named_classes_reuse_the_environment_capture_without_fabricated_versions() {
    let schema = environment_schema(Limits::default());
    for (class, text, expected) in [
        ("function_definition_class", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: []\n", "function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\nmay_return_missing: false\ncomparison_decimals: 4\ntests: []\n"),
        ("codelist_source_class", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\n", "codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}]}]\n"),
    ] {
        let result = schema.prepare_named_class(source("independent.yaml", text), class).unwrap();
        assert!(Arc::ptr_eq(result.schema(), &schema));
        assert_eq!(result.class_name(), class);
        assert_eq!(result.source().bytes, text.as_bytes());
        assert_eq!(result.schema().structure().root_class().name, "environment_class");
        let actual = &result.normalized().document;
        let expected = decode_yaml(expected.as_bytes(), DecodeLimits::default()).unwrap().document;
        assert_eq!(logical_tree(actual, actual.root()), logical_tree(&expected, expected.root()));
        assert!(actual.field(actual.root(), "schema_version").is_none());
    }
}

#[test]
fn named_class_selection_is_bounded_closed_and_cannot_bypass_version_admission() {
    let schema = environment_schema(Limits {
        identity_bytes: 64,
        ..Limits::default()
    });
    let malformed = || source("independent.yaml", "[unclosed");
    assert!(matches!(
        schema.prepare_named_class(malformed(), &"x".repeat(65)),
        Err(Error::Limit("class_name"))
    ));
    for class in ["missing_class", "str", ""] {
        assert!(
            matches!(schema.prepare_named_class(malformed(), class), Err(Error::UnknownClass(name)) if name == class)
        );
    }
    assert!(matches!(
        schema.prepare_named_class(malformed(), "environment_class"),
        Err(Error::VersionedRootRequired)
    ));
    assert!(matches!(
        schema.prepare_named_class(malformed(), "function_definition_class"),
        Err(Error::Decode { .. })
    ));
}

#[test]
fn class_admission_cannot_bypass_versioned_environment_or_open_function_fields() {
    let environment = environment_schema(Limits::default());
    assert!(matches!(
        environment.prepare_class(source("environment.yaml", "language: python\n")),
        Err(Error::VersionedRootRequired)
    ));
    let function =
        CapturedSchema::admit_root(modules(), 0, "function_definition_class", Limits::default())
            .unwrap();
    let text="schema_version: '1.0'\nfunction: program.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: []\ncontract_version: '1'\nbinding: {call: program.constant}\n";
    let Err(Error::Findings(result)) = function.prepare_class(source("function.yaml", text)) else {
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
        temporal::{Date, DatePrecision},
        value::{ColumnType, Value, ValueType},
    };
    let schema =
        CapturedSchema::admit_root(modules(), 0, "function_definition_class", Limits::default())
            .unwrap();
    let text="function: project.identity\ndescription: Return the date.\nparams: [{name: x, type: date}]\nreturns: date\ntests:\n  - {id: leap-day, covers: [normal, boundary], args: {x: {date: '2024-02-29'}}, result: {date: '2024-02-29'}}\n  - {id: missing-x, covers: [short-circuit-missing:x], args: {x: null}, result: null}\n";
    let captured = schema
        .prepare_class(source("functions/identity.yaml", text))
        .unwrap();
    let document = &captured.normalized().document;
    let decoded = project_function_document::decode(document, document.root(), "identity").unwrap();
    let date = Value::Date(Date::new(2024, 2, 29, DatePrecision::Day).unwrap());
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
        temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
        value::Value,
    };
    let schema =
        CapturedSchema::admit_root(modules(), 0, "function_definition_class", Limits::default())
            .unwrap();
    let text="function: project.identity\ndescription: Exact defaults.\nparams:\n  - {name: integer, type: int, required: false, default: -9223372036854775808}\n  - {name: float, type: float, required: false, default: -0.0}\n  - {name: boolean, type: bool, required: false, default: false}\n  - {name: text, type: str, required: false, default: \"a\\0b\\u96ea\"}\n  - {name: date, type: date, required: false, default: {date: '2024-02-29'}}\n  - {name: datetime, type: datetime, required: false, default: {datetime: '2024-02-29T08:30'}}\n  - {name: missing, type: str, required: false, default: null, accepts_missing: true}\nreturns: str\ntests: []\n";
    let captured = schema.prepare_class(source("function.yaml", text)).unwrap();
    let doc = &captured.normalized().document;
    let decoded = project_function_document::decode(doc, doc.root(), "defaults").unwrap();
    let date = Date::new(2024, 2, 29, DatePrecision::Day).unwrap();
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
                DateTime::new(date, 8, 30, 0, DateTimePrecision::Second).unwrap()
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
        CapturedSchema::admit_root(modules(), 0, "function_definition_class", Limits::default())
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
            Catalogue, Codelist, DataType, External, Item, Source as TerminologySource, Standard,
        },
        project_terminology_document,
        value::Value,
    };
    let schema =
        CapturedSchema::admit_root(modules(), 0, "codelist_source_class", Limits::default())
            .unwrap();
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
        CapturedSchema::admit_root(modules(), 0, "codelist_source_class", Limits::default())
            .unwrap();
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
fn environment_decoder_keeps_inline_path_order_and_unread_submission_metadata() {
    use yamaa_core::{
        project_environment::Submission,
        project_environment_document::{self, Declaration},
        project_function::Language,
    };
    let schema = environment_schema(Limits::default());
    let text="schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions:\n  first: functions/first.yaml\n  second: {function: project.constant, description: Constant, params: [], returns: int, tests: []}\ncodelists:\n  - ../terminology.yaml\n  - standard: {name: CDISC/NCI, publishing_set: SDTM, version: '2024-03-29'}\n    codelists: [{id: EXTERNAL, name: External, external: {dictionary: Dict, version: '1', href: '../not-read.xml'}}]\nstudy: {id: Study01, name: Study, description: Study, protocol_name: Protocol}\nadam: {standard: {name: ADaMIG, version: '1.3', status: Final}, specs: [adam/adsl.yaml], define: ../data/adam/define.xml, documents: [{id: adrg, kind: supplemental, link: 'docs/adrg.pdf', title: Guide}]}\n";
    let captured = schema
        .prepare_structural(source("environment.yaml", text))
        .unwrap();
    let document = &captured.normalized().document;
    let result = project_environment_document::decode(document, document.root()).unwrap();
    assert_eq!(result.language, Some(Language::Python));
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
    assert_eq!(result.study, document.field(document.root(), "study"));
    assert_eq!(
        result.submissions,
        vec![(
            Submission::Adam,
            document.field(document.root(), "adam").unwrap()
        )]
    );
    assert_eq!(captured.source().bytes, text.as_bytes());
}

#[test]
fn environment_decoder_preserves_absent_versus_present_empty_functions() {
    use yamaa_core::project_environment_document;
    let schema = environment_schema(Limits::default());
    for (text, present) in [
        ("schema_version: '1.0'\n", false),
        ("schema_version: '1.0'\nfunctions: {}\n", true),
    ] {
        let captured = schema
            .prepare_structural(source("environment.yaml", text))
            .unwrap();
        let document = &captured.normalized().document;
        let result = project_environment_document::decode(document, document.root()).unwrap();
        assert_eq!(result.functions.is_some(), present);
        assert!(result.functions.into_iter().flatten().next().is_none());
        assert!(result.language.is_none() && result.lock.is_none() && result.study.is_none());
        assert!(result.codelists.is_empty() && result.submissions.is_empty());
    }
}

#[test]
fn environment_decoder_collects_every_independent_inline_scalar_failure() {
    use yamaa_core::{project_environment_document, project_function_document::Kind};
    let text="schema_version: '1.0'\nfunctions:\n  first: {function: project.a, description: A, params: [], returns: int, tests: [{id: wide, covers: [normal], args: {}, result: 9223372036854775808}]}\n  second: {function: project.b, description: B, params: [], returns: date, tests: [{id: bad-date, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\ncodelists: [{codelists: [{id: WIDE, name: Wide, data_type: integer, items: [{value: 9223372036854775808, rank: 9223372036854775808}]}]}]\n";
    let captured = environment_schema(Limits::default())
        .prepare_structural(source("environment.yaml", text))
        .unwrap();
    let document = &captured.normalized().document;
    let findings = project_environment_document::decode(document, document.root()).unwrap_err();
    assert_eq!(
        findings
            .iter()
            .map(|f| (f.path.as_str(), f.kind))
            .collect::<Vec<_>>(),
        [
            ("functions.first.tests[0].result", Kind::IntegerRange),
            ("functions.second.tests[0].result", Kind::InvalidDate),
            (
                "codelists[0].codelists[0].items[0].value",
                Kind::IntegerRange
            ),
            (
                "codelists[0].codelists[0].items[0].rank",
                Kind::IntegerRange
            ),
        ]
    );
    assert!(findings.iter().all(|f| f.node < document.nodes().len()));
}
