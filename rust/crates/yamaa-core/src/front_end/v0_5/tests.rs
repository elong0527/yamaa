//! Lowering of documents the model admits without schema normalization,
//! which hand-built specs and host bindings pass. Every form, normalized
//! as a host admits it, is checked with the shipped schema in
//! `yamaa-adapters/tests/ast_lowering.rs`.
use super::*;
use crate::ast::{ExprKind, ScalarOp};
use crate::test_document::Tree;

pub(crate) fn op(name: &str, payload: Tree) -> Tree {
    Tree::map(vec![(name, payload)])
}
pub(crate) fn derivation(value: Tree) -> Tree {
    Tree::map(vec![("value", value)])
}
pub(crate) fn column(name: &str, kind: &str, derivation: Tree) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text(kind)),
        ("derivation", derivation),
    ])
}
fn variable(name: &str) -> Tree {
    Tree::map(vec![("variable", Tree::text(name))])
}
/// A spec whose first column is `ID`, read from `SRC.ID`; `extra` fields
/// are added or replaced.
pub(crate) fn document(columns: Vec<Tree>, extra: Vec<(&str, Tree)>) -> Tree {
    let mut all = vec![column(
        "ID",
        "int",
        derivation(op("source", variable("SRC.ID"))),
    )];
    all.extend(columns);
    let mut tree = Tree::map(vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("SRC.csv"))]),
            )]),
        ),
        ("keys", Tree::list(vec![Tree::text("ID")])),
        ("columns", Tree::list(all)),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("out.csv")),
                ("columns", Tree::list(vec![Tree::text("ID")])),
            ]),
        ),
    ]);
    for (field, value) in extra {
        tree.set(&[field], value);
    }
    tree
}
pub(crate) fn admit(tree: Tree) -> SpecificationDocument {
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
fn lowered(columns: Vec<Tree>, extra: Vec<(&str, Tree)>) -> Lowered {
    lower(&admit(document(columns, extra)), None)
}
fn root(spec: &Spec, column: usize) -> ExprId {
    spec.columns[column].derivation.as_ref().unwrap().expr
}
fn kind(spec: &Spec, column: usize) -> &ExprKind {
    &spec.expr(root(spec, column)).kind
}

#[test]
fn operations_outside_the_language_lower_to_unsupported_nodes() {
    let mut declared = column("ODD", "str", derivation(op("bogus", Tree::integer(1))));
    declared.set(
        &["verifications"],
        Tree::list(vec![op("not_a_check", Tree::map(vec![]))]),
    );
    let lowered = lowered(
        vec![
            declared,
            column("BAD", "str", derivation(op("compute", Tree::map(vec![])))),
        ],
        vec![(
            "verifications",
            Tree::list(vec![op("assert", Tree::map(vec![]))]),
        )],
    );
    let spec = lowered.spec();
    assert_eq!(kind(spec, 1), &ExprKind::Unsupported("bogus".into()));
    assert_eq!(
        spec.columns[1].checks[0].kind,
        crate::ast::ColumnCheckKind::Unsupported("not_a_check".into())
    );
    assert_eq!(kind(spec, 2), &ExprKind::Unsupported("compute".into()));
    assert_eq!(
        spec.checks[0].kind,
        crate::ast::DatasetCheckKind::Unsupported("assert".into())
    );
}

#[test]
fn positions_give_spans_and_defaults_take_their_ancestor_s() {
    let tree = document(
        vec![column(
            "CODE",
            "str",
            derivation(op("literal", Tree::text("A"))),
        )],
        vec![],
    );
    let spec = admit(tree);
    let nodes = spec.document().nodes().len();
    // Every node is written on its own line, except the literal's text,
    // which takes the position of its nearest written ancestor.
    let literal = spec
        .document()
        .nodes()
        .iter()
        .position(|node| matches!(node, N::Text(text) if text == "A"))
        .unwrap();
    let positions = Positions {
        files: vec![FileRef {
            identity: "spec.yaml".into(),
        }],
        nodes: (0..nodes)
            .map(|node| {
                (node != literal).then_some(Span {
                    file: crate::ast::FileId(0),
                    line: node as u32 + 1,
                    column: 1,
                })
            })
            .collect(),
    };
    let lowered = lower(&spec, Some(&positions));
    let lowered_spec = lowered.spec();
    assert_eq!(lowered_spec.files, positions.files);
    // The literal takes the line of its operation mapping, `{literal: A}`.
    let operation = spec
        .document()
        .nodes()
        .iter()
        .position(|node| matches!(node, N::Mapping(entries) if entries.len() == 1 && entries[0].1 == literal))
        .unwrap();
    let id = root(lowered_spec, 1);
    assert_eq!(lowered_spec.expr(id).meta.span.line, operation as u32 + 1);
    assert!(lowered_spec.meta.span.is_known());
    let unknown = lower(&spec, None);
    assert_eq!(unknown.spec().expr(id).meta.span, Span::UNKNOWN);
}

#[test]
fn shorthand_payloads_admitted_without_normalization_still_lower() {
    let lowered = lowered(
        vec![
            column("A", "str", derivation(op("source", Tree::text("SRC.A")))),
            column(
                "B",
                "str",
                derivation(op("str_template", Tree::text("{A}"))),
            ),
            column(
                "C",
                "str",
                derivation(op(
                    "case",
                    Tree::list(vec![Tree::map(vec![
                        ("when", Tree::text("A = 'x'")),
                        ("then", Tree::text("B")),
                    ])]),
                )),
            ),
            column(
                "D",
                "str",
                derivation(op(
                    "row_number",
                    Tree::map(vec![("window", Tree::text("named"))]),
                )),
            ),
        ],
        vec![],
    );
    let spec = lowered.spec();
    assert!(matches!(kind(spec, 1), ExprKind::Ref(name) if name.as_str() == "SRC.A"));
    assert!(matches!(
        kind(spec, 2),
        ExprKind::Scalar {
            op: ScalarOp::StrTemplate { .. },
            ..
        }
    ));
    let ExprKind::Case(arms) = kind(spec, 3) else {
        panic!("a case");
    };
    assert!(matches!(&spec.expr(arms[0].result).kind, ExprKind::Ref(name) if name.as_str() == "B"));
    let ExprKind::Window(window) = kind(spec, 4) else {
        panic!("a window");
    };
    assert!(window.order_by.is_empty() && window.group_by.is_empty());
}
