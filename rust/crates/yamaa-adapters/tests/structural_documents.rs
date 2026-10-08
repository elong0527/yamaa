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
