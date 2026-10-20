use crate::support::diagnostic::rendered;
use crate::support::roots::{Codelist, External, Item, Source, Standard};
use std::collections::BTreeSet;
use yamaa_core::json::Value as JsonValue;
use yamaa_core::{
    ast::CodelistType,
    diagnostic::ConditionCode,
    project_environment::{self, Draft, LockKind, LockReference},
    project_environment_diagnostics,
    project_function::Language,
    value::Value,
};
/// CT-6: a sponsor-extended item of a published extensible list has no
/// published identifier, so an `alias` on it is refused; the published item
/// beside it keeps its own.
pub fn extended_alias_reached() -> BTreeSet<ConditionCode> {
    let item = |value: &str, alias: Option<&str>, extended| Item {
        value: Value::Str(value.into()),
        decode: None,
        rank: None,
        alias: alias.map(String::from),
        extended,
    };
    let mut draft = draft();
    draft.codelists = vec![Source {
        standard: Some(Standard {
            name: "CDISC/NCI".into(),
            publishing_set: "SDTM".into(),
            version: "2025-03-28".into(),
        }),
        codelists: vec![Codelist {
            id: "EPOCH".into(),
            name: "Epoch".into(),
            data_type: CodelistType::Text,
            extensible: true,
            alias: Some("C99079".into()),
            format_name: None,
            items: Some(vec![
                item("SCREENING", Some("C48262"), false),
                item("EXTENSION", Some("C48262"), true),
                item("TITRATION", None, true),
            ]),
            external: None,
        }],
    }
    .root()];
    draft.language = Some(Language::Python);
    let findings = project_environment::validate(Language::Python, &draft);
    let projected = project_environment_diagnostics::diagnostics(&draft, &findings).unwrap();
    assert_eq!(projected.len(), 1, "{projected:?}");
    let finding = &projected[0];
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().rule
        ),
        ("validation", "codelist_extended_alias", Some("CT-6"))
    );
    assert_eq!(
        finding.spec_paths,
        ["codelists[0].codelists[0].items[1].alias"]
    );
    for (key, value) in [
        ("codelist", "EPOCH"),
        ("value", "EXTENSION"),
        ("alias", "C48262"),
    ] {
        assert_eq!(finding.context[key], JsonValue::String(value.into()));
    }
    assert_eq!(finding.context.len(), 3);
    BTreeSet::from([rendered(&finding)])
}
/// CT-12: a text codelist names a character format, so its `format_name`
/// begins with `$`; the compliant list beside it passes.
pub fn format_name_reached() -> BTreeSet<ConditionCode> {
    let list = |id: &str, format: &str| Codelist {
        id: id.into(),
        name: id.into(),
        data_type: CodelistType::Text,
        extensible: false,
        alias: None,
        format_name: Some(format.into()),
        items: Some(vec![Item {
            value: Value::Str("Y".into()),
            decode: None,
            rank: None,
            alias: None,
            extended: false,
        }]),
        external: None,
    };
    let mut draft = draft();
    draft.codelists = vec![Source {
        standard: None,
        codelists: vec![list("NY", "$NY"), list("SEX", "SEX")],
    }
    .root()];
    let findings = project_environment::validate(Language::Python, &draft);
    let projected = project_environment_diagnostics::diagnostics(&draft, &findings).unwrap();
    assert_eq!(projected.len(), 1, "{projected:?}");
    let finding = &projected[0];
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().rule
        ),
        ("validation", "value_not_permitted", Some("CT-12"))
    );
    assert_eq!(
        finding.spec_paths,
        ["codelists[0].codelists[1].format_name"]
    );
    for (key, value) in [("codelist", "SEX"), ("value", "SEX"), ("data_type", "text")] {
        assert_eq!(finding.context[key], JsonValue::String(value.into()));
    }
    assert_eq!(finding.context.len(), 3);
    assert_eq!(finding.source_span, None);
    assert_eq!(finding.operand_route, None);
    BTreeSet::from([rendered(&finding)])
}
/// FUNC-2, FUNC-3 (P-FN-2): a finding in a 1.0 function file fails as
/// `function_file_invalid` at that file's own path, and a lock whose kind is
/// not its host's at the project's `environments.<host>.lock`, with the
/// reasons and context a 0.5 environment file's findings take.
pub fn function_file_reached() -> BTreeSet<ConditionCode> {
    use crate::support::project_call_compiler::definition;
    // A function file that names no callable at all.
    let unimplemented = definition("constant").file(&[]);
    let draft = Draft {
        surface: yamaa_core::ast::Surface::V1 { minor: 0 },
        language: Some(Language::Python),
        lock: Some(LockReference {
            written: "renv.lock".into(),
            kind: LockKind::Renv,
        }),
        functions: Some(vec![
            // A Python callable written for R.
            definition("id").file(&[
                (Language::Python, "project.id"),
                (Language::R, "project.id"),
            ]),
            // A second file declaring the same name.
            definition("id").file(&[(Language::Python, "project.id")]),
            unimplemented,
        ]),
        codelists: vec![],
    };
    let findings = project_environment::validate(Language::Python, &draft);
    let projected = project_environment_diagnostics::diagnostics(&draft, &findings).unwrap();
    let expected = [
        (
            "environments.python.lock",
            vec![
                ("reason", "lock_kind_mismatch"),
                ("language", "python"),
                ("actual_lock_kind", "renv"),
            ],
        ),
        (
            "implementations.r",
            vec![
                ("function", "id"),
                ("language", "r"),
                ("reason", "invalid_callable"),
            ],
        ),
        (
            "name",
            vec![("function", "id"), ("reason", "duplicate_function")],
        ),
        (
            "implementations",
            vec![
                ("function", "constant"),
                ("reason", "missing_implementation"),
            ],
        ),
    ];
    assert_eq!(projected.len(), expected.len(), "{projected:?}");
    let mut reached = BTreeSet::new();
    for (finding, (path, context)) in projected.iter().zip(expected) {
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().rule
            ),
            ("validation", "function_file_invalid", Some("FUNC-2"))
        );
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(
            finding.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
                .collect()
        );
        reached.insert(rendered(finding));
    }
    // A value the declared type cannot read fails as its file does, keeping
    // the text it is written as.
    let scalar = project_environment_diagnostics::function_file_scalar_diagnostics(&[
        yamaa_core::project_function_document::Finding {
            path: "tests[0].result".into(),
            node: 0,
            kind: yamaa_core::project_function_document::Kind::InvalidDate,
            value: Some("2024-02-30".into()),
        },
    ])
    .unwrap();
    assert_eq!(scalar.len(), 1);
    assert_eq!(scalar[0].code, ConditionCode::FunctionFileInvalid);
    assert_eq!(scalar[0].spec_paths, ["tests[0].result"]);
    assert_eq!(scalar[0].context["reason"], "invalid_date");
    assert_eq!(scalar[0].context["value"], "2024-02-30");
    reached.insert(rendered(&scalar[0]));
    // A lock that is no packaging lock fails at the field that holds it,
    // naming the project file that holds that field (FUNC-3).
    let lock = project_environment_diagnostics::invalid_packaging_lock(
        Language::R,
        "study/project.yaml",
        "r/renv.lock",
    );
    assert_eq!(
        (
            lock.definition().phase,
            lock.definition().condition,
            lock.definition().rule
        ),
        ("validation", "function_file_invalid", Some("FUNC-2"))
    );
    assert_eq!(lock.spec_paths, ["environments.r.lock"]);
    assert_eq!(
        lock.context,
        [
            ("source", "study/project.yaml"),
            ("entry", "study/project.yaml"),
            ("path", "r/renv.lock"),
            ("environment_path", "environments.r.lock"),
            ("cause", "invalid_packaging_lock"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), JsonValue::String(value.into())))
        .collect()
    );
    reached.insert(rendered(&lock));
    reached
}
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
        data_type: CodelistType::Text,
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
        surface: yamaa_core::ast::Surface::V0_5,
        language: Some(Language::Python),
        lock: None,
        functions: Some(vec![]),
        codelists: vec![Source {
            standard: None,
            codelists: vec![list.clone(), list],
        }
        .root()],
    }
}
pub fn reached() -> BTreeSet<ConditionCode> {
    let d = draft();
    let findings = project_environment::validate(Language::Python, &d);
    let projected = project_environment_diagnostics::diagnostics(&d, &findings).unwrap();
    let expected = [
        (
            "codelist_shape_invalid",
            "CT-3",
            "codelists[0].codelists[0]",
        ),
        (
            "codelist_shape_invalid",
            "CT-4",
            "codelists[0].codelists[0].items[0].value",
        ),
        (
            "codelist_extension_not_admitted",
            "CT-6",
            "codelists[0].codelists[0].items[0].extended",
        ),
        (
            "codelist_shape_invalid",
            "CT-4",
            "codelists[0].codelists[0].items[1].value",
        ),
        (
            "codelist_duplicate_value",
            "CT-4",
            "codelists[0].codelists[0].items[1].value",
        ),
        (
            "codelist_partial_item_field",
            "CT-5",
            "codelists[0].codelists[0]",
        ),
        (
            "codelist_partial_item_field",
            "CT-5",
            "codelists[0].codelists[0]",
        ),
        (
            "duplicate_define_identifier",
            "CT-1",
            "codelists[0].codelists[1].id",
        ),
        (
            "duplicate_define_identifier",
            "CT-1",
            "codelists[0].codelists[1].name",
        ),
        (
            "codelist_shape_invalid",
            "CT-3",
            "codelists[0].codelists[1]",
        ),
        (
            "codelist_shape_invalid",
            "CT-4",
            "codelists[0].codelists[1].items[0].value",
        ),
        (
            "codelist_extension_not_admitted",
            "CT-6",
            "codelists[0].codelists[1].items[0].extended",
        ),
        (
            "codelist_shape_invalid",
            "CT-4",
            "codelists[0].codelists[1].items[1].value",
        ),
        (
            "codelist_duplicate_value",
            "CT-4",
            "codelists[0].codelists[1].items[1].value",
        ),
        (
            "codelist_partial_item_field",
            "CT-5",
            "codelists[0].codelists[1]",
        ),
        (
            "codelist_partial_item_field",
            "CT-5",
            "codelists[0].codelists[1]",
        ),
    ];
    assert_eq!(projected.len(), expected.len());
    let mut reached = BTreeSet::new();
    for (finding, (condition, rule, path)) in projected.iter().zip(expected) {
        assert_eq!(
            (
                finding.definition().phase,
                finding.definition().condition,
                finding.definition().rule
            ),
            ("validation", condition, Some(rule))
        );
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(finding.source_span, None);
        assert_eq!(finding.operand_route, None);
        reached.insert(rendered(&finding));
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
            finding.definition().rule,
        ),
        ("validation", "project_environment_invalid", Some("FUNC-2")),
    );
    assert_eq!(finding.spec_paths, ["lock"]);
    assert_eq!(finding.context.len(), 3);
    assert_eq!(finding.context["reason"], "lock_kind_mismatch");
    assert_eq!(finding.context["language"], "python");
    assert_eq!(finding.context["actual_lock_kind"], "renv");
    assert_eq!(finding.source_span, None);
    assert_eq!(finding.operand_route, None);
    reached.insert(rendered(&finding));
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
                finding.definition().rule,
            ),
            ("validation", "project_environment_missing", Some("FUNC-1"),),
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
        reached.insert(rendered(&finding));
    }
    reached
}
