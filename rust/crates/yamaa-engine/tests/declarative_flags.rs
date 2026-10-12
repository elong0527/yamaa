//! Lazy reads, exact raw truth, handler ownership and attempt-local refusals.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::Read,
    dataset::{
        Assignment, ConversionHandler, DatasetPlan, Expression, Flag, FlagBinding, FlagRead,
        Intermediate, IntermediateKey, MatchRead, MatchValue, RowMode, RowTemplate,
        SecondarySource, SourceSchemas,
    },
    predicate_compiler,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits, Resource};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    fail: RefCell<Option<Failure>>,
}
impl TableAccess for Table {
    type Error = Failure;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
        self.reads.borrow_mut().push((row, column));
        if column == 1 {
            if let Some(error) = self.fail.borrow_mut().take() {
                return Err(CellError::Access(error));
            }
        }
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn schema(fields: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        fields
            .iter()
            .map(|(n, t)| Column {
                name: (*n).into(),
                kind: *t,
            })
            .collect(),
    )
    .unwrap()
}
fn table() -> Table {
    Table {
        schema: schema(&[("ID", T::Int), ("AGE", T::Int)]),
        rows: vec![
            vec![Value::Int(1), Value::Int(70)],
            vec![Value::Int(2), Value::Int(5)],
            vec![Value::Int(3), Value::Missing],
        ],
        reads: RefCell::default(),
        fail: RefCell::default(),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 1000,
        key_cells: 1000,
        work_cells: 10000,
        scalar_text_bytes: 10000,
        output_text_bytes: 10000,
        identity_cells: 1000,
        identity_text_bytes: 10000,
    }
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation.flag"),
    }
}
fn flag(expr: &str, read: FlagRead, true_value: Value) -> Expression {
    Expression::Flag(
        Flag::new(
            predicate_compiler::compile(
                expr,
                "columns.C1.derivation.flag.condition",
                Default::default(),
            )
            .unwrap(),
            vec![FlagBinding {
                name: "AGE".into(),
                read,
            }],
            true_value,
            Some(Value::Int(2)),
            Some(Value::Int(3)),
        )
        .unwrap(),
    )
}
fn plan(t: &Table, true_value: Value) -> DatasetPlan {
    DatasetPlan::new(
        t.schema.clone(),
        schema(&[("ID", T::Int), ("F", T::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    flag("AGE >= 65", FlagRead::Read(Read::Source(1)), true_value),
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
#[test]
fn three_truths_only_selected_literal_conversion_and_one_outer_handler_owner() {
    let table = table();
    let plan = plan(&table, Value::Str("bad".into()))
        .with_conversion_handlers(vec![ConversionHandler {
            assignment_path: "columns.C1.derivation.flag".into(),
            handler: yamaa_core::conversion::LiteralHandler {
                value: Value::Int(99),
                spec_path: "columns.C1.derivation.unconvertible".into(),
            },
        }])
        .unwrap();
    let attempt = plan.execute_observed(&table, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(99)],
            vec![Value::Int(2), Value::Int(2)],
            vec![Value::Int(3), Value::Int(3)]
        ]
    );
    assert_eq!(attempt.handler_counts.len(), 1);
    assert_eq!(attempt.handler_counts[0].count, 1);
    assert_eq!(
        attempt.handler_counts[0].spec_path,
        "columns.C1.derivation.unconvertible"
    );
    assert_eq!(
        *table.reads.borrow(),
        [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
    );
}
#[test]
fn opaque_read_moves_once_and_never_selects_a_branch_or_later_row() {
    let table = table();
    let marker = Rc::new(());
    let payload = Box::new(7);
    let address = &*payload as *const u8;
    *table.fail.borrow_mut() = Some(Failure(payload, marker.clone()));
    let attempt = plan(&table, Value::Int(1)).execute_observed(&table, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("opaque")
    };
    assert_eq!(path, "columns.C1.derivation.flag.condition");
    assert_eq!(source_row, 0);
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained));
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn repeated_named_operands_share_choice_and_suppress_inherited_no_match_counts() {
    let table = table();
    let donor = Table {
        schema: schema(&[("K", T::Int), ("V", T::Int)]),
        rows: vec![
            vec![Value::Int(1), Value::Int(70)],
            vec![Value::Int(2), Value::Int(5)],
        ],
        reads: RefCell::default(),
        fail: RefCell::default(),
    };
    let flag = flag(
        "AGE > 65 AND AGE < 100",
        FlagRead::Read(Read::Intermediate {
            index: 0,
            column: 1,
        }),
        Value::Int(1),
    );
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: table.schema.clone(),
            secondary: vec![SecondarySource {
                name: "TAB".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![Intermediate {
            identifier: "MATCHED".into(),
            path: "intermediates[0]".into(),
            source: yamaa_core::dataset::IntermediateSource::Input(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key.K".into(),
                value: MatchValue::Variable(MatchRead::Output {
                    column: 0,
                    identifier: "ID".into(),
                }),
            }],
            filter: None,
            selection: None,
            no_match: Some(Value::Missing),

            range: None,
            columns: None,
        }],
        schema(&[("ID", T::Int), ("F", T::Int)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(1, flag)],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed_sources(&table, &[&donor], limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(1)],
            vec![Value::Int(2), Value::Int(2)],
            vec![Value::Int(3), Value::Int(3)]
        ]
    );
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(
        donor.reads.borrow().iter().filter(|(_, c)| *c == 1).count(),
        4
    );
    assert_eq!(
        donor.reads.borrow().iter().filter(|(_, c)| *c == 0).count(),
        6
    );
}
#[test]
fn cumulative_predicate_budget_refuses_before_later_effect_and_retry_is_fresh() {
    let table = table();
    let mut quota = limits();
    quota.work_cells = 0;
    let result = plan(&table, Value::Int(1)).execute(&table, quota);
    assert!(matches!(
        *result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(table.reads.borrow().is_empty());
    let mut quota = limits();
    quota.work_cells = 5;
    let result = plan(&table, Value::Int(1)).execute(&table, quota);
    assert!(result.is_err());
    let prefix = table.reads.borrow().clone();
    assert_eq!(prefix, [(0, 0), (0, 1)]);
    table.reads.borrow_mut().clear();
    assert!(plan(&table, Value::Int(1))
        .execute(&table, limits())
        .is_ok());
    assert_eq!(table.reads.borrow().len(), 6);
}

#[test]
fn implicit_collections_refuse_ambiguity_with_complete_authored_key_identity() {
    let mut table = table();
    table.rows = vec![
        vec![Value::Int(1), Value::Int(70)],
        vec![Value::Int(1), Value::Int(5)],
    ];
    let plan = DatasetPlan::new(
        table.schema.clone(),
        schema(&[("ID", T::Int), ("F", T::Int)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(
            1,
            flag(
                "AGE >= 65",
                FlagRead::Collect {
                    column: 1,
                    identifier: "SRC.AGE".into(),
                },
                Value::Int(1),
            ),
        )],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&table, limits());
    let ExecutionError::MultipleValues {
        path,
        identifier,
        value_count,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("ambiguity")
    };
    assert_eq!(path, "columns.C1.derivation.flag.condition");
    assert_eq!(identifier, "SRC.AGE");
    assert_eq!(value_count, 2);
    let identity = identity.unwrap();
    assert_eq!(identity.position, 0);
    assert_eq!(identity.values, [Value::Int(1)]);
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(
        table
            .reads
            .borrow()
            .iter()
            .filter(|(_, c)| *c == 1)
            .copied()
            .collect::<Vec<_>>(),
        [(0, 1), (1, 1)]
    );
}
#[test]
fn selected_text_refusal_precedes_conversion_recovery_and_counters_accumulate() {
    let table = table();
    let plan = plan(&table, Value::Str("bad".into()))
        .with_conversion_handlers(vec![ConversionHandler {
            assignment_path: "columns.C1.derivation.flag".into(),
            handler: yamaa_core::conversion::LiteralHandler {
                value: Value::Int(99),
                spec_path: "columns.C1.derivation.unconvertible".into(),
            },
        }])
        .unwrap();
    let mut quota = limits();
    quota.scalar_text_bytes = 2;
    let attempt = plan.execute_observed(&table, quota);
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            required: Some(3),
            ..
        }
    ));
    assert_eq!(attempt.handler_counts.len(), 1);
    assert_eq!(attempt.handler_counts[0].count, 0);
    assert_eq!(
        attempt.handler_counts[0].spec_path,
        "columns.C1.derivation.unconvertible"
    );
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    let constant = Expression::Flag(
        Flag::new(
            predicate_compiler::compile("TRUE", "flag.condition", Default::default()).unwrap(),
            vec![],
            Value::Str("Y".into()),
            None,
            None,
        )
        .unwrap(),
    );
    let plan = DatasetPlan::new(
        table.schema.clone(),
        schema(&[("ID", T::Int), ("F", T::Str)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0, Expression::Source(0)), assign(1, constant)],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    table.reads.borrow_mut().clear();
    let attempt = plan.execute_observed(&table, quota);
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            required: Some(3),
            ..
        }
    ));
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(*table.reads.borrow(), [(0, 0), (1, 0), (2, 0)]);
    assert!(plan.execute(&table, limits()).is_ok());
}
fn named_plan(table: &Table, donor: &Table, filtered: bool) -> DatasetPlan {
    use yamaa_core::bound_expression::{Binding, BoundPredicate};
    let filter = filtered.then(|| {
        BoundPredicate::new(
            predicate_compiler::compile("TAB.V > 0", "intermediates[0].filter", Default::default())
                .unwrap(),
            vec![Binding {
                name: "TAB.V".into(),
                read: Read::Source(1),
            }],
        )
        .unwrap()
    });
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: table.schema.clone(),
            secondary: vec![SecondarySource {
                name: "TAB".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![Intermediate {
            identifier: "MATCHED".into(),
            path: "intermediates[0]".into(),
            source: yamaa_core::dataset::IntermediateSource::Input(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key.K".into(),
                value: MatchValue::Variable(MatchRead::Output {
                    column: 0,
                    identifier: "ID".into(),
                }),
            }],
            filter: filter.map(Into::into),
            selection: None,
            no_match: Some(Value::Missing),

            range: None,
            columns: None,
        }],
        schema(&[("ID", T::Int), ("F", T::Int)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assign(
            1,
            flag(
                "AGE > 65 AND AGE < 100",
                FlagRead::Read(Read::Intermediate {
                    index: 0,
                    column: 1,
                }),
                Value::Int(1),
            ),
        )],
        vec![0],
        vec![],
    )
    .unwrap()
}
#[test]
fn lazy_filter_and_donor_opaque_failures_keep_original_payload_and_effect_boundary() {
    for filtered in [false, true] {
        let table = table();
        let marker = Rc::new(());
        let payload = Box::new(9);
        let address = &*payload as *const u8;
        let donor = Table {
            schema: schema(&[("K", T::Int), ("V", T::Int)]),
            rows: vec![vec![Value::Int(1), Value::Int(70)]],
            reads: RefCell::default(),
            fail: RefCell::new(Some(Failure(payload, marker.clone()))),
        };
        let attempt = named_plan(&table, &donor, filtered).execute_observed_sources(
            &table,
            &[&donor],
            limits(),
        );
        let failure = if filtered {
            let ExecutionError::Predicate { source_row, error } = *attempt.result.unwrap_err()
            else {
                panic!("filter")
            };
            assert_eq!(source_row, 0);
            assert_eq!(error.spec_path, "intermediates[0].filter");
            let yamaa_core::predicate::ErrorKind::Resolution {
                identifier,
                error: CellError::Access(failure),
            } = error.kind
            else {
                panic!("opaque")
            };
            assert_eq!(identifier, "TAB.V");
            assert_eq!(*donor.reads.borrow(), [(0, 1)]);
            failure
        } else {
            let ExecutionError::SecondaryCell {
                path,
                source,
                source_row,
                error: CellError::Access(failure),
            } = *attempt.result.unwrap_err()
            else {
                panic!("donor")
            };
            assert_eq!(path, "intermediates[0]");
            assert_eq!(source, 0);
            assert_eq!(source_row, 0);
            assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1)]);
            failure
        };
        assert_eq!(&*failure.0 as *const u8, address);
        assert!(Rc::ptr_eq(&failure.1, &marker));
        assert!(attempt.handler_counts.is_empty());
    }
}
