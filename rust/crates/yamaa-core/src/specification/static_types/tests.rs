use super::*;
use crate::{diagnostic::ConditionCode as C, schema::SpecificationDocument};

use crate::test_document::Tree;

fn operation<'a>(name: &'a str, fields: Vec<(&'a str, Tree)>) -> Tree {
    Tree::map(vec![(name, Tree::map(fields))])
}
fn literal(value: N) -> Tree {
    Tree::map(vec![("literal", Tree::Scalar(value))])
}
fn column<'a>(name: &'a str, kind: &'a str, value: Tree) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text(kind)),
        ("derivation", Tree::map(vec![("value", value)])),
    ])
}
fn document(value: Tree, verification: Option<Tree>) -> SpecificationDocument {
    let mut fields = vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("SRC.csv"))]),
            )]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        (
            "columns",
            Tree::List(vec![
                column(
                    "ID",
                    "int",
                    operation("source", vec![("variable", Tree::text("SRC.ID"))]),
                ),
                column("TERM", "str", literal(N::Text("alpha".into()))),
                column("DAY", "date", literal(N::Null)),
                column("COUNT", "int", literal(N::Integer("1".into()))),
                column("RESULT", "float", value),
            ]),
        ),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("result.csv")),
                (
                    "columns",
                    Tree::List(vec![Tree::text("ID"), Tree::text("RESULT")]),
                ),
            ]),
        ),
    ];
    if let Some(verification) = verification {
        fields.push(("verifications", Tree::List(vec![verification])));
    }
    let mut nodes = Vec::new();
    let root = Tree::map(fields).append(&mut nodes);
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
fn binding_codes(value: Tree, verification: Option<Tree>) -> Vec<C> {
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
    let case = Tree::map(vec![(
        "case",
        Tree::List(vec![Tree::map(vec![
            (
                "when",
                Tree::text("UNKNOWN = 1 OR DAY = '2025-01-01' OR COUNT LIKE '1%'"),
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
fn predicate_arithmetic_operands_are_typed_as_in_compute() {
    let case = |when: &str| {
        Tree::map(vec![(
            "case",
            Tree::List(vec![Tree::map(vec![
                ("when", Tree::text(when)),
                ("then", literal(N::Integer("1".into()))),
            ])]),
        )])
    };
    // Each non-numeric operand of arithmetic is reported once, and arithmetic
    // over a faulty operand adds no comparison finding of its own.
    assert_eq!(
        binding_codes(
            case("TERM + 1 > 0 OR DAY - 1 < COUNT OR TERM * 2 = 1"),
            None
        ),
        vec![C::NumericInputType, C::NumericInputType]
    );
    let spec = PreparedSpecification::prepare(&document(case("COUNT * 2 > ID - 1"), None)).unwrap();
    assert!(spec.bind(&source()).is_ok());
}

#[test]
fn compute_and_reduction_reject_known_text_without_evaluation() {
    assert_eq!(
        binding_codes(
            operation("compute", vec![("expr", Tree::text("TERM + 1"))]),
            None
        ),
        vec![C::NumericInputType]
    );
    assert_eq!(
        binding_codes(
            operation(
                "aggregate",
                vec![
                    ("group_by", Tree::List(vec![Tree::text("ID")])),
                    ("expr", Tree::text("SUM(TERM)"))
                ]
            ),
            None
        ),
        vec![C::DatasetReductionInputType]
    );
}

/// EXPR-14: `compute` and `aggregate` take their type from their operands by
/// VALUE-10 and FORMULA-23, int while every operator and operand keeps int.
#[test]
fn numeric_and_aggregate_result_types_follow_their_operands() {
    use crate::expression::{compile, Context};
    use crate::result_type::ResultType;
    let kinds = |name: &str| match name {
        "A" | "EX.COUNT" => ResultType::INT,
        "B" | "EX.DOSE" => ResultType::FLOAT,
        "EX.TERM" => ResultType::STR,
        _ => ResultType::UNKNOWN,
    };
    let result = |text: &str, context| compile(text, context, "result").unwrap().result_type(kinds);
    assert_eq!(result("A + 1", Context::Compute), ResultType::INT);
    assert_eq!(
        result("ABS(A) * MOD(A, 3)", Context::Compute),
        ResultType::INT
    );
    assert_eq!(result("A + B", Context::Compute), ResultType::FLOAT);
    assert_eq!(result("A / 1", Context::Compute), ResultType::FLOAT);
    assert_eq!(result("FLOOR(A)", Context::Compute), ResultType::FLOAT);
    assert_eq!(
        result("A + POWER(B, 2) / -3", Context::Compute),
        ResultType::FLOAT
    );
    assert_eq!(result("NULLIF(A, 0)", Context::Compute), ResultType::INT);
    assert_eq!(result("NULLIF(A, B)", Context::Compute), ResultType::FLOAT);
    assert_eq!(result("NULLIF(B, A)", Context::Compute), ResultType::FLOAT);
    assert_eq!(result("NULLIF(A, NULL)", Context::Compute), ResultType::INT);
    assert_eq!(result("A + NULL", Context::Compute), ResultType::MISSING);
    assert_eq!(result("UNTYPED + 1", Context::Compute), ResultType::NUMBER);
    assert_eq!(result("SUM(EX.COUNT)", Context::Aggregate), ResultType::INT);
    assert_eq!(result("COUNT(EX.*)", Context::Aggregate), ResultType::INT);
    assert_eq!(
        result("MEAN(EX.COUNT)", Context::Aggregate),
        ResultType::FLOAT
    );
    assert_eq!(result("MIN(EX.TERM)", Context::Aggregate), ResultType::STR);
    assert_eq!(
        result("ONLY(EX.DOSE)", Context::Aggregate),
        ResultType::FLOAT
    );
    assert_eq!(
        result("SUM(EX.DOSE) / NULLIF(EX.COUNT, 0)", Context::Aggregate),
        ResultType::FLOAT
    );
    assert_eq!(
        result("SUM(EX.COUNT) - COUNT(EX.*)", Context::Aggregate),
        ResultType::INT
    );
}

#[test]
fn independent_calendar_findings_keep_first_error_priority() {
    use crate::{
        dataset::{
            completion_policy, completion_policy_findings, DatePolicyError, DayDeclaration,
            TemporalType,
        },
        temporal::Precision,
    };
    let findings = completion_policy_findings(
        TemporalType::Date,
        Some("13"),
        Some(DayDeclaration::Number("0")),
        Precision::Year,
        None,
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
    let DatePolicyError::Diagnostic(first) = completion_policy(
        TemporalType::Date,
        Some("13"),
        Some(DayDeclaration::Number("0")),
        Precision::Year,
        None,
        "date",
    )
    .unwrap_err() else {
        panic!("first finding")
    };
    assert_eq!(first, findings[0]);
    let invalid = operation(
        "impute",
        vec![
            ("type", Tree::text("date")),
            ("source", Tree::text("TERM")),
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
    let findings = Cut::declaration_findings("cut", &[10.0, 5.0], &labels);
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
            ("source", Tree::text("COUNT")),
            (
                "breaks",
                Tree::List(vec![
                    Tree::Scalar(N::Integer("10".into())),
                    Tree::Scalar(N::Integer("5".into())),
                ]),
            ),
            ("labels", Tree::List(vec![Tree::text("a")])),
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
fn repeated_type_findings_keep_the_complete_ordered_context() {
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
    let plan =
        crate::expression::compile(&expression, crate::expression::Context::Compute, "result")
            .unwrap();
    let findings = numeric_kinds(&plan, |_| Some(crate::value::ValueType::Str));
    assert_eq!(findings.len(), 1024);
    for (index, finding) in findings.iter().enumerate() {
        assert_eq!(finding.code, C::NumericInputType);
        assert_eq!(finding.spec_paths, ["result"]);
        assert_eq!(
            finding.context["source"],
            crate::json::Value::from(format!("X{index}"))
        );
        assert_eq!(
            finding.context["expr"],
            crate::json::Value::from(expression.as_str())
        );
        assert_eq!(
            finding.context["expected"],
            crate::json::Value::from("numeric")
        );
        assert_eq!(finding.context["actual"], crate::json::Value::from("str"));
    }
}

fn phase_document(
    columns: Vec<Tree>,
    intermediates: Vec<Tree>,
    rows: Vec<Tree>,
) -> SpecificationDocument {
    let mut fields = vec![
        ("schema_version", Tree::text("1.0")),
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("SRC.csv"))]),
            )]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        ("columns", Tree::List(columns)),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("result.csv")),
                ("columns", Tree::List(vec![Tree::text("ID")])),
            ]),
        ),
    ];
    if !intermediates.is_empty() {
        fields.push(("intermediates", Tree::List(intermediates)));
    }
    if !rows.is_empty() {
        fields.push(("rows", Tree::List(rows)));
    }
    let mut nodes = Vec::new();
    let root = Tree::map(fields).append(&mut nodes);
    SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap()
}
fn empty_column(name: &str) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text("int")),
    ])
}
fn source_value(name: &str) -> Tree {
    operation("source", vec![("variable", Tree::text(name))])
}
fn row_value(value: Tree) -> Tree {
    Tree::map(vec![("value", value)])
}
fn row_number() -> Tree {
    operation(
        "row_number",
        vec![(
            "window",
            Tree::map(vec![(
                "order_by",
                Tree::List(vec![Tree::map(vec![("variable", Tree::text("ID"))])]),
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
    for grouped in [false, true] {
        let mut row = vec![
            ("id", Tree::text("candidate")),
            ("filter", Tree::text("ALIAS > 1")),
            (
                "derivations",
                Tree::map(vec![
                    ("ID", row_value(source_value("SRC.ID"))),
                    ("SEQ", row_value(row_number())),
                    ("ALIAS", row_value(source_value("SEQ"))),
                ]),
            ),
        ];
        if grouped {
            row.push(("group_by", Tree::List(vec![Tree::text("SRC.ID")])));
        }
        let spec = PreparedSpecification::prepare(&phase_document(
            vec![
                empty_column("ID"),
                empty_column("SEQ"),
                empty_column("ALIAS"),
            ],
            vec![],
            vec![Tree::map(row)],
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
                crate::json::Value::String("ALIAS".into())
            );
        }
    }
}

#[test]
fn a_deferred_lookup_match_has_its_authored_filter_phase_witness() {
    let intermediate = Tree::map(vec![
        ("id", Tree::text("COLLECTED")),
        ("dataset", Tree::text("SRC")),
        ("key", Tree::List(vec![Tree::text("ID"), Tree::text("SEQ")])),
        ("columns", Tree::List(vec![Tree::text("VAL")])),
    ]);
    let row = Tree::map(vec![
        ("id", Tree::text("candidate")),
        ("filter", Tree::text("COLLECTED.VAL IS NULL")),
        (
            "derivations",
            Tree::map(vec![
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
            crate::json::Value::String(expected.into())
        );
    }
}

#[test]
fn input_backed_defaults_join_self_donors_while_self_backed_columns_stay_late() {
    use crate::dataset::Expression;

    for (named, self_backed) in [(false, false), (true, false), (true, true)] {
        let prior = Tree::map(vec![
            ("id", Tree::text("PRIOR_SAE")),
            ("dataset", Tree::text("SELF")),
            ("key", Tree::List(vec![Tree::text("ID")])),
            ("filter", Tree::text("SELF.LATE > 0 AND SELF.ALIAS > 0")),
            (
                "order_by",
                Tree::List(vec![Tree::map(vec![("variable", Tree::text("SELF.SEQ"))])]),
            ),
            ("keep", Tree::text("first")),
        ]);
        let first = Tree::map(vec![
            ("id", Tree::text("FIRST_AE")),
            (
                "dataset",
                Tree::text(if self_backed { "SELF" } else { "SRC" }),
            ),
            ("key", Tree::List(vec![Tree::text("ID")])),
            (
                "order_by",
                Tree::List(vec![Tree::map(vec![(
                    "variable",
                    Tree::text(if self_backed { "SELF.SEQ" } else { "SRC.SEQ" }),
                )])]),
            ),
            ("keep", Tree::text("first")),
        ]);
        let row = |id, prior| {
            Tree::map(vec![
                ("id", Tree::text(id)),
                (
                    "derivations",
                    Tree::map(vec![
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
            assert!(plan.column_bindings().is_empty());
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
                    crate::json::Value::String(name.into())
                );
            }
        }
    }
}

#[test]
fn donor_filter_observes_the_computed_intermediate_type() {
    let intermediate = Tree::map(vec![
        ("id", Tree::text("EOT")),
        ("dataset", Tree::text("SRC")),
        ("filter", Tree::text("SRC.EOT_FALLBACK = 'x'")),
        (
            "derivations",
            Tree::map(vec![(
                "EOT_FALLBACK",
                row_value(operation(
                    "compute",
                    vec![("expr", Tree::text("SEQ + VAL / 1000"))],
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
    let first = Tree::map(vec![
        ("id", Tree::text("FIRST_AE")),
        ("dataset", Tree::text("SRC")),
        ("key", Tree::map(vec![("SEQ", Tree::text("CURRENT"))])),
    ]);
    let row = Tree::map(vec![
        ("id", Tree::text("candidate")),
        (
            "derivations",
            Tree::map(vec![
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
