use super::*;
use crate::{
    diagnostic::ConditionCode as C,
    schema::{DocumentLimits, SpecificationDocument, ValidationBudget},
};

#[derive(Clone)]
enum Tree<'a> {
    Scalar(N),
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Text(text) => N::Text(text.into()),
            Self::List(values) => N::Sequence(
                values
                    .into_iter()
                    .map(|value| value.append(nodes))
                    .collect(),
            ),
            Self::Map(values) => N::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| (Self::Text(key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
}
fn operation<'a>(name: &'a str, fields: Vec<(&'a str, Tree<'a>)>) -> Tree<'a> {
    Tree::Map(vec![(name, Tree::Map(fields))])
}
fn literal(value: N) -> Tree<'static> {
    Tree::Map(vec![("literal", Tree::Scalar(value))])
}
fn column<'a>(name: &'a str, kind: &'a str, value: Tree<'a>) -> Tree<'a> {
    use Tree::*;
    Map(vec![
        ("name", Text(name)),
        ("type", Text(kind)),
        ("derivation", Map(vec![("value", value)])),
    ])
}
fn document(value: Tree<'_>, verification: Option<Tree<'_>>) -> SpecificationDocument {
    use Tree::*;
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("SRC.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column(
                    "ID",
                    "int",
                    operation("source", vec![("variable", Text("SRC.ID"))]),
                ),
                column("TERM", "str", literal(N::Text("alpha".into()))),
                column("DAY", "date", literal(N::Null)),
                column("COUNT", "int", literal(N::Integer("1".into()))),
                column("RESULT", "float", value),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("RESULT")])),
            ]),
        ),
    ];
    if let Some(verification) = verification {
        fields.push(("verifications", List(vec![verification])));
    }
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
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
fn binding_codes(value: Tree<'_>, verification: Option<Tree<'_>>) -> Vec<C> {
    let spec = PreparedSpecification::prepare(&document(value, verification)).unwrap();
    let BindError::Invalid(findings) = spec.bind(&source()).unwrap_err() else {
        panic!("typed binding findings")
    };
    findings
        .into_iter()
        .flat_map(|finding| finding.diagnostics(spec.source()).unwrap())
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn independent_case_types_survive_an_unknown_operand() {
    use Tree::*;
    let case = Map(vec![(
        "case",
        List(vec![Map(vec![
            (
                "when",
                Text("UNKNOWN = 1 OR DAY = '2025-01-01' OR COUNT LIKE '1%'"),
            ),
            ("then", literal(N::Integer("1".into()))),
        ])]),
    )]);
    let codes = binding_codes(case, None);
    assert_eq!(codes.len(), 3);
    assert!(codes.contains(&C::PredicateIncompatiblePair));
    assert!(codes.contains(&C::PredicateExpectedText));
}

#[test]
fn compute_and_reduction_reject_known_text_without_evaluation() {
    assert_eq!(
        binding_codes(
            operation("compute", vec![("expr", Tree::Text("TERM + 1"))]),
            None
        ),
        vec![C::NumericInputType]
    );
    assert_eq!(
        binding_codes(
            operation(
                "aggregate",
                vec![
                    ("group_by", Tree::List(vec![Tree::Text("ID")])),
                    ("expr", Tree::Text("SUM(TERM)"))
                ]
            ),
            None
        ),
        vec![C::DatasetReductionInputType]
    );
}

#[test]
fn numeric_and_aggregate_result_kinds_are_native_metadata() {
    let numeric = crate::numeric_compiler::compile_numeric_with_policy(
        "A + POWER(B, 2) / -3",
        "result",
        Default::default(),
        crate::numeric_compiler::MathPolicy::PortableLibmV1,
    )
    .unwrap();
    assert_eq!(numeric.result_type(), ColumnType::Float);
    let aggregate = crate::aggregate_compiler::compile_aggregate(
        "SUM(EX.DOSE) / NULLIF(EX.PLANDOSE, 0)",
        "result",
        Default::default(),
    )
    .unwrap();
    assert_eq!(aggregate.numeric_result_type(), Some(ColumnType::Float));
    let bare =
        crate::aggregate_compiler::compile_aggregate("MIN(EX.TERM)", "result", Default::default())
            .unwrap();
    assert_eq!(bare.numeric_result_type(), None);
}

#[test]
fn independent_calendar_findings_keep_first_error_priority() {
    use crate::{
        dataset::{date_policy, date_policy_findings, DatePolicyError, DayDeclaration},
        temporal::DatePrecision,
    };
    let findings = date_policy_findings(
        Some("13"),
        DayDeclaration::Number("0"),
        DatePrecision::Year,
        "date",
    )
    .unwrap()
    .unwrap_err();
    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        vec![C::ImputationMonthRange, C::ImputationDayRange]
    );
    let DatePolicyError::Diagnostic(first) = date_policy(
        Some("13"),
        DayDeclaration::Number("0"),
        DatePrecision::Year,
        "date",
    )
    .unwrap_err() else {
        panic!("first finding")
    };
    assert_eq!(first, findings[0]);
    let invalid = operation(
        "date_impute",
        vec![
            ("source", Tree::Text("TERM")),
            ("month", Tree::Scalar(N::Integer("13".into()))),
            ("day", Tree::Scalar(N::Integer("0".into()))),
        ],
    );
    let PrepareError::Invalid(findings) =
        PreparedSpecification::prepare(&document(invalid, None)).unwrap_err()
    else {
        panic!("two declaration findings")
    };
    assert_eq!(
        findings
            .into_iter()
            .map(|finding| finding.diagnostic().code)
            .collect::<Vec<_>>(),
        vec![C::ImputationMonthRange, C::ImputationDayRange]
    );
}

#[test]
fn independent_cut_findings_keep_first_error_priority() {
    use crate::mapping_cut::{AdmissionError, Cut};
    let labels = vec![String::from("a")];
    let findings = Cut::declaration_findings("COUNT", "cut", &[10.0, 5.0], &labels).unwrap();
    assert_eq!(findings.len(), 2);
    assert_ne!(findings[0].context, findings[1].context);
    let AdmissionError::Diagnostic(first) = Cut::new(
        "COUNT".into(),
        "cut".into(),
        vec![10.0, 5.0],
        labels,
        false,
        None,
    )
    .unwrap_err() else {
        panic!("first finding")
    };
    assert_eq!(first, findings[0]);
    let invalid = operation(
        "cut",
        vec![
            ("source", Tree::Text("COUNT")),
            (
                "breaks",
                Tree::List(vec![
                    Tree::Scalar(N::Integer("10".into())),
                    Tree::Scalar(N::Integer("5".into())),
                ]),
            ),
            ("labels", Tree::List(vec![Tree::Text("a")])),
        ],
    );
    let PrepareError::Invalid(findings) =
        PreparedSpecification::prepare(&document(invalid, None)).unwrap_err()
    else {
        panic!("two declaration findings")
    };
    assert_eq!(findings.len(), 2);
}

#[test]
fn repeated_type_findings_refuse_whole_projection_before_output() {
    let mut terms = (0..1024)
        .map(|index| format!("X{index}"))
        .collect::<Vec<_>>();
    while terms.len() > 1 {
        terms = terms
            .chunks_exact(2)
            .map(|pair| format!("({} + {})", pair[0], pair[1]))
            .collect();
    }
    let expression = terms.pop().unwrap();
    let plan = crate::numeric_compiler::compile_numeric(&expression, "result", Default::default())
        .unwrap();
    assert!(matches!(
        numeric_kinds(&plan, |_| Some(crate::value::ValueType::Str)),
        Err(BindError::PredicateDiagnosticLimit)
    ));
}

fn phase_document(
    columns: Vec<Tree<'_>>,
    intermediates: Vec<Tree<'_>>,
    rows: Vec<Tree<'_>>,
) -> SpecificationDocument {
    use Tree::*;
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("SRC.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        ("columns", List(columns)),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
    ];
    if !intermediates.is_empty() {
        fields.push(("intermediates", List(intermediates)));
    }
    if !rows.is_empty() {
        fields.push(("rows", List(rows)));
    }
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn empty_column(name: &str) -> Tree<'_> {
    Tree::Map(vec![
        ("name", Tree::Text(name)),
        ("type", Tree::Text("int")),
    ])
}
fn source_value(name: &str) -> Tree<'_> {
    operation("source", vec![("variable", Tree::Text(name))])
}
fn row_value(value: Tree<'_>) -> Tree<'_> {
    Tree::Map(vec![("value", value)])
}
fn row_number() -> Tree<'static> {
    use Tree::*;
    operation(
        "row_number",
        vec![(
            "window",
            Map(vec![(
                "order_by",
                List(vec![Map(vec![("variable", Text("ID"))])]),
            )]),
        )],
    )
}
fn phase_findings(
    spec: &PreparedSpecification,
    schema: &TableSchema,
) -> Vec<crate::diagnostic::Diagnostic> {
    let BindError::Invalid(findings) = spec.bind(schema).unwrap_err() else {
        panic!("canonical binding diagnostics")
    };
    findings
        .into_iter()
        .flat_map(|finding| finding.diagnostics(spec.source()).unwrap())
        .collect()
}

#[test]
fn only_ungrouped_filters_refuse_direct_and_transitive_window_reads() {
    use Tree::*;
    for grouped in [false, true] {
        let mut row = vec![
            ("id", Text("candidate")),
            ("filter", Text("ALIAS > 1")),
            (
                "derivations",
                Map(vec![
                    ("ID", row_value(source_value("SRC.ID"))),
                    ("SEQ", row_value(row_number())),
                    ("ALIAS", row_value(source_value("SEQ"))),
                ]),
            ),
        ];
        if grouped {
            row.push(("group_by", List(vec![Text("SRC.ID")])));
        }
        let spec = PreparedSpecification::prepare(&phase_document(
            vec![
                empty_column("ID"),
                empty_column("SEQ"),
                empty_column("ALIAS"),
            ],
            vec![],
            vec![Map(row)],
        ))
        .unwrap();
        if grouped {
            assert!(spec.bind(&source()).is_ok());
        } else {
            let findings = phase_findings(&spec, &source());
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].code, C::RowFilterPhaseBoundary);
            assert_eq!(
                findings[0].context["identifier"],
                crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str("ALIAS".into()))
            );
        }
    }
}

#[test]
fn a_deferred_lookup_match_has_its_authored_filter_phase_witness() {
    use Tree::*;
    let intermediate = Map(vec![
        ("id", Text("COLLECTED")),
        ("dataset", Text("SRC")),
        ("key", List(vec![Text("ID"), Text("SEQ")])),
        ("columns", List(vec![Text("VAL")])),
    ]);
    let row = Map(vec![
        ("id", Text("candidate")),
        ("filter", Text("COLLECTED.VAL IS NULL")),
        (
            "derivations",
            Map(vec![
                ("ID", row_value(source_value("SRC.ID"))),
                ("SEQ", row_value(row_number())),
                ("VAL", row_value(source_value("SRC.VAL"))),
            ]),
        ),
    ]);
    let schema = TableSchema::new(
        ["ID", "SEQ", "VAL"]
            .into_iter()
            .map(|name| Column {
                name: name.into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap();
    let spec = PreparedSpecification::prepare(&phase_document(
        vec![empty_column("ID"), empty_column("SEQ"), empty_column("VAL")],
        vec![intermediate],
        vec![row],
    ))
    .unwrap();
    let findings = phase_findings(&spec, &schema);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, C::RowFilterPhaseBoundary);
    for (key, expected) in [
        ("identifier", "COLLECTED.VAL"),
        ("lookup_match", "SEQ"),
        ("row", "candidate"),
        ("available_phase", "window_derivation"),
        ("required_phase", "row_filter"),
    ] {
        assert_eq!(
            findings[0].context[key],
            crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(expected.into()))
        );
    }
}

#[test]
fn input_backed_defaults_join_self_donors_while_self_backed_columns_stay_late() {
    use crate::dataset::Expression;
    use Tree::*;
    for (named, self_backed) in [(false, false), (true, false), (true, true)] {
        let prior = Map(vec![
            ("id", Text("PRIOR_SAE")),
            ("dataset", Text("SELF")),
            ("key", List(vec![Text("ID")])),
            ("filter", Text("SELF.LATE > 0 AND SELF.ALIAS > 0")),
            (
                "order_by",
                List(vec![Map(vec![("variable", Text("SELF.SEQ"))])]),
            ),
            ("keep", Text("first")),
        ]);
        let first = Map(vec![
            ("id", Text("FIRST_AE")),
            ("dataset", Text(if self_backed { "SELF" } else { "SRC" })),
            ("key", List(vec![Text("ID")])),
            (
                "order_by",
                List(vec![Map(vec![(
                    "variable",
                    Text(if self_backed { "SELF.SEQ" } else { "SRC.SEQ" }),
                )])]),
            ),
            ("keep", Text("first")),
        ]);
        let row = |id, prior| {
            Map(vec![
                ("id", Text(id)),
                (
                    "derivations",
                    Map(vec![
                        ("ID", row_value(source_value("SRC.ID"))),
                        ("SEQ", row_value(source_value("SRC.SEQ"))),
                        ("PRIOR", row_value(prior)),
                    ]),
                ),
            ])
        };
        let spec = PreparedSpecification::prepare(&phase_document(
            vec![
                empty_column("ID"),
                empty_column("SEQ"),
                empty_column("PRIOR"),
                column(
                    "LATE",
                    "int",
                    source_value(if named { "FIRST_AE.SEQ" } else { "SRC.SEQ" }),
                ),
                column("ALIAS", "int", source_value("LATE")),
            ],
            vec![prior, first],
            vec![
                row("first", literal(N::Null)),
                row("second", source_value("PRIOR_SAE.SEQ")),
            ],
        ))
        .unwrap();
        let schema = TableSchema::new(
            ["ID", "SEQ"]
                .into_iter()
                .map(|name| Column {
                    name: name.into(),
                    kind: ColumnType::Int,
                })
                .collect(),
        )
        .unwrap();
        if !self_backed {
            // Both direct input reads and input-backed named selections are
            // row defaults under REQ-1260. LATE and ALIAS are therefore derived
            // in every template and available to its completed SELF donors.
            let plan = spec.bind(&schema).unwrap();
            assert!(plan.columns().is_empty());
            assert_eq!(plan.templates().len(), 2);
            assert_eq!(
                plan.output()
                    .columns()
                    .iter()
                    .map(|column| (column.name.as_str(), column.kind))
                    .collect::<Vec<_>>(),
                ["ID", "SEQ", "PRIOR", "LATE", "ALIAS"].map(|name| (name, ColumnType::Int))
            );
            let donor = &plan.intermediates()[0];
            assert!(donor.source.is_self());
            assert_eq!(
                donor.source_schema(plan.secondary()).unwrap(),
                plan.output()
            );
            assert_eq!(donor.source_column(plan.secondary(), "LATE"), Some(3));
            assert_eq!(donor.source_column(plan.secondary(), "ALIAS"), Some(4));
            for template in plan.templates() {
                let assignment = |column| {
                    template
                        .assignments
                        .iter()
                        .find(|assignment| assignment.column == column)
                        .unwrap()
                };
                assert_eq!(assignment(3).path, "columns.LATE.derivation.source");
                assert_eq!(
                    assignment(3).expression,
                    if named {
                        Expression::Intermediate {
                            index: 1,
                            column: 1,
                        }
                    } else {
                        Expression::Source(1)
                    }
                );
                assert_eq!(assignment(4).path, "columns.ALIAS.derivation.source");
                assert_eq!(assignment(4).expression, Expression::Column(3));
            }
        } else {
            // SELF-backed LATE remains a column derivation over all completed
            // templates (REQ-0120), so neither it nor ALIAS is a donor field.
            let findings = phase_findings(&spec, &schema);
            let excluded = findings
                .iter()
                .filter(|finding| finding.code == C::LookupSelectionReference)
                .collect::<Vec<_>>();
            assert_eq!(excluded.len(), 2);
            for (finding, name) in excluded.into_iter().zip(["SELF.LATE", "SELF.ALIAS"]) {
                assert_eq!(finding.spec_paths, ["intermediates[0].filter"]);
                assert_eq!(
                    finding.context["identifier"],
                    crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(name.into()))
                );
            }
        }
    }
}

#[test]
fn donor_filter_observes_the_computed_intermediate_type() {
    use Tree::*;
    let intermediate = Map(vec![
        ("id", Text("EOT")),
        ("dataset", Text("SRC")),
        ("filter", Text("SRC.EOT_FALLBACK = 'x'")),
        (
            "derivations",
            Map(vec![(
                "EOT_FALLBACK",
                row_value(operation(
                    "compute",
                    vec![("expr", Text("SEQ + VAL / 1000"))],
                )),
            )]),
        ),
    ]);
    let spec = PreparedSpecification::prepare(&phase_document(
        vec![column("ID", "int", source_value("SRC.ID"))],
        vec![intermediate],
        vec![],
    ))
    .unwrap();
    let schema = TableSchema::new(
        ["ID", "SEQ", "VAL"]
            .into_iter()
            .map(|name| Column {
                name: name.into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap();
    let findings = phase_findings(&spec, &schema);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, C::PredicateIncompatiblePair);
    assert_eq!(
        findings[0].spec_paths,
        [String::from("intermediates[0].filter")]
    );
}

#[test]
fn explicit_named_row_reads_still_promote_their_local_match_defaults() {
    use Tree::*;
    let first = Map(vec![
        ("id", Text("FIRST_AE")),
        ("dataset", Text("SRC")),
        ("key", Map(vec![("SEQ", Text("CURRENT"))])),
    ]);
    let row = Map(vec![
        ("id", Text("candidate")),
        (
            "derivations",
            Map(vec![
                ("ID", row_value(source_value("SRC.ID"))),
                ("VAL", row_value(source_value("FIRST_AE.SEQ"))),
            ]),
        ),
    ]);
    let spec = PreparedSpecification::prepare(&phase_document(
        vec![
            empty_column("ID"),
            column("CURRENT", "int", source_value("SRC.SEQ")),
            empty_column("VAL"),
        ],
        vec![first],
        vec![row],
    ))
    .unwrap();
    let schema = TableSchema::new(
        ["ID", "SEQ"]
            .into_iter()
            .map(|name| Column {
                name: name.into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap();
    spec.bind(&schema).unwrap();
}
