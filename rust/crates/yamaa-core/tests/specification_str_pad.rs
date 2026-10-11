//! Independent captured reference ordering for positive-width admission failures.
#[allow(dead_code)]
#[path = "support/project_call_compiler.rs"]
mod support;
use support::{specification, specification_with_rows, Tree};
use yamaa_core::{
    diagnostic::ConditionCode,
    schema::DocumentNode as N,
    specification::{BindError, PreparedSpecification},
    table::{Column, TableSchema},
    value::ColumnType,
};

fn pad(source: &str, width: &str) -> Tree {
    Tree::map(vec![(
        "str_pad",
        Tree::map(vec![
            ("source", Tree::text(source)),
            ("width", Tree::Scalar(N::Integer(width.into()))),
        ]),
    )])
}
fn cases(when: &str, first: Tree, otherwise: Tree) -> Tree {
    Tree::map(vec![(
        "case",
        Tree::List(vec![
            Tree::map(vec![("when", Tree::text(when)), ("then", first)]),
            Tree::map(vec![("otherwise", otherwise)]),
        ]),
    )])
}
fn findings(spec: &yamaa_core::schema::SpecificationDocument) -> Vec<(ConditionCode, String)> {
    let prepared = PreparedSpecification::prepare(spec).unwrap();
    let source = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
        panic!("retained width and reference findings")
    };
    findings
        .iter()
        .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
        .map(|diagnostic| (diagnostic.code, diagnostic.spec_paths[0].clone()))
        .collect()
}
fn widths_and_sources(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).into()).collect()
}

#[test]
fn all_nested_case_widths_precede_that_derivations_reference_findings() {
    let spec = specification(vec![(
        "A",
        "str",
        cases(
            "ID = 1",
            cases("ID = 1", pad("SRC.ABSENT1", "0"), pad("SRC.ABSENT2", "-1")),
            pad("SRC.ABSENT3", "0"),
        ),
    )]);
    let actual = findings(&spec);
    assert_eq!(
        actual
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>(),
        widths_and_sources(&[
            "columns.A.derivation.case[0].then.case[0].then.str_pad.width",
            "columns.A.derivation.case[0].then.case[1].otherwise.str_pad.width",
            "columns.A.derivation.case[1].otherwise.str_pad.width",
            "columns.A.derivation.case[0].then.case[0].then.str_pad.source",
            "columns.A.derivation.case[0].then.case[1].otherwise.str_pad.source",
            "columns.A.derivation.case[1].otherwise.str_pad.source",
        ])
    );
    assert!(actual[..3]
        .iter()
        .all(|(code, _)| *code == ConditionCode::StrPadWidth));
}

#[test]
fn width_checkpoint_precedes_case_predicate_reference_binding_without_duplication() {
    let spec = specification(vec![(
        "A",
        "str",
        cases("ABSENT = 1", pad("SRC.ABSENT1", "0"), pad("SRC.ID", "2")),
    )]);
    let actual = findings(&spec);
    assert_eq!(actual[0].0, ConditionCode::StrPadWidth);
    assert_eq!(
        actual.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
        widths_and_sources(&[
            "columns.A.derivation.case[0].then.str_pad.width",
            "columns.A.derivation.case[0].when",
            "columns.A.derivation.case[0].then.str_pad.source",
        ])
    );
    // The existing case predicate reference projection still uses REQ-0103;
    // reference Python uses REQ-0189. This test qualifies order, not that debt.
}

#[test]
fn separate_column_derivations_keep_width_then_reference_interleaving() {
    let spec = specification(vec![
        ("A", "str", pad("SRC.ABSENT1", "0")),
        ("B", "str", pad("SRC.ABSENT2", "-1")),
    ]);
    assert_eq!(
        findings(&spec)
            .into_iter()
            .map(|(_, path)| path)
            .collect::<Vec<_>>(),
        widths_and_sources(&[
            "columns.A.derivation.str_pad.width",
            "columns.A.derivation.str_pad.source",
            "columns.B.derivation.str_pad.width",
            "columns.B.derivation.str_pad.source",
        ])
    );
}

#[test]
fn row_substitutions_report_all_widths_before_reference_binding() {
    let spec = specification_with_rows(
        vec![
            (
                "A",
                "str",
                Tree::map(vec![("literal", Tree::text("unused"))]),
            ),
            (
                "B",
                "str",
                Tree::map(vec![("literal", Tree::text("unused"))]),
            ),
        ],
        vec![Tree::map(vec![
            ("id", Tree::text("records")),
            (
                "derivations",
                Tree::map(vec![
                    ("A", Tree::map(vec![("value", pad("ABSENT1", "0"))])),
                    ("B", Tree::map(vec![("value", pad("ABSENT2", "-1"))])),
                ]),
            ),
        ])],
    );
    assert_eq!(
        findings(&spec)
            .into_iter()
            .map(|(_, path)| path)
            .collect::<Vec<_>>(),
        widths_and_sources(&[
            "rows[0].derivations.A.str_pad.width",
            "rows[0].derivations.B.str_pad.width",
            "rows[0].derivations.A.str_pad.source",
            "rows[0].derivations.B.str_pad.source",
        ])
    );
}

#[test]
fn predicate_grammar_stays_before_its_branch_width_validation() {
    let spec = specification(vec![(
        "A",
        "str",
        cases("ID >", pad("SRC.ABSENT", "0"), pad("SRC.ID", "2")),
    )]);
    let actual = findings(&spec);
    assert_eq!(actual[0].0.definition().condition, "invalid_predicate");
    assert_eq!(actual[1].0, ConditionCode::StrPadWidth);
    assert_eq!(
        actual.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
        widths_and_sources(&[
            "columns.A.derivation.case[0].when",
            "columns.A.derivation.case[0].then.str_pad.width",
            "columns.A.derivation.case[0].then.str_pad.source",
        ])
    );
}

#[test]
fn numeric_grammar_and_widths_keep_authored_order_before_reference_binding() {
    let invalid_numeric = || {
        Tree::map(vec![(
            "compute",
            Tree::map(vec![("expr", Tree::text("ID +"))]),
        )])
    };
    for (first, otherwise, expected) in [
        (
            invalid_numeric(),
            pad("SRC.ABSENT", "0"),
            widths_and_sources(&[
                "columns.A.derivation.case[0].then.compute.expr",
                "columns.A.derivation.case[1].otherwise.str_pad.width",
                "columns.A.derivation.case[0].when",
                "columns.A.derivation.case[1].otherwise.str_pad.source",
            ]),
        ),
        (
            pad("SRC.ABSENT", "0"),
            invalid_numeric(),
            widths_and_sources(&[
                "columns.A.derivation.case[0].then.str_pad.width",
                "columns.A.derivation.case[1].otherwise.compute.expr",
                "columns.A.derivation.case[0].when",
                "columns.A.derivation.case[0].then.str_pad.source",
            ]),
        ),
    ] {
        let spec = specification(vec![("A", "str", cases("ABSENT = 1", first, otherwise))]);
        let actual = findings(&spec);
        assert!(actual[..2]
            .iter()
            .any(|(code, _)| *code == ConditionCode::StrPadWidth));
        assert!(actual[..2]
            .iter()
            .any(|(code, _)| code.definition().condition == "invalid_numeric_expression"));
        assert_eq!(
            actual.into_iter().map(|(_, path)| path).collect::<Vec<_>>(),
            expected
        );
    }
}
