//! EXPR-14 result types read by column conversion: VALUE-24 refuses, before
//! any record is read, a declared type no result of the expression can take.
use super::rendered;
use crate::support::document::Tree;
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode as C, Diagnostic},
    json::Value as V,
    schema::{Document, SpecificationDocument},
    specification::{BindError, PreparedSpecification},
    table::{Column, TableSchema},
    value::ColumnType,
};

/// One output column besides the key, derived by `derivation` and optionally
/// answered by an `unconvertible` literal.
fn document(kind: &str, value: Tree, unconvertible: Option<Tree>) -> SpecificationDocument {
    let mut derivation = vec![("value", value)];
    if let Some(handler) = unconvertible {
        derivation.push(("unconvertible", handler));
    }
    let column = |name: &str, kind: &str, derivation: Tree| {
        Tree::map(vec![
            ("name", Tree::text(name)),
            ("type", Tree::text(kind)),
            ("derivation", derivation),
        ])
    };
    let tree = Tree::map(vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![
                    ("path", Tree::text("input.csv")),
                    ("types", Tree::map(vec![("ID", Tree::text("int"))])),
                ]),
            )]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        (
            "columns",
            Tree::List(vec![
                column(
                    "ID",
                    "int",
                    Tree::map(vec![(
                        "value",
                        Tree::map(vec![(
                            "source",
                            Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                        )]),
                    )]),
                ),
                column("VALUE", kind, Tree::map(derivation)),
            ]),
        ),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("output.csv")),
                (
                    "columns",
                    Tree::List(vec![Tree::text("ID"), Tree::text("VALUE")]),
                ),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}

fn source() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}

/// `check` and binding report one finding, the same before and after capture.
fn refused(kind: &str, value: Tree) -> Diagnostic {
    let prepared = PreparedSpecification::prepare(&document(kind, value, None)).unwrap();
    let checked = prepared.check_declaration_diagnostics();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source()) else {
        panic!("a result type its column cannot take is refused at binding")
    };
    let bound = findings
        .iter()
        .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(checked, bound, "check reports what binding reports");
    let [diagnostic] = bound.as_slice() else {
        panic!("one finding: {bound:?}")
    };
    diagnostic.clone()
}

pub fn reached() -> BTreeSet<C> {
    let window = Tree::map(vec![(
        "row_number",
        Tree::map(vec![(
            "window",
            Tree::map(vec![("order_by", Tree::List(vec![Tree::text("ID")]))]),
        )]),
    )]);
    let mut reached = BTreeSet::new();
    for (kind, value, from) in [
        // A fixed result type: a row number is a count.
        ("datetime", window.clone(), "int"),
        // A result type from what is read: an integer literal.
        (
            "date",
            Tree::map(vec![("literal", Tree::integer(5))]),
            "int",
        ),
        // Boolean values convert to no column type.
        (
            "str",
            Tree::map(vec![("literal", Tree::boolean(true))]),
            "bool",
        ),
    ] {
        let diagnostic = refused(kind, value);
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().rule
            ),
            ("validation", "conversion_failed", Some("VALUE-24"))
        );
        assert_eq!(diagnostic.spec_paths, ["columns.VALUE"]);
        assert_eq!(
            diagnostic.context,
            [
                ("from".into(), V::String(from.into())),
                ("to".into(), V::String(kind.into())),
            ]
            .into()
        );
        reached.insert(rendered(&diagnostic));
    }
    // A declared `unconvertible` answers each value, so the derivation stands;
    // a text result may parse; and an always-missing result converts.
    for (kind, value, handler) in [
        (
            "date",
            Tree::map(vec![("literal", Tree::integer(5))]),
            Some(Tree::map(vec![(
                "literal",
                Tree::Scalar(yamaa_core::schema::DocumentNode::Null),
            )])),
        ),
        (
            "date",
            Tree::map(vec![("literal", Tree::text("2025-01-02"))]),
            None,
        ),
        ("int", window, None),
    ] {
        let prepared = PreparedSpecification::prepare(&document(kind, value, handler)).unwrap();
        assert!(prepared.check_declaration_diagnostics().is_empty());
        prepared.bind(&source()).unwrap();
    }
    reached
}
