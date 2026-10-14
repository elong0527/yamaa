//! Owned causes reached by the compiler and eager donor uniqueness descriptor.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V, Diagnostic},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    specification::{BindError, PreflightFinding, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
enum Tree<'a> {
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
use Tree::*;
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Text(text) => N::Text(text.into()),
            List(items) => N::Sequence(items.into_iter().map(|item| item.append(nodes)).collect()),
            Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(key, value)| (Text(key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn source(reference: &str) -> Tree<'_> {
    Map(vec![(
        "value",
        Map(vec![("source", Map(vec![("variable", Text(reference))]))]),
    )])
}
fn model(mut extra: Vec<(&str, Tree<'_>)>, driver: bool) -> SpecificationDocument {
    let mut item = vec![("id", Text("P")), ("dataset", Text("D"))];
    item.append(&mut extra);
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("OUT")),
        ("base", Text("SRC")),
        ("keys", List(vec![Text("ID")])),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("src.csv"))])),
                ("D", Map(vec![("path", Text("d.csv"))])),
            ]),
        ),
        ("intermediates", List(vec![Map(item)])),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("ID")),
                ("type", Text("int")),
                ("derivation", source("SRC.ID")),
            ])]),
        ),
    ];
    if driver {
        fields.push((
            "rows",
            List(vec![Map(vec![
                ("id", Text("selected")),
                ("dataset", Text("P")),
                ("derivations", Map(vec![("ID", source("P.ID"))])),
            ])]),
        ));
    }
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn read_model(self_target: bool) -> SpecificationDocument {
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("OUT")),
        ("base", Text("SRC")),
        ("keys", List(vec![Text("ID")])),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("src.csv"))])),
                ("D", Map(vec![("path", Text("d.csv"))])),
            ]),
        ),
        (
            "intermediates",
            List(vec![
                Map(vec![
                    ("id", Text("EARLIER")),
                    ("dataset", Text(if self_target { "SELF" } else { "SRC" })),
                    (
                        "key",
                        Map(vec![(
                            "ID",
                            Text(if self_target { "ID" } else { "D.ABSENT" }),
                        )]),
                    ),
                ]),
                Map(vec![
                    ("id", Text("P")),
                    ("dataset", Text("D")),
                    ("derivations", Map(vec![("NEXT", source("EARLIER.ID"))])),
                ]),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
        (
            "columns",
            List(vec![Map(vec![
                ("name", Text("ID")),
                ("type", Text("int")),
                ("derivation", source("SRC.ID")),
            ])]),
        ),
    ];
    if self_target {
        fields.push((
            "rows",
            List(vec![Map(vec![
                ("id", Text("first")),
                ("dataset", Text("SRC")),
                ("derivations", Map(vec![("ID", source("SRC.ID"))])),
            ])]),
        ));
    }
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn text(text: &str) -> V {
    V::Scalar(Value::Str(text.into()))
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
pub(super) fn reached() -> BTreeSet<ConditionCode> {
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
            "REQ-1263",
            "intermediates[1].derivations.NEXT.source",
            context,
        ));
    }
    let unique = || {
        (
            "verifications",
            List(vec![Map(vec![("unique", List(vec![Text("ID")]))])]),
        )
    };
    let derive = || {
        (
            "derivations",
            Map(vec![(
                "NEXT",
                Map(vec![(
                    "value",
                    Map(vec![("compute", Map(vec![("expr", Text("X + SRC.ID"))]))]),
                )]),
            )]),
        )
    };
    let cases = vec![
        (
            vec![("derivations", Map(vec![("NEXT", source("D.ABSENT"))]))],
            false,
            "unknown_field",
            "REQ-1185",
            "intermediates[0].derivations.NEXT.source",
            vec![
                ("intermediate", text("P")),
                ("identifier", text("D.ABSENT")),
            ],
        ),
        (
            vec![("derivations", Map(vec![("ID", source("D.X"))]))],
            false,
            "duplicate_derivation",
            "REQ-1185",
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
                Map(vec![(
                    "NEXT",
                    Map(vec![(
                        "value",
                        Map(vec![(
                            "row_number",
                            Map(vec![(
                                "window",
                                Map(vec![
                                    ("group_by", List(vec![Text("P.ID")])),
                                    ("order_by", List(vec![Text("D.ID")])),
                                ]),
                            )]),
                        )]),
                    )]),
                )]),
            )],
            false,
            "unknown_field",
            "REQ-1263",
            "intermediates[0].derivations.NEXT.row_number.window.group_by[0]",
            vec![("intermediate", text("P")), ("identifier", text("P.ID"))],
        ),
        (
            vec![(
                "verifications",
                List(vec![Map(vec![("unique", List(vec![Text("ABSENT")]))])]),
            )],
            false,
            "unknown_field",
            "REQ-1245",
            "intermediates[0].verifications[0].unique[0]",
            vec![
                ("intermediate", text("P")),
                ("identifier", text("D.ABSENT")),
            ],
        ),
        (
            vec![unique(), ("filter", Text("D.X > SRC.ID"))],
            false,
            "correlated_filter_with_unique_verification",
            "REQ-1245",
            "intermediates[0].verifications[0].unique",
            vec![("intermediate", text("P"))],
        ),
        (
            vec![unique(), derive()],
            false,
            "correlated_derivation_with_unique_verification",
            "REQ-1245",
            "intermediates[0].verifications[0].unique",
            vec![("intermediate", text("P"))],
        ),
        (
            vec![("key", List(vec![Text("ID")]))],
            true,
            "invalid_intermediate_driver",
            "REQ-1262",
            "rows[0].dataset",
            vec![
                ("intermediate", text("P")),
                ("fields", V::Sequence(vec![text("key")])),
            ],
        ),
        (
            vec![
                (
                    "derivations",
                    Map(vec![(
                        "NEXT",
                        Map(vec![(
                            "value",
                            Map(vec![("compute", Map(vec![("expr", Text("X + 1"))]))]),
                        )]),
                    )]),
                ),
                ("columns", List(vec![Text("ID"), Text("NEXT")])),
            ],
            true,
            "unknown_intermediate_driver_type",
            "REQ-1262",
            "rows[0].dataset",
            vec![
                ("intermediate", text("P")),
                ("columns", V::Sequence(vec![text("NEXT")])),
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
    let prepared = PreparedSpecification::prepare(&model(vec![unique()], false)).unwrap();
    let plan = prepared.bind_sources(&[&src, &donor]).unwrap();
    let item = &plan.intermediates()[0];
    reached.insert(check(
        item.unique[0].diagnostic(item, &donor, "D", 1),
        "verification",
        "duplicate_intermediate_records",
        "REQ-1245",
        "intermediates[0].verifications[0].unique",
        vec![
            ("intermediate", text("P")),
            ("dataset", text("D")),
            ("columns", V::Sequence(vec![text("ID")])),
            ("duplicate_count", V::Integer("1".into())),
        ],
    ));
    reached
}
