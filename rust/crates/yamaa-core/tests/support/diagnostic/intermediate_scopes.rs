//! Owned causes reached by the compiler and eager donor uniqueness descriptor.
use crate::support::document::Tree;
use crate::support::resolved::intermediates;
use std::collections::BTreeSet;
use yamaa_core::json::Value as V;
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    schema::{Document, SpecificationDocument},
    specification::{BindError, PreflightFinding, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::ColumnType,
};

fn source(reference: &str) -> Tree {
    Tree::map(vec![(
        "value",
        Tree::map(vec![(
            "source",
            Tree::map(vec![("variable", Tree::text(reference))]),
        )]),
    )])
}
fn model(mut extra: Vec<(&str, Tree)>, driver: bool) -> SpecificationDocument {
    let mut item = vec![("id", Tree::text("P")), ("dataset", Tree::text("D"))];
    item.append(&mut extra);
    let mut fields = vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("OUT")),
        ("base", Tree::text("SRC")),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        (
            "input",
            Tree::map(vec![
                ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                ("D", Tree::map(vec![("path", Tree::text("d.csv"))])),
            ]),
        ),
        ("intermediates", Tree::List(vec![Tree::map(item)])),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("out.csv")),
                ("columns", Tree::List(vec![Tree::text("ID")])),
            ]),
        ),
        (
            "columns",
            Tree::List(vec![Tree::map(vec![
                ("name", Tree::text("ID")),
                ("type", Tree::text("int")),
                ("derivation", source("SRC.ID")),
            ])]),
        ),
    ];
    if driver {
        fields.push((
            "rows",
            Tree::List(vec![Tree::map(vec![
                ("id", Tree::text("selected")),
                ("dataset", Tree::text("P")),
                ("derivations", Tree::map(vec![("ID", source("P.ID"))])),
            ])]),
        ));
    }
    let mut nodes = Vec::new();
    let root = Tree::map(fields).append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
fn read_model(self_target: bool) -> SpecificationDocument {
    let mut fields = vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("OUT")),
        ("base", Tree::text("SRC")),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        (
            "input",
            Tree::map(vec![
                ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                ("D", Tree::map(vec![("path", Tree::text("d.csv"))])),
            ]),
        ),
        (
            "intermediates",
            Tree::List(vec![
                Tree::map(vec![
                    ("id", Tree::text("EARLIER")),
                    (
                        "dataset",
                        Tree::text(if self_target { "SELF" } else { "SRC" }),
                    ),
                    (
                        "key",
                        Tree::map(vec![(
                            "ID",
                            Tree::text(if self_target { "ID" } else { "D.ABSENT" }),
                        )]),
                    ),
                ]),
                Tree::map(vec![
                    ("id", Tree::text("P")),
                    ("dataset", Tree::text("D")),
                    (
                        "derivations",
                        Tree::map(vec![("NEXT", source("EARLIER.ID"))]),
                    ),
                ]),
            ]),
        ),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("out.csv")),
                ("columns", Tree::List(vec![Tree::text("ID")])),
            ]),
        ),
        (
            "columns",
            Tree::List(vec![Tree::map(vec![
                ("name", Tree::text("ID")),
                ("type", Tree::text("int")),
                ("derivation", source("SRC.ID")),
            ])]),
        ),
    ];
    if self_target {
        fields.push((
            "rows",
            Tree::List(vec![Tree::map(vec![
                ("id", Tree::text("first")),
                ("dataset", Tree::text("SRC")),
                ("derivations", Tree::map(vec![("ID", source("SRC.ID"))])),
            ])]),
        ));
    }
    let mut nodes = Vec::new();
    let root = Tree::map(fields).append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
fn text(text: &str) -> V {
    V::String(text.into())
}
fn check(
    d: Diagnostic,
    phase: &str,
    condition: &str,
    requirement: &str,
    path: &str,
    context: Vec<(&str, V)>,
) -> ConditionCode {
    let definition = d.definition();
    assert_eq!(
        (
            definition.phase,
            definition.condition,
            definition.requirement
        ),
        (phase, condition, Some(requirement))
    );
    assert_eq!(d.spec_paths, [path]);
    assert_eq!(
        d.context,
        context
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect()
    );
    assert_eq!(d.source_span, None);
    assert_eq!(d.operand_route, None);
    d.code
}
pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let src = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let donor = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "X".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let mut extra_codes = BTreeSet::new();
    for self_target in [false, true] {
        let prepared = PreparedSpecification::prepare(&read_model(self_target)).unwrap();
        let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &donor]) else {
            panic!("contextual reader finding");
        };
        let diagnostics = findings
            .iter()
            .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1);
        let context = if self_target {
            vec![
                ("intermediate", text("P")),
                ("identifier", text("EARLIER.ID")),
            ]
        } else {
            vec![
                ("intermediate", text("P")),
                ("identifier", text("D.ABSENT")),
                ("read", text("EARLIER")),
            ]
        };
        extra_codes.insert(check(
            diagnostics[0].clone(),
            "validation",
            if self_target {
                "phase_boundary"
            } else {
                "unknown_field"
            },
            "LOOKUP-16",
            "intermediates[1].derivations.NEXT.source",
            context,
        ));
    }
    // Model documents hold the canonical class of the `unique` shorthand.
    let unique = || {
        (
            "verifications",
            Tree::List(vec![Tree::map(vec![(
                "unique",
                Tree::map(vec![("columns", Tree::List(vec![Tree::text("ID")]))]),
            )])]),
        )
    };
    let derive = || {
        (
            "derivations",
            Tree::map(vec![(
                "NEXT",
                Tree::map(vec![(
                    "value",
                    Tree::map(vec![(
                        "compute",
                        Tree::map(vec![("expr", Tree::text("X + SRC.ID"))]),
                    )]),
                )]),
            )]),
        )
    };
    let cases = vec![
        (
            vec![("derivations", Tree::map(vec![("NEXT", source("P.ID"))]))],
            false,
            "dependency_cycle",
            "LOOKUP-16",
            "intermediates[0].derivations.NEXT.source",
            vec![("cycle", V::Array(vec![text("P"), text("P")]))],
        ),
        (
            vec![("derivations", Tree::map(vec![("NEXT", source("D.ABSENT"))]))],
            false,
            "unknown_field",
            "LOOKUP-15",
            "intermediates[0].derivations.NEXT.source",
            vec![
                ("intermediate", text("P")),
                ("identifier", text("D.ABSENT")),
            ],
        ),
        (
            vec![("derivations", Tree::map(vec![("ID", source("D.X"))]))],
            false,
            "duplicate_derivation",
            "LOOKUP-15",
            "intermediates[0].derivations.ID",
            vec![
                ("intermediate", text("P")),
                ("derivation", text("ID")),
                ("identifier", text("D.ID")),
            ],
        ),
        (
            vec![(
                "derivations",
                Tree::map(vec![(
                    "NEXT",
                    Tree::map(vec![(
                        "value",
                        Tree::map(vec![(
                            "row_number",
                            Tree::map(vec![(
                                "window",
                                Tree::map(vec![
                                    ("group_by", Tree::List(vec![Tree::text("P.ID")])),
                                    ("order_by", Tree::List(vec![Tree::text("D.ID")])),
                                ]),
                            )]),
                        )]),
                    )]),
                )]),
            )],
            false,
            "unknown_field",
            "LOOKUP-16",
            "intermediates[0].derivations.NEXT.row_number.window.group_by[0]",
            vec![("intermediate", text("P")), ("identifier", text("P.ID"))],
        ),
        (
            vec![(
                "verifications",
                Tree::List(vec![Tree::map(vec![(
                    "unique",
                    Tree::map(vec![("columns", Tree::List(vec![Tree::text("ABSENT")]))]),
                )])]),
            )],
            false,
            "unknown_field",
            "LOOKUP-17",
            "intermediates[0].verifications[0].unique.columns[0]",
            vec![
                ("intermediate", text("P")),
                ("identifier", text("D.ABSENT")),
            ],
        ),
        (
            vec![unique(), ("filter", Tree::text("D.X > SRC.ID"))],
            false,
            "correlated_filter_with_unique_verification",
            "LOOKUP-17",
            "intermediates[0].verifications[0].unique",
            vec![("intermediate", text("P"))],
        ),
        (
            vec![unique(), derive()],
            false,
            "correlated_derivation_with_unique_verification",
            "LOOKUP-17",
            "intermediates[0].verifications[0].unique",
            vec![("intermediate", text("P"))],
        ),
        (
            vec![("key", Tree::List(vec![Tree::text("ID")]))],
            true,
            "invalid_intermediate_driver",
            "ROW-2",
            "rows[0].dataset",
            vec![
                ("intermediate", text("P")),
                ("fields", V::Array(vec![text("key")])),
            ],
        ),
        (
            // A case selecting an int or a string has a type decided per
            // value (EXPR-14), so no driver layout is known for it.
            vec![
                (
                    "derivations",
                    Tree::map(vec![(
                        "NEXT",
                        Tree::map(vec![(
                            "value",
                            Tree::map(vec![(
                                "case",
                                Tree::List(vec![
                                    Tree::map(vec![
                                        ("when", Tree::text("X > 0")),
                                        ("then", Tree::map(vec![("literal", Tree::integer(1))])),
                                    ]),
                                    Tree::map(vec![(
                                        "otherwise",
                                        Tree::map(vec![("literal", Tree::text("none"))]),
                                    )]),
                                ]),
                            )]),
                        )]),
                    )]),
                ),
                (
                    "columns",
                    Tree::List(vec![Tree::text("ID"), Tree::text("NEXT")]),
                ),
            ],
            true,
            "unknown_intermediate_driver_type",
            "ROW-2",
            "rows[0].dataset",
            vec![
                ("intermediate", text("P")),
                ("columns", V::Array(vec![text("NEXT")])),
            ],
        ),
    ];
    let mut reached = extra_codes;
    for (fields, driver, condition, requirement, path, context) in cases {
        let diagnostic = match PreparedSpecification::prepare(&model(fields, driver)) {
            Ok(prepared) => {
                let BindError::Invalid(mut findings) =
                    prepared.bind_sources(&[&src, &donor]).unwrap_err()
                else {
                    panic!("expected owned finding")
                };
                assert_eq!(findings.len(), 1);
                findings
                    .pop()
                    .unwrap()
                    .diagnostics(prepared.source())
                    .unwrap()
                    .pop()
                    .unwrap()
            }
            Err(PrepareError::Invalid(mut findings)) => {
                assert_eq!(findings.len(), 1);
                match findings.pop().unwrap() {
                    PreflightFinding::Scalar(diagnostic) => diagnostic,
                    other => panic!("unexpected preflight {other:?}"),
                }
            }
            Err(error) => panic!("unexpected preparation {error:?}"),
        };
        reached.insert(check(
            diagnostic,
            "validation",
            condition,
            requirement,
            path,
            context,
        ));
    }
    // A driver is admitted whenever each field it exposes has one result type
    // (EXPR-14): a formula over integers is an integer, padding is text.
    for (name, value, kind) in [
        (
            "NEXT",
            Tree::map(vec![(
                "compute",
                Tree::map(vec![("expr", Tree::text("X + 1"))]),
            )]),
            ColumnType::Int,
        ),
        (
            "PADDED",
            Tree::map(vec![(
                "str_pad",
                Tree::map(vec![
                    ("source", Tree::text("X")),
                    ("width", Tree::integer(3)),
                ]),
            )]),
            ColumnType::Str,
        ),
    ] {
        let driven = model(
            vec![
                (
                    "derivations",
                    Tree::map(vec![(name, Tree::map(vec![("value", value)]))]),
                ),
                (
                    "columns",
                    Tree::List(vec![Tree::text("ID"), Tree::text(name)]),
                ),
            ],
            true,
        );
        let plan = PreparedSpecification::prepare(&driven)
            .unwrap()
            .bind_sources(&[&src, &donor])
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let item = intermediates(&plan)[0];
        let derived = item.derivations.as_ref().unwrap();
        assert_eq!(
            derived.column_kind(donor.columns().len()),
            Some(kind),
            "{name}"
        );
    }
    let prepared = PreparedSpecification::prepare(&model(vec![unique()], false)).unwrap();
    let plan = prepared.bind_sources(&[&src, &donor]).unwrap();
    let item = intermediates(&plan)[0];
    reached.insert(check(
        item.unique[0].diagnostic(item, &donor, "D", 1),
        "verification",
        "duplicate_intermediate_records",
        "LOOKUP-17",
        "intermediates[0].verifications[0].unique",
        vec![
            ("intermediate", text("P")),
            ("dataset", text("D")),
            ("columns", V::Array(vec![text("ID")])),
            (
                "duplicate_count",
                V::Number(("1").parse().expect("canonical integer")),
            ),
        ],
    ));
    reached
}
