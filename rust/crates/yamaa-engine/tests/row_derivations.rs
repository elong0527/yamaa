use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, CaseBranch, DatasetPlan, Expression, FirstAvailable, OrderTerm, RowMode,
        RowTemplate, SecondarySource, SelectionRead, SelectionSource, Window, WindowKind,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits};
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    failure: Option<(usize, usize, Rc<str>)>,
}
impl TableAccess for Table {
    type Error = Rc<str>;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if let Some((r, c, error)) = &self.failure {
            if (*r, *c) == (row, column) {
                return Err(CellError::Access(error.clone()));
            }
        }
        Ok((&self.rows[row][column]).into())
    }
}
fn schema(names: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
fn assign(column: usize, expression: Expression, path: &str) -> Assignment {
    Assignment {
        column,
        expression,
        path: path.into(),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 10_000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
#[test]
fn grouped_filter_removes_candidates_before_numbering_windows() {
    let source = schema(&[("ID", ColumnType::Int)]);
    let table = Table {
        schema: source.clone(),
        rows: [1, 1, 2, 3, 3]
            .into_iter()
            .map(|id| vec![Value::Int(id)])
            .collect(),
        reads: RefCell::default(),
        failure: None,
    };
    let filter = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("N > 1", "row.filter", Default::default()).unwrap(),
        vec![Binding {
            name: "N".into(),
            read: Read::Column(1),
        }],
    )
    .unwrap();
    let plan = DatasetPlan::new(
        source,
        schema(&[
            ("ID", ColumnType::Int),
            ("N", ColumnType::Int),
            ("SEQ", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0), "row.ID"),
                assign(
                    1,
                    Expression::Count {
                        column: None,
                        text: "COUNT(*)".into(),
                    },
                    "row.N",
                ),
                assign(
                    2,
                    Expression::Window(Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 0,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                    "row.SEQ",
                ),
            ],
            filter: Some(filter),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute(&table, limits()).unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(2), Value::Int(1)],
            vec![Value::Int(3), Value::Int(2), Value::Int(2)],
        ]
    );
}

#[test]
fn grouped_filter_uses_one_eligibility_pass_before_windows() {
    struct OnceTable {
        schema: TableSchema,
        reads: RefCell<Vec<(usize, usize)>>,
        payload: Rc<str>,
    }
    impl TableAccess for OnceTable {
        type Error = Rc<str>;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            3
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
            let mut reads = self.reads.borrow_mut();
            let repeated = reads.contains(&(row, column));
            reads.push((row, column));
            match column {
                0 => Ok(ValueRef::Int(row as i64 + 1)),
                1 if repeated => Err(CellError::Access(Rc::clone(&self.payload))),
                1 => Ok(ValueRef::Int(if row == 1 { 0 } else { 1 })),
                _ => panic!("unexpected source column"),
            }
        }
    }
    let source = schema(&[("ID", ColumnType::Int), ("KEEP", ColumnType::Int)]);
    let table = OnceTable {
        schema: source.clone(),
        reads: RefCell::default(),
        payload: Rc::from("filter source was revisited"),
    };
    let filter = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("KEEP > 0", "row.filter", Default::default())
            .unwrap(),
        vec![Binding {
            name: "KEEP".into(),
            read: Read::Column(1),
        }],
    )
    .unwrap();
    let plan = DatasetPlan::new(
        source,
        schema(&[
            ("ID", ColumnType::Int),
            ("KEEP", ColumnType::Int),
            ("SEQ", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0), "row.ID"),
                assign(
                    1,
                    Expression::Reduce {
                        identifier: None,
                        column: 1,
                        reducer: yamaa_core::reduction::NumericReducer::Sum,
                        text: "SUM(SRC.KEEP)".into(),
                    },
                    "row.KEEP",
                ),
                assign(
                    2,
                    Expression::Window(Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 0,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                    "row.SEQ",
                ),
            ],
            filter: Some(filter),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    // This finite attempt budget admits one eligibility evaluation per group.
    // Repeating the predicate after the windows consumes additional work.
    let mut bounded = limits();
    bounded.work_cells = 40;
    let attempt = plan.execute_observed(&table, bounded);
    assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(1), Value::Int(1)],
            vec![Value::Int(3), Value::Int(1), Value::Int(2)]
        ],
    );
    assert_eq!(
        *table.reads.borrow(),
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 0),
            (2, 1)
        ],
    );
}

#[test]
fn windows_number_each_template_after_filter_without_revisiting_sources() {
    let source = schema(&[("ID", ColumnType::Int)]);
    let table = Table {
        schema: source.clone(),
        rows: vec![
            vec![Value::Int(3)],
            vec![Value::Int(1)],
            vec![Value::Int(2)],
        ],
        reads: RefCell::default(),
        failure: None,
    };
    let output = schema(&[
        ("ID", ColumnType::Int),
        ("SECTION", ColumnType::Str),
        ("SEQ", ColumnType::Int),
        ("NEXT", ColumnType::Int),
    ]);
    let window = |kind| {
        Expression::Window(Window {
            kind,
            group_by: vec![],
            order_by: vec![OrderTerm {
                column: 0,
                descending: false,
                nulls_first: false,
            }],
            filter: None,
        })
    };
    let filter = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("ID > 1", "row.filter", Default::default())
            .unwrap(),
        vec![Binding {
            name: "ID".into(),
            read: Read::Column(0),
        }],
    )
    .unwrap();
    let template = |section: &str, filter| RowTemplate {
        mode: RowMode::Records,
        assignments: vec![
            assign(0, Expression::Source(0), "row.ID"),
            assign(
                1,
                Expression::Literal(Value::Str(section.into())),
                "row.SECTION",
            ),
            assign(2, window(WindowKind::RowNumber), "row.SEQ"),
            assign(
                3,
                Expression::Window(Window {
                    kind: WindowKind::RowValue {
                        column: 2,
                        offset: 1,
                    },
                    group_by: vec![],
                    order_by: vec![OrderTerm {
                        column: 2,
                        descending: false,
                        nulls_first: false,
                    }],
                    filter: None,
                }),
                "row.NEXT",
            ),
        ],
        filter,
    };
    let plan = DatasetPlan::new(
        source,
        output,
        vec![template("all", None), template("screened", Some(filter))],
        vec![],
        vec![0, 1],
        vec![],
    )
    .unwrap();
    let result = plan.execute(&table, limits()).unwrap();
    assert_eq!(
        result.dataset.rows(),
        [
            vec![
                Value::Int(3),
                Value::Str("all".into()),
                Value::Int(3),
                Value::Missing
            ],
            vec![
                Value::Int(1),
                Value::Str("all".into()),
                Value::Int(1),
                Value::Int(2)
            ],
            vec![
                Value::Int(2),
                Value::Str("all".into()),
                Value::Int(2),
                Value::Int(3)
            ],
            vec![
                Value::Int(3),
                Value::Str("screened".into()),
                Value::Int(2),
                Value::Missing
            ],
            vec![
                Value::Int(2),
                Value::Str("screened".into()),
                Value::Int(1),
                Value::Int(2)
            ]
        ]
    );
    assert_eq!(
        *table.reads.borrow(),
        [(0, 0), (1, 0), (2, 0), (0, 0), (1, 0), (2, 0)]
    );
}
#[test]
fn nested_case_selection_keeps_authored_reads_and_original_failure() {
    let source = schema(&[
        ("ID", ColumnType::Int),
        ("FIRST", ColumnType::Str),
        ("LAST", ColumnType::Str),
    ]);
    let payload: Rc<str> = Rc::from("original source failure");
    let table = Table {
        schema: source.clone(),
        rows: vec![
            vec![
                Value::Int(1),
                Value::Str("first".into()),
                Value::Str("unread".into()),
            ],
            vec![Value::Int(2), Value::Missing, Value::Str("failed".into())],
        ],
        reads: RefCell::default(),
        failure: Some((1, 2, payload.clone())),
    };
    let output = schema(&[("ID", ColumnType::Int), ("RESULT", ColumnType::Str)]);
    let path = "rows[0].derivations.RESULT.case[0].then.first_available";
    let predicate = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("ID > 0", "row.case.when", Default::default())
            .unwrap(),
        vec![Binding {
            name: "ID".into(),
            read: Read::Column(0),
        }],
    )
    .unwrap();
    let selection = Expression::FirstAvailable(Box::new(FirstAvailable::new(
        [1, 2]
            .iter()
            .enumerate()
            .map(|(index, column)| SelectionSource {
                path: format!("{path}.sources[{index}]"),
                read: SelectionRead::Source(*column),
            })
            .collect(),
        Value::Missing,
    )));
    let plan = DatasetPlan::new(
        source,
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0), "row.ID"),
                assign(
                    1,
                    Expression::Case(vec![CaseBranch {
                        when: Some(predicate),
                        assignment: Box::new(assign(1, selection, path)),
                    }]),
                    "row.RESULT",
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&table, limits());
    let error = attempt.result.unwrap_err();
    let ExecutionError::Cell {
        path: actual,
        source_row,
        error: CellError::Access(held),
    } = *error
    else {
        panic!("source error changed")
    };
    assert_eq!(actual, format!("{path}.sources[1]"));
    assert_eq!(source_row, 1);
    assert!(Rc::ptr_eq(&held, &payload));
    assert_eq!(
        *table.reads.borrow(),
        [(0, 0), (0, 1), (1, 0), (1, 1), (1, 2)]
    );
    assert!(attempt.retained_verifications.is_empty());
}

#[test]
fn unselected_nested_function_requires_activation_before_every_table_method() {
    use yamaa_core::{
        dataset::BoundFunction,
        function_signature::{FunctionIdentity, InvocationPlan},
    };
    let source = schema(&[("ID", ColumnType::Int)]);
    let table = Table {
        schema: source.clone(),
        rows: vec![vec![Value::Int(1)]],
        reads: RefCell::default(),
        failure: None,
    };
    let predicate = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("ID < 0", "case.when", Default::default()).unwrap(),
        vec![Binding {
            name: "ID".into(),
            read: Read::Column(0),
        }],
    )
    .unwrap();
    let signature = InvocationPlan::new(
        FunctionIdentity {
            name: "id".into(),
            contract_version: "1".into(),
            implementation_version: "1".into(),
            call: "project.id".into(),
        },
        vec![],
        ColumnType::Int,
        false,
    )
    .unwrap();
    let function = Expression::Function(BoundFunction::new(0, signature, vec![]).unwrap());
    let plan = DatasetPlan::new(
        source.clone(),
        schema(&[("ID", ColumnType::Int), ("RESULT", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0), "row.ID"),
                assign(
                    1,
                    Expression::Case(vec![CaseBranch {
                        when: Some(predicate),
                        assignment: Box::new(assign(
                            1,
                            function,
                            "row.RESULT.case[0].then.function",
                        )),
                    }]),
                    "row.RESULT",
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert!(matches!(
        *plan.execute(&table, limits()).unwrap_err(),
        ExecutionError::FunctionBinding { slot: 0 }
    ));
    assert!(table.reads.borrow().is_empty());
}

#[test]
fn distinct_driver_failure_retains_its_source_coordinate_payload_and_entered_prefix() {
    let primary = Table {
        schema: schema(&[("ID", ColumnType::Int)]),
        rows: vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        reads: RefCell::default(),
        failure: None,
    };
    let payload: Rc<str> = "secondary retained failure".into();
    let secondary = Table {
        schema: schema(&[
            ("PAD", ColumnType::Str),
            ("ID", ColumnType::Int),
            ("VALUE", ColumnType::Float),
        ]),
        rows: vec![
            vec![Value::Str("x".into()), Value::Int(10), Value::float(3.0)],
            vec![Value::Str("y".into()), Value::Int(20), Value::float(4.0)],
        ],
        reads: RefCell::default(),
        failure: Some((1, 2, payload.clone())),
    };
    let assignments = |id, value| {
        vec![
            assign(0, Expression::Source(id), "rows.ID.source"),
            assign(1, value, "rows.VALUE.source"),
        ]
    };
    let plan = DatasetPlan::new_with_sources(
        primary.schema.clone(),
        vec![SecondarySource {
            name: "SECOND".into(),
            schema: secondary.schema.clone(),
        }],
        schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Float)]),
        vec![
            RowTemplate {
                mode: RowMode::Records,
                assignments: assignments(0, Expression::Literal(Value::float(1.0))),
                filter: None,
            },
            RowTemplate {
                mode: RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
                assignments: assignments(1, Expression::Source(2)),
                filter: None,
            },
        ],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed_sources(&primary, &[&secondary], limits());
    let error = attempt.result.unwrap_err();
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(actual),
    } = *error
    else {
        panic!("original cell cause")
    };
    assert_eq!(path, "rows.VALUE.source");
    assert_eq!(source_row, 1);
    assert!(Rc::ptr_eq(&actual, &payload));
    assert_eq!(*primary.reads.borrow(), [(0, 0), (1, 0)]);
    assert_eq!(*secondary.reads.borrow(), [(0, 1), (0, 2), (1, 1), (1, 2)]);
    assert!(attempt.retained_verifications.is_empty());
}

/// Container reads run after the template window without an extra source pass.
#[test]
fn wrapped_first_available_reads_the_completed_template_window() {
    let source = schema(&[("ID", ColumnType::Int)]);
    let table = Table {
        schema: source.clone(),
        rows: vec![
            vec![Value::Int(3)],
            vec![Value::Int(1)],
            vec![Value::Int(2)],
        ],
        reads: RefCell::default(),
        failure: None,
    };
    let plan = DatasetPlan::new(
        source,
        schema(&[
            ("ID", ColumnType::Int),
            ("SEQ", ColumnType::Int),
            ("SELECTED", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0), "row.ID"),
                assign(
                    1,
                    Expression::Window(Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 0,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                    "row.SEQ",
                ),
                assign(
                    2,
                    Expression::FirstAvailable(Box::new(FirstAvailable::new(
                        vec![SelectionSource {
                            path: "row.SELECTED.sources[0]".into(),
                            read: SelectionRead::Expression(Box::new(assign(
                                2,
                                Expression::SourceBinding {
                                    source: Box::new(assign(
                                        2,
                                        Expression::Column(1),
                                        "row.SELECTED.source",
                                    )),
                                    absent: None,
                                    variable_absent: false,
                                    selection_declared: false,
                                    variable: "SEQ".into(),
                                    path: "row.SELECTED.source".into(),
                                },
                                "row.SELECTED.source",
                            ))),
                        }],
                        Value::Int(99),
                    ))),
                    "row.SELECTED",
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let result = plan.execute(&table, limits()).unwrap();
    assert_eq!(
        result.dataset.rows(),
        [
            vec![Value::Int(3), Value::Int(3), Value::Int(3)],
            vec![Value::Int(1), Value::Int(1), Value::Int(1)],
            vec![Value::Int(2), Value::Int(2), Value::Int(2)]
        ]
    );
    assert_eq!(*table.reads.borrow(), [(0, 0), (1, 0), (2, 0)]);
}

/// An unselected source-selection callback is checked before every table method.
#[test]
fn wrapped_selection_cannot_hide_an_unactivated_function() {
    use yamaa_core::{
        dataset::BoundFunction,
        function_signature::{FunctionIdentity, InvocationPlan},
    };
    struct Untouched;
    impl TableAccess for Untouched {
        type Error = Rc<str>;
        fn schema(&self) -> &TableSchema {
            panic!("schema entered before function admission")
        }
        fn row_count(&self) -> usize {
            panic!("cardinality entered before function admission")
        }
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
            panic!("cell entered before function admission")
        }
    }
    let signature = InvocationPlan::new(
        FunctionIdentity {
            name: "id".into(),
            contract_version: "1".into(),
            implementation_version: "1".into(),
            call: "project.id".into(),
        },
        vec![],
        ColumnType::Int,
        false,
    )
    .unwrap();
    let selection = Expression::FirstAvailable(Box::new(FirstAvailable::new(
        vec![
            SelectionSource {
                path: "row.RESULT.sources[0]".into(),
                read: SelectionRead::Expression(Box::new(assign(
                    1,
                    Expression::Literal(Value::Int(7)),
                    "row.RESULT.literal",
                ))),
            },
            SelectionSource {
                path: "row.RESULT.sources[1]".into(),
                read: SelectionRead::Expression(Box::new(assign(
                    1,
                    Expression::SourceBinding {
                        source: Box::new(assign(
                            1,
                            Expression::Case(vec![CaseBranch {
                                when: Some(
                                    BoundPredicate::new(
                                        yamaa_core::predicate_compiler::compile(
                                            "1 = 1",
                                            "row.RESULT.case.when",
                                            Default::default(),
                                        )
                                        .unwrap(),
                                        vec![],
                                    )
                                    .unwrap(),
                                ),
                                assignment: Box::new(assign(
                                    1,
                                    Expression::Function(
                                        BoundFunction::new(0, signature, vec![]).unwrap(),
                                    ),
                                    "row.RESULT.function",
                                )),
                            }]),
                            "row.RESULT.case",
                        )),
                        absent: None,
                        variable_absent: false,
                        selection_declared: false,
                        variable: "FUNCTION".into(),
                        path: "row.RESULT.source".into(),
                    },
                    "row.RESULT.source",
                ))),
            },
        ],
        Value::Missing,
    )));
    let plan = DatasetPlan::new(
        schema(&[("ID", ColumnType::Int)]),
        schema(&[("ID", ColumnType::Int), ("RESULT", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0), "row.ID"),
                assign(1, selection, "row.RESULT"),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert!(matches!(
        *plan.execute(&Untouched, limits()).unwrap_err(),
        ExecutionError::FunctionBinding { slot: 0 }
    ));
}
