use std::collections::BTreeSet;
use yamaa_core::json::Value as JsonValue;
use yamaa_core::{
    diagnostic::ConditionCode,
    project_environment::{self, Draft, LockKind, LockReference},
    project_environment_diagnostics,
    project_function::Language,
    project_terminology::{Codelist, DataType, External, Item, Source},
    value::Value,
};
/// A project that links a Python environment file with no functions and
/// declares two invalid codelists of its own.
pub fn draft() -> Draft {
    let items = vec![
        Item {
            value: Value::Bool(true),
            decode: Some("bad".into()),
            rank: Some(1),
            alias: None,
            extended: true,
        },
        Item {
            value: Value::Bool(true),
            decode: None,
            rank: None,
            alias: None,
            extended: false,
        },
    ];
    let list = Codelist {
        id: "SEX".into(),
        name: "Sex".into(),
        data_type: DataType::Text,
        extensible: false,
        alias: None,
        format_name: None,
        items: Some(items),
        external: Some(External {
            dictionary: "Other".into(),
            version: "1".into(),
            href: None,
        }),
    };
    Draft {
        language: Some(Language::Python),
        lock: None,
        functions: Some(vec![]),
        codelists: vec![Source {
            standard: None,
            codelists: vec![list.clone(), list],
        }],
    }
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let d = draft();
    let findings = project_environment::validate(Language::Python, &d);
    let projected = project_environment_diagnostics::diagnostics(&d, &findings).unwrap();
    let expected = [
        (
            "codelist_shape_invalid",
            "REQ-0947",
            "codelists[0].codelists[0]",
        ),
        (
            "codelist_shape_invalid",
            "REQ-0950",
            "codelists[0].codelists[0].items[0].value",
        ),
        (
            "codelist_extension_not_admitted",
            "REQ-0952",
            "codelists[0].codelists[0].items[0].extended",
        ),
        (
            "codelist_shape_invalid",
            "REQ-0950",
            "codelists[0].codelists[0].items[1].value",
        ),
        (
            "codelist_duplicate_value",
            "REQ-0949",
            "codelists[0].codelists[0].items[1].value",
        ),
        (
            "codelist_partial_item_field",
            "REQ-0951",
            "codelists[0].codelists[0]",
        ),
        (
            "codelist_partial_item_field",
            "REQ-0951",
            "codelists[0].codelists[0]",
        ),
        (
            "duplicate_define_identifier",
            "REQ-0948",
            "codelists[0].codelists[1].id",
        ),
        (
            "duplicate_define_identifier",
            "REQ-0948",
            "codelists[0].codelists[1].name",
        ),
        (
            "codelist_shape_invalid",
            "REQ-0947",
            "codelists[0].codelists[1]",
        ),
        (
            "codelist_shape_invalid",
            "REQ-0950",
            "codelists[0].codelists[1].items[0].value",
        ),
        (
            "codelist_extension_not_admitted",
            "REQ-0952",
            "codelists[0].codelists[1].items[0].extended",
        ),
        (
            "codelist_shape_invalid",
            "REQ-0950",
            "codelists[0].codelists[1].items[1].value",
        ),
        (
            "codelist_duplicate_value",
            "REQ-0949",
            "codelists[0].codelists[1].items[1].value",
        ),
        (
            "codelist_partial_item_field",
            "REQ-0951",
            "codelists[0].codelists[1]",
        ),
        (
            "codelist_partial_item_field",
            "REQ-0951",
            "codelists[0].codelists[1]",
        ),
    ];
    assert_eq!(projected.len(), expected.len());
    let mut reached = BTreeSet::new();
    for (finding, (condition, requirement, path)) in projected.iter().zip(expected) {
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(finding.code);
    }
    // The `environments` key states the language, so only the captured lock
    // syntax can disagree with it.
    let mut lock_mismatch = d.clone();
    lock_mismatch.lock = Some(LockReference {
        written: "renv.lock".into(),
        kind: LockKind::Renv,
    });
    lock_mismatch.codelists.clear();
    let findings = project_environment::validate(Language::Python, &lock_mismatch);
    let projected =
        project_environment_diagnostics::diagnostics(&lock_mismatch, &findings).unwrap();
    assert_eq!(projected.len(), 1);
    let finding = &projected[0];
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().requirement,
        ),
        (
            "validation",
            "project_environment_invalid",
            Some("REQ-0695")
        ),
    );
    assert_eq!(finding.spec_paths, ["lock"]);
    assert_eq!(finding.context.len(), 3);
    assert_eq!(finding.context["reason"], "lock_kind_mismatch");
    assert_eq!(finding.context["language"], "python");
    assert_eq!(finding.context["actual_lock_kind"], "renv");
    assert_eq!(finding.source_span, None);
    assert_eq!(finding.operand_route, None);
    reached.insert(finding.code);
    for (failure, cause) in [
        (
            project_environment_diagnostics::ReadFailure::Missing,
            "missing",
        ),
        (
            project_environment_diagnostics::ReadFailure::Unreadable,
            "unreadable",
        ),
    ] {
        let finding = failure.diagnostic("/study/project.yaml");
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().requirement,
            ),
            (
                "validation",
                "project_environment_missing",
                Some("REQ-0694"),
            ),
        );
        assert_eq!(finding.spec_paths, ["environment"]);
        assert_eq!(finding.context.len(), 2);
        assert_eq!(finding.context["cause"], JsonValue::String(cause.into()),);
        assert_eq!(
            finding.context["path"],
            JsonValue::String("/study/project.yaml".into()),
        );
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(finding.code);
    }
    reached
}
