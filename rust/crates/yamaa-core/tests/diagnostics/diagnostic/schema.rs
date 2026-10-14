use super::project_support::Tree as T;
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::ConditionCode,
    schema::{
        BundleLimits, Document, DocumentLimits, DocumentNode as N, NormalizationBudget,
        NormalizationError, NormalizationLimits, SchemaContext as C, SchemaDiagnostic,
        SchemaModule, SchemaStructure, SpecificationDocument, ValidationBudget, ValidationLimits,
    },
};

fn document(mut tree: T) -> Document {
    if let T::Map(items) = &mut tree {
        if !items
            .iter()
            .any(|(k, _)| k == "schema_version" || k == "version")
        {
            items.insert(0, ("schema_version".into(), T::text("1.0")));
        }
    }
    raw_document(tree)
}
fn raw_document(tree: T) -> Document {
    fn push(tree: T, nodes: &mut Vec<N>) -> usize {
        let node = match tree {
            T::Scalar(node) => node,
            T::List(items) => N::Sequence(items.into_iter().map(|t| push(t, nodes)).collect()),
            T::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (push(T::text(&k), nodes), push(v, nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    let mut nodes = vec![];
    let root = push(tree, &mut nodes);
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
}
fn integer(value: &str) -> T {
    T::Scalar(N::Integer(value.into()))
}
fn descriptor(kind: &str) -> T {
    T::map(vec![("type", T::text(kind))])
}
fn field(name: &str, kind: &str, required: bool) -> T {
    T::map(vec![(
        name,
        T::map(vec![
            ("type", T::text(kind)),
            ("required", T::Scalar(N::Boolean(required))),
        ]),
    )])
}
fn schema(fields: Vec<T>, declarations: Vec<(&str, T)>) -> SchemaStructure {
    let mut fields_with_version = vec![field("schema_version", "str", false)];
    fields_with_version.extend(fields);
    let mut entries = vec![
        ("version", T::text("1.0")),
        ("root_class", T::List(fields_with_version)),
    ];
    entries.extend(declarations);
    SchemaStructure::admit(
        vec![SchemaModule {
            name: "schema.yaml".into(),
            document: document(T::map(entries)),
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
fn normal_budget() -> NormalizationBudget {
    NormalizationBudget::new(NormalizationLimits::default())
}
fn check(
    reached: &mut BTreeSet<ConditionCode>,
    findings: &[SchemaDiagnostic],
    code: ConditionCode,
    path: &str,
    condition: &str,
    requirement: &str,
) {
    let found = findings
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("missing {code:?}: {findings:?}"));
    let def = found.definition();
    assert_eq!(
        (def.phase, def.condition, def.requirement),
        ("validation", condition, Some(requirement))
    );
    assert_eq!(found.path, path);
    assert!(!found.context.is_empty());
    reached.insert(code);
}

/// Real schema/model detectors independently pin the cause, path, condition and owning rule.
pub fn reached() -> BTreeSet<ConditionCode> {
    use ConditionCode::*;
    let mut reached = BTreeSet::new();
    let basic = schema(vec![field("needed", "int", true)], vec![]);
    let input = document(T::map(vec![
        ("needed", T::Scalar(N::Boolean(true))),
        ("extra", T::text("x")),
    ]));
    let findings = basic.validate_document(&input, &mut budget()).unwrap();
    check(
        &mut reached,
        &findings,
        SchemaInvalidFieldType,
        "needed",
        "invalid_field_type",
        "REQ-0287",
    );
    check(
        &mut reached,
        &findings,
        SchemaUnknownField,
        "extra",
        "unknown_field",
        "REQ-0285",
    );
    let findings = basic
        .validate_document(&document(T::map(vec![])), &mut budget())
        .unwrap();
    check(
        &mut reached,
        &findings,
        SchemaMissingRequiredField,
        "needed",
        "missing_required_field",
        "REQ-0285",
    );
    let findings = basic
        .validate_document(
            &document(T::map(vec![("schema_version", T::text("old"))])),
            &mut budget(),
        )
        .unwrap();
    check(
        &mut reached,
        &findings,
        SchemaVersionMismatch,
        "schema_version",
        "schema_version_mismatch",
        "REQ-0281",
    );
    assert_eq!(findings.len(), 1); // Version rejection precedes missing-field detection.
    let resolved = schema(vec![field("output", "str", true)], vec![]);
    let findings = resolved
        .validate_resolved_document(&document(T::map(vec![])), &mut budget())
        .unwrap();
    check(
        &mut reached,
        &findings,
        SchemaResolvedOutputRequired,
        "output",
        "missing_required_field",
        "REQ-0657",
    );
    let ordinary = resolved
        .validate_document(&document(T::map(vec![])), &mut budget())
        .unwrap();
    assert_eq!(ordinary[0].code, SchemaMissingRequiredField);
    assert_eq!(ordinary[0].definition().requirement, Some("REQ-0285"));

    let operations = schema(
        vec![field("derive", "derivation", false)],
        vec![
            (
                "derivation",
                T::map(vec![(
                    "type",
                    T::List(vec![T::text("str"), T::text("expression")]),
                )]),
            ),
            (
                "expression",
                T::map(vec![("registry", T::text("operations"))]),
            ),
            ("operations", T::map(vec![("literal", descriptor("int"))])),
        ],
    );
    for (value, code, path, condition, req) in [
        (
            T::map(vec![]),
            SchemaInvalidOperationCount,
            "derive",
            "invalid_operation_count",
            "REQ-0286",
        ),
        (
            T::map(vec![("absent", integer("1"))]),
            SchemaUnknownOperation,
            "derive.absent",
            "unknown_operation",
            "REQ-0286",
        ),
        (
            integer("2"),
            SchemaBareDerivationScalar,
            "derive",
            "bare_derivation_scalar",
            "REQ-0320",
        ),
    ] {
        let findings = operations
            .validate_document(&document(T::map(vec![("derive", value)])), &mut budget())
            .unwrap();
        check(&mut reached, &findings, code, path, condition, req);
    }
    for (alias, req, codes) in [
        (
            "custom",
            "REQ-0287",
            [
                SchemaDescriptorValueNotPermitted,
                SchemaDescriptorPatternMismatch,
                SchemaDescriptorMinimumLength,
                SchemaDescriptorInvalidSize,
            ],
        ),
        (
            "column_type",
            "REQ-0012",
            [
                SchemaColumnTypeValueNotPermitted,
                SchemaColumnTypePatternMismatch,
                SchemaColumnTypeMinimumLength,
                SchemaColumnTypeInvalidSize,
            ],
        ),
        (
            "day_rule",
            "REQ-0609",
            [
                SchemaDayRuleValueNotPermitted,
                SchemaDayRulePatternMismatch,
                SchemaDayRuleMinimumLength,
                SchemaDayRuleInvalidSize,
            ],
        ),
        (
            "time_rule",
            "REQ-1184",
            [
                SchemaTimeRuleValueNotPermitted,
                SchemaTimeRulePatternMismatch,
                SchemaTimeRuleMinimumLength,
                SchemaTimeRuleInvalidSize,
            ],
        ),
    ] {
        let constrained = schema(
            vec![field("v", alias, false)],
            vec![(
                alias,
                T::map(vec![
                    ("type", T::text("str")),
                    ("values", T::List(vec![T::text("OK"), T::text("OK")])),
                    ("pattern", T::text("[A-Z]+")),
                    ("min_length", integer("2")),
                ]),
            )],
        );
        let findings = constrained
            .validate_document(&document(T::map(vec![("v", T::text("x"))])), &mut budget())
            .unwrap();
        assert_eq!(findings.len(), 3);
        for (code, condition) in codes[..3].iter().copied().zip([
            "value_not_permitted",
            "pattern_mismatch",
            "minimum_length",
        ]) {
            check(&mut reached, &findings, code, "v", condition, req);
        }
        assert!(matches!(
            findings[0].context[0],
            ("value", C::InputValue(_))
        ));
        let sized = schema(
            vec![field("v", alias, false)],
            vec![(
                alias,
                T::map(vec![("type", T::text("list[str]")), ("size", integer("2"))]),
            )],
        );
        let findings = sized
            .validate_document(
                &document(T::map(vec![("v", T::List(vec![]))])),
                &mut budget(),
            )
            .unwrap();
        check(&mut reached, &findings, codes[3], "v", "invalid_size", req);
    }
    let layer_schema = schema(
        vec![
            field("columns", "list[column_class]", false),
            field("output", "str", true),
        ],
        vec![
            (
                "column_class",
                T::List(vec![field("name", "str", true), field("type", "str", true)]),
            ),
            ("identifier", descriptor("str")),
        ],
    );
    for (fields, code, path, condition, req) in [
        (
            vec![
                ("schema_version", T::text("1.0")),
                ("columns", T::text("x")),
            ],
            SchemaLayerInvalidFieldType,
            "columns",
            "invalid_field_type",
            "REQ-0658",
        ),
        (
            vec![("schema_version", T::text("1.0")), ("extra", T::text("x"))],
            SchemaLayerUnknownField,
            "extra",
            "unknown_field",
            "REQ-0658",
        ),
        (
            vec![
                ("schema_version", T::text("1.0")),
                ("columns", T::List(vec![T::map(vec![])])),
            ],
            SchemaLayerMissingRequiredField,
            "columns[0].name",
            "missing_required_field",
            "REQ-0658",
        ),
        (
            vec![
                ("schema_version", T::text("1.0")),
                ("output", T::Scalar(N::Null)),
            ],
            SchemaLayerInvalidClear,
            "output",
            "invalid_clear",
            "REQ-0660",
        ),
        (
            vec![
                ("schema_version", T::text("1.0")),
                (
                    "columns",
                    T::List(vec![
                        T::map(vec![("name", T::text("X"))]),
                        T::map(vec![("name", T::text("X"))]),
                    ]),
                ),
            ],
            SchemaLayerIdentifierRepeated,
            "columns.X.name",
            "duplicate_identifier",
            "REQ-0659",
        ),
        (
            vec![("extra", T::text("x"))],
            SchemaLayerVersionMismatch,
            "schema_version",
            "schema_version_mismatch",
            "REQ-0656",
        ),
    ] {
        let input = if code == SchemaLayerVersionMismatch {
            raw_document(T::map(fields))
        } else {
            document(T::map(fields))
        };
        let NormalizationError::Invalid(findings) = layer_schema
            .normalize_layer(&input, &mut normal_budget())
            .unwrap_err()
        else {
            panic!("authored layer finding")
        };
        check(&mut reached, &findings, code, path, condition, req);
    }
    let window_schema = schema(
        vec![field("w", "window_selection", false)],
        vec![
            (
                "window_selection",
                T::map(vec![(
                    "type",
                    T::List(vec![T::text("str"), T::text("window_spec")]),
                )]),
            ),
            (
                "window_spec",
                T::List(vec![field("order_by", "list[str]", false)]),
            ),
        ],
    );
    let NormalizationError::Invalid(findings) = window_schema
        .expand_named_windows(
            &document(T::map(vec![("w", T::text("ABSENT"))])),
            true,
            &mut normal_budget(),
        )
        .unwrap_err()
    else {
        panic!("window finding")
    };
    check(
        &mut reached,
        &findings,
        SchemaUnknownWindow,
        "w",
        "unknown_window",
        "REQ-1253",
    );
    for (extra, code, path, req) in [
        (
            vec![("domain", integer("7"))],
            SchemaModelInvalidValue,
            "domain",
            "REQ-0287",
        ),
        (
            vec![("output", T::map(vec![]))],
            SchemaModelMissingField,
            "output.path",
            "REQ-0285",
        ),
        (
            vec![("extra", T::text("x"))],
            SchemaModelUnknownField,
            "extra",
            "REQ-0285",
        ),
        (
            vec![("verifications", T::List(vec![T::map(vec![])]))],
            SchemaModelInvalidOperationCount,
            "verifications.0",
            "REQ-0286",
        ),
    ] {
        let mut fields = vec![
            ("schema_version", T::text("1.0")),
            ("domain", T::text("X")),
            ("input", T::map(vec![])),
            ("keys", T::List(vec![])),
            (
                "output",
                T::map(vec![
                    ("path", T::text("o.csv")),
                    ("columns", T::List(vec![])),
                ]),
            ),
            ("columns", T::List(vec![])),
        ];
        for (name, value) in extra {
            fields.retain(|(k, _)| *k != name);
            fields.push((name, value));
        }
        let findings = SpecificationDocument::admit(document(T::map(fields)), &mut budget())
            .unwrap()
            .unwrap_err();
        check(
            &mut reached,
            &findings,
            code,
            path,
            "model_contract_mismatch",
            req,
        );
    }
    reached
}
