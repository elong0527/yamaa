use super::rendered;
use crate::support::project_call_compiler::Tree as T;
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::ConditionCode,
    schema::{
        Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
        SchemaModule, SchemaStructure, SpecificationDocument,
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
    tree.document()
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
    )
    .unwrap()
}
fn check(
    reached: &mut BTreeSet<ConditionCode>,
    findings: &[SchemaDiagnostic],
    code: ConditionCode,
    path: &str,
    condition: &str,
    rule: &str,
) {
    let found = findings
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("missing {code:?}: {findings:?}"));
    let def = found.definition();
    assert_eq!(
        (def.phase, def.condition, def.rule),
        ("validation", condition, Some(rule))
    );
    assert_eq!(found.path, path);
    assert!(!found.context.is_empty());
    reached.insert(rendered(found));
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
    let findings = basic.validate_document(&input).unwrap();
    check(
        &mut reached,
        &findings,
        SchemaInvalidFieldType,
        "needed",
        "invalid_field_type",
        "SCHEMA-12",
    );
    check(
        &mut reached,
        &findings,
        SchemaUnknownField,
        "extra",
        "unknown_field",
        "SCHEMA-13",
    );
    let findings = basic.validate_document(&document(T::map(vec![]))).unwrap();
    check(
        &mut reached,
        &findings,
        SchemaMissingRequiredField,
        "needed",
        "missing_required_field",
        "SCHEMA-13",
    );
    let findings = basic
        .validate_document(&document(T::map(vec![("schema_version", T::text("old"))])))
        .unwrap();
    check(
        &mut reached,
        &findings,
        SchemaVersionMismatch,
        "schema_version",
        "schema_version_mismatch",
        "SCHEMA-2",
    );
    assert_eq!(findings.len(), 1); // Version rejection precedes missing-field detection.
    let resolved = schema(vec![field("output", "str", true)], vec![]);
    let findings = resolved
        .validate_resolved_document(&document(T::map(vec![])))
        .unwrap();
    check(
        &mut reached,
        &findings,
        SchemaResolvedOutputRequired,
        "output",
        "missing_required_field",
        "LAYER-12",
    );
    let ordinary = resolved
        .validate_document(&document(T::map(vec![])))
        .unwrap();
    assert_eq!(ordinary[0].code, SchemaMissingRequiredField);
    assert_eq!(ordinary[0].definition().rule, Some("SCHEMA-13"));

    // A written long form holding only its shorthand's field restates it; the
    // resolved, canonical document holds that long form and is not refused.
    let shorthand = schema(
        vec![field("item", "item_value", false)],
        vec![
            (
                "item_value",
                T::map(vec![(
                    "type",
                    T::List(vec![T::text("str"), T::text("item_class")]),
                )]),
            ),
            ("item_class", T::List(vec![field("name", "str", true)])),
        ],
    );
    let long_form = document(T::map(vec![(
        "item",
        T::map(vec![("name", T::text("DM"))]),
    )]));
    let findings = shorthand.validate_document(&long_form).unwrap();
    check(
        &mut reached,
        &findings,
        SchemaRedundantLongForm,
        "item",
        "redundant_long_form",
        "SCHEMA-15",
    );
    assert_eq!(
        findings[0].context,
        [
            ("field", C::Text("name".into())),
            ("shorthand", C::InputValue(4)),
        ]
    );
    assert!(shorthand
        .validate_resolved_document(&long_form)
        .unwrap()
        .is_empty());

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
            "SCHEMA-7",
        ),
        (
            T::map(vec![("absent", integer("1"))]),
            SchemaUnknownOperation,
            "derive.absent",
            "unknown_operation",
            "SCHEMA-7",
        ),
        (
            integer("2"),
            SchemaBareDerivationScalar,
            "derive",
            "bare_derivation_scalar",
            "EXPR-5",
        ),
    ] {
        let findings = operations
            .validate_document(&document(T::map(vec![("derive", value)])))
            .unwrap();
        check(&mut reached, &findings, code, path, condition, req);
    }
    let cases = schema(
        vec![field("derive", "expression", false)],
        vec![
            (
                "expression",
                T::map(vec![("registry", T::text("expressions"))]),
            ),
            (
                "expressions",
                T::map(vec![("case", descriptor("list[case_item_class]"))]),
            ),
            (
                "case_item_class",
                T::map(vec![(
                    "type",
                    T::List(vec![
                        T::text("case_branch_class"),
                        T::text("case_otherwise_class"),
                    ]),
                )]),
            ),
            (
                "case_branch_class",
                T::List(vec![field("when", "str", true), field("then", "int", true)]),
            ),
            (
                "case_otherwise_class",
                T::List(vec![field("otherwise", "int", true)]),
            ),
        ],
    );
    let otherwise_only = T::map(vec![(
        "case",
        T::List(vec![T::map(vec![("otherwise", integer("0"))])]),
    )]);
    let findings = cases
        .validate_document(&document(T::map(vec![("derive", otherwise_only)])))
        .unwrap();
    check(
        &mut reached,
        &findings,
        SchemaCaseItems,
        "derive.case",
        "invalid_field_type",
        "EXPR-7",
    );
    // A column type or day rule outside its closed values keeps the rule that
    // closes them (VALUE-1, TIME-10); every other constraint is SCHEMA-12's.
    for (alias, codes) in [
        (
            "custom",
            [
                (SchemaDescriptorValueNotPermitted, "SCHEMA-12"),
                (SchemaDescriptorPatternMismatch, "SCHEMA-12"),
                (SchemaDescriptorMinimumLength, "SCHEMA-12"),
                (SchemaDescriptorInvalidSize, "SCHEMA-12"),
            ],
        ),
        (
            "column_type",
            [
                (SchemaColumnTypeValueNotPermitted, "VALUE-1"),
                (SchemaColumnTypePatternMismatch, "SCHEMA-12"),
                (SchemaColumnTypeMinimumLength, "SCHEMA-12"),
                (SchemaColumnTypeInvalidSize, "SCHEMA-12"),
            ],
        ),
        (
            "day_rule",
            [
                (SchemaDayRuleValueNotPermitted, "TIME-10"),
                (SchemaDayRulePatternMismatch, "SCHEMA-12"),
                (SchemaDayRuleMinimumLength, "SCHEMA-12"),
                (SchemaDayRuleInvalidSize, "SCHEMA-12"),
            ],
        ),
        (
            "time_rule",
            [
                (SchemaTimeRuleValueNotPermitted, "SCHEMA-12"),
                (SchemaTimeRulePatternMismatch, "SCHEMA-12"),
                (SchemaTimeRuleMinimumLength, "SCHEMA-12"),
                (SchemaTimeRuleInvalidSize, "SCHEMA-12"),
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
            .validate_document(&document(T::map(vec![("v", T::text("x"))])))
            .unwrap();
        assert_eq!(findings.len(), 3);
        for ((code, req), condition) in codes[..3].iter().copied().zip([
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
            .validate_document(&document(T::map(vec![("v", T::List(vec![]))])))
            .unwrap();
        let (code, req) = codes[3];
        check(&mut reached, &findings, code, "v", "invalid_size", req);
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
            "SCHEMA-12",
        ),
        (
            vec![("schema_version", T::text("1.0")), ("extra", T::text("x"))],
            SchemaLayerUnknownField,
            "extra",
            "unknown_field",
            "SCHEMA-13",
        ),
        (
            vec![
                ("schema_version", T::text("1.0")),
                ("columns", T::List(vec![T::map(vec![])])),
            ],
            SchemaLayerMissingRequiredField,
            "columns[0].name",
            "missing_required_field",
            "SCHEMA-13",
        ),
        (
            vec![
                ("schema_version", T::text("1.0")),
                ("output", T::Scalar(N::Null)),
            ],
            SchemaLayerInvalidClear,
            "output",
            "invalid_clear",
            "LAYER-9",
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
            "LAYER-4",
        ),
        (
            vec![("extra", T::text("x"))],
            SchemaLayerVersionMismatch,
            "schema_version",
            "schema_version_mismatch",
            "LAYER-4",
        ),
    ] {
        let input = if code == SchemaLayerVersionMismatch {
            raw_document(T::map(fields))
        } else {
            document(T::map(fields))
        };
        let NormalizationError::Invalid(findings) =
            layer_schema.normalize_layer(&input).unwrap_err()
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
        .expand_named_windows(&document(T::map(vec![("w", T::text("ABSENT"))])), true)
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
        "WINDOW-1",
    );
    for (extra, code, path, req) in [
        (
            vec![("domain", integer("7"))],
            SchemaModelInvalidValue,
            "domain",
            "SCHEMA-12",
        ),
        (
            vec![("output", T::map(vec![]))],
            SchemaModelMissingField,
            "output.path",
            "SCHEMA-13",
        ),
        (
            vec![("extra", T::text("x"))],
            SchemaModelUnknownField,
            "extra",
            "SCHEMA-13",
        ),
        (
            vec![("verifications", T::List(vec![T::map(vec![])]))],
            SchemaModelInvalidOperationCount,
            "verifications.0",
            "SCHEMA-7",
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
        let findings = SpecificationDocument::admit(document(T::map(fields)))
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
