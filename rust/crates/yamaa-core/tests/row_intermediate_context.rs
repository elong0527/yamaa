//! Row admission uses original named selectors and actual completed match values.
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, MatchRead, MatchValue,
        PlanError, RowMode, RowTemplate, SecondarySource, SourceSchemas,
    },
    schema::{
        Document, DocumentLimits, DocumentNode as N, SpecificationDocument, ValidationBudget,
    },
    specification::{BindError, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::{ColumnType as T, Value},
};
enum Tree<'a> {
    Text(&'a str),
    Scalar(N),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Text(v) => N::Text(v.into()),
            Self::Scalar(v) => v,
            Self::List(items) => N::Sequence(items.into_iter().map(|v| v.append(nodes)).collect()),
            Self::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (Self::Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn doc(
    reordered: bool,
    drivers: [Option<&str>; 2],
    grouped: bool,
    field: &str,
) -> SpecificationDocument {
    use Tree::*;
    let source = |name| {
        Map(vec![(
            "value",
            Map(vec![("source", Map(vec![("variable", Text(name))]))]),
        )])
    };
    let mut inputs = vec![
        ("SRC", Map(vec![("path", Text("src.csv"))])),
        ("DONOR", Map(vec![("path", Text("donor.csv"))])),
    ];
    if reordered {
        inputs.reverse();
    }
    let mut rows = Vec::new();
    for (id, driver) in ["first", "second"].iter().zip(drivers) {
        let mut fields = vec![
            ("id", Text(id)),
            ("derivations", Map(vec![("ROWV", source(field))])),
        ];
        if let Some(driver) = driver {
            fields.push(("dataset", Text(driver)));
        }
        if grouped {
            fields.push(("group_by", List(vec![Text("SRC.ID")])));
        }
        rows.push(Map(fields));
    }
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("input", Map(inputs)),
        ("keys", List(vec![Text("ID")])),
        (
            "intermediates",
            List(vec![Map(vec![
                ("id", Text("P")),
                ("dataset", Text("DONOR")),
                ("key", Map(vec![("K", Text("KEY"))])),
                ("no_match", Scalar(N::Null)),
            ])]),
        ),
        (
            "columns",
            List(vec![
                Map(vec![
                    ("name", Text("ID")),
                    ("type", Text("str")),
                    ("derivation", source("SRC.ID")),
                ]),
                Map(vec![
                    ("name", Text("KEY")),
                    ("type", Text("str")),
                    ("derivation", source("SRC.K")),
                ]),
                Map(vec![("name", Text("ROWV")), ("type", Text("str"))]),
                Map(vec![
                    ("name", Text("COLV")),
                    ("type", Text("str")),
                    ("derivation", source("P.V")),
                ]),
            ]),
        ),
        ("rows", List(rows)),
        (
            "output",
            Map(vec![
                ("path", Text("out.csv")),
                (
                    "columns",
                    List(vec![Text("ID"), Text("KEY"), Text("ROWV"), Text("COLV")]),
                ),
            ]),
        ),
    ]);
    let mut nodes = Vec::new();
    let root = tree.append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn schema(fields: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        fields
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
#[test]
fn common_authored_input_needs_no_base_and_can_follow_secondary_input_in_written_order() {
    let src = schema(&[("ID", T::Str), ("K", T::Str)]);
    let donor = schema(&[("K", T::Str), ("V", T::Str)]);
    for reordered in [false, true] {
        let prepared = PreparedSpecification::prepare(&doc(
            reordered,
            [Some("SRC"), Some("SRC")],
            false,
            "P.V",
        ))
        .unwrap();
        assert_eq!(prepared.source().name, "SRC");
        let schemas = if reordered {
            vec![&donor, &src]
        } else {
            vec![&src, &donor]
        };
        let plan = prepared.bind_sources(&schemas).unwrap();
        assert_eq!(plan.secondary()[0].name, "DONOR");
        assert_eq!(
            plan.intermediates()[0]
                .output_dependencies()
                .collect::<Vec<_>>(),
            [1]
        );
        for template in plan.templates() {
            assert_eq!(template.mode, RowMode::Records);
            assert_eq!(
                template
                    .assignments
                    .iter()
                    .map(|a| a.column)
                    .collect::<Vec<_>>(),
                [0, 1, 2]
            );
            assert!(matches!(
                template.assignments[2].expression,
                Expression::Intermediate {
                    index: 0,
                    column: 1
                }
            ));
        }
        assert_eq!(plan.columns().len(), 1);
        assert!(matches!(
            plan.columns()[0].expression,
            Expression::Intermediate {
                index: 0,
                column: 1
            }
        ));
    }
}
#[test]
fn unknown_and_omitted_drivers_keep_model_preflight_and_exact_typed_context() {
    for (driver, expected) in [(Some("ABSENT"), Some("ABSENT")), (None, None)] {
        let Err(PrepareError::Invalid(findings)) =
            PreparedSpecification::prepare(&doc(false, [Some("SRC"), driver], false, "P.V"))
        else {
            panic!("preflight")
        };
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", "driver_unavailable", Some("REQ-0034"))
        );
        assert_eq!(diagnostic.spec_paths, ["rows[1].dataset"]);
        assert_eq!(diagnostic.context.len(), 2);
        assert_eq!(
            diagnostic.context["row"],
            yamaa_core::diagnostic::ContextValue::Scalar(Value::Str("second".into()))
        );
        assert_eq!(
            diagnostic.context["dataset"],
            yamaa_core::diagnostic::ContextValue::Scalar(
                expected
                    .map(|v| Value::Str(v.into()))
                    .unwrap_or(Value::Missing)
            )
        );
    }
}
#[test]
fn foreign_driver_defaults_and_grouped_named_reads_keep_explicit_pending_boundaries() {
    let Err(PrepareError::Unsupported(features)) =
        PreparedSpecification::prepare(&doc(false, [Some("SRC"), Some("DONOR")], false, "P.V"))
    else {
        panic!("distinct gate")
    };
    assert_eq!(
        (features[0].operation.as_str(), features[0].path.as_str()),
        ("row_secondary_source", "columns.ID.derivation")
    );
    let Err(PrepareError::Unsupported(features)) =
        PreparedSpecification::prepare(&doc(false, [Some("SRC"), Some("SRC")], true, "P.V"))
    else {
        panic!("grouped gate")
    };
    assert_eq!(
        (features[0].operation.as_str(), features[0].path.as_str()),
        (
            "grouped_intermediate_source",
            "rows[0].derivations.ROWV.source"
        )
    );
}
#[test]
fn unknown_named_field_retains_full_lookup_cause_at_authored_read_boundary() {
    let prepared =
        PreparedSpecification::prepare(&doc(false, [Some("SRC"), Some("SRC")], false, "P.ABSENT"))
            .unwrap();
    let src = schema(&[("ID", T::Str), ("K", T::Str)]);
    let donor = schema(&[("K", T::Str), ("V", T::Str)]);
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &donor]) else {
        panic!("lookup")
    };
    let diagnostics = findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2);
    for (index, d) in diagnostics.iter().enumerate() {
        assert_eq!(
            (
                d.definition().phase,
                d.definition().condition,
                d.definition().requirement
            ),
            ("validation", "unknown_field", Some("REQ-0125"))
        );
        assert_eq!(
            d.spec_paths,
            [format!("rows[{index}].derivations.ROWV.source")]
        );
        assert_eq!(
            d.context,
            std::collections::BTreeMap::from([(
                "identifier".into(),
                yamaa_core::diagnostic::ContextValue::Scalar(Value::Str("P.ABSENT".into()))
            )])
        );
    }
}
#[test]
fn whole_source_metadata_admission_precedes_schema_copy_even_without_source_rows() {
    let prepared =
        PreparedSpecification::prepare(&doc(false, [Some("SRC"), Some("SRC")], false, "P.V"))
            .unwrap();
    let huge = "q".repeat(1_000_000);
    let src = schema(&[("ID", T::Str), ("K", T::Str), (&huge, T::Str)]);
    let donor = schema(&[("K", T::Str), ("V", T::Str)]);
    assert!(matches!(
        prepared.bind_sources(&[&src, &donor]),
        Err(BindError::Catalog(
            yamaa_core::reference_binding::Error::Limit {
                resource: "row_intermediate_metadata",
                ..
            }
        ))
    ));
}
#[test]
fn typed_record_plan_rejects_uncompleted_matching_outputs_and_grouped_reads() {
    let src = schema(&[("ID", T::Str)]);
    let donor = schema(&[("K", T::Str), ("V", T::Str)]);
    let output = schema(&[("ID", T::Str), ("V", T::Str)]);
    for (mode, assignments, expected) in [
        (
            RowMode::Records,
            vec![
                Assignment {
                    column: 1,
                    path: "rows[0].derivations.V.source".into(),
                    expression: Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                },
                Assignment {
                    column: 0,
                    path: "rows[0].derivations.ID.source".into(),
                    expression: Expression::Source(0),
                },
            ],
            PlanError::InvalidLookup,
        ),
        (
            RowMode::Groups(vec![0]),
            vec![
                Assignment {
                    column: 0,
                    path: "rows[0].derivations.ID.source".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: "rows[0].derivations.V.source".into(),
                    expression: Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                },
            ],
            PlanError::InvalidIntermediate,
        ),
    ] {
        let result = DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: src.clone(),
                secondary: vec![SecondarySource {
                    name: "DONOR".into(),
                    schema: donor.clone(),
                }],
            },
            vec![Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: 0,
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 0,
                        identifier: "ID".into(),
                    }),
                }],
                filter: None,
                selection: None,
                no_match: Some(Value::Missing),
            }],
            output.clone(),
            vec![RowTemplate {
                mode,
                assignments,
                filter: None,
            }],
            vec![],
            vec![0],
            vec![],
        );
        assert_eq!(result.unwrap_err(), expected);
    }
}

#[test]
fn secondary_row_drivers_share_output_matching_without_reusing_primary_record_coordinates() {
    let src = schema(&[("ID", T::Str)]);
    let donor = schema(&[("K", T::Str), ("V", T::Str)]);
    let output = schema(&[("ID", T::Str), ("V", T::Str)]);
    let make = |read| {
        DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: src.clone(),
                secondary: vec![SecondarySource {
                    name: "DONOR".into(),
                    schema: donor.clone(),
                }],
            },
            vec![Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: 0,
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(read),
                }],
                filter: None,
                selection: None,
                no_match: Some(Value::Missing),
            }],
            output.clone(),
            vec![RowTemplate {
                mode: RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
                assignments: vec![
                    Assignment {
                        column: 0,
                        path: "rows[0].derivations.ID.source".into(),
                        expression: Expression::Source(0),
                    },
                    Assignment {
                        column: 1,
                        path: "rows[0].derivations.V.source".into(),
                        expression: Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    },
                ],
                filter: None,
            }],
            vec![],
            vec![0],
            vec![],
        )
    };
    assert!(make(MatchRead::Output {
        column: 0,
        identifier: "ID".into()
    })
    .is_ok());
    assert_eq!(
        make(MatchRead::Record {
            column: 0,
            identifier: "SRC.ID".into()
        }),
        Err(PlanError::InvalidIntermediate)
    );
}
