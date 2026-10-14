use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate, Temporal, TemporalKind},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits};

// Opaque caller-thread errors need neither cloning nor formatting.
struct Opaque(Rc<()>);
struct Table {
    schema: TableSchema,
    first: Value,
    fail_at: usize,
    failure: RefCell<Option<Box<Opaque>>>,
    reads: RefCell<Vec<usize>>,
}
impl TableAccess for Table {
    type Error = Box<Opaque>;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, _: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push(column);
        if column == self.fail_at {
            return Err(CellError::Access(
                self.failure.borrow_mut().take().expect("single effect"),
            ));
        }
        Ok(match column {
            0 => ValueRef::Int(1),
            1 => ValueRef::from(&self.first),
            2 => ValueRef::Date("2024-01-01".parse().unwrap()),
            _ => panic!("invalid table column"),
        })
    }
}
fn schema(names: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|&(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    )
    .unwrap()
}
fn plan(table: &Table) -> DatasetPlan {
    let path = "columns.DAY.derivation.study_day";
    DatasetPlan::new(
        table.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("DAY", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    expression: Expression::Source(0),
                    path: "columns.ID.derivation".into(),
                },
                Assignment {
                    column: 1,
                    path: "columns.DAY.derivation".into(),
                    expression: Expression::Temporal {
                        operands: vec![
                            Assignment {
                                column: 1,
                                expression: Expression::Source(1),
                                path: format!("{path}.date"),
                            },
                            Assignment {
                                column: 1,
                                expression: Expression::Source(2),
                                path: format!("{path}.reference"),
                            },
                        ],
                        operation: Temporal::new(
                            TemporalKind::StudyDay,
                            vec!["SRC.DATE".into(), "SRC.REF".into()],
                            path.into(),
                        )
                        .unwrap(),
                    },
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 20,
        key_cells: 20,
        work_cells: 1000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
#[test]
fn temporal_operands_stop_on_first_failure_and_missing_does_not_skip_later_authority() {
    for (first, fail_at, expected_reads) in [
        (Value::Missing, 1, vec![1]),
        (Value::Missing, 2, vec![1, 2]),
        (Value::Date("2024-01-02".parse().unwrap()), 2, vec![1, 2]),
    ] {
        let token = Rc::new(());
        let original = Box::new(Opaque(token.clone()));
        let address = &*original as *const Opaque;
        let table = Table {
            schema: schema(&[
                ("ID", ColumnType::Int),
                ("DATE", ColumnType::Date),
                ("REF", ColumnType::Date),
            ]),
            first,
            fail_at,
            failure: RefCell::new(Some(original)),
            reads: RefCell::default(),
        };
        let attempted = plan(&table).execute_observed(&table, limits());
        let ExecutionError::Cell {
            error: CellError::Access(error),
            ..
        } = *attempted.result.err().unwrap()
        else {
            panic!("original cell error required")
        };
        assert_eq!(&*error as *const Opaque, address);
        assert!(Rc::ptr_eq(&error.0, &token));
        assert_eq!(
            table
                .reads
                .borrow()
                .iter()
                .copied()
                .filter(|&c| c != 0)
                .collect::<Vec<_>>(),
            expected_reads
        );
        assert!(table.failure.borrow().is_none());
        assert!(attempted.handler_counts.is_empty());
    }
}
#[test]
fn temporal_type_failure_precedes_later_operand_and_cannot_use_conversion_handler() {
    use yamaa_core::{conversion::LiteralHandler, dataset::ConversionHandler};
    let table = Table {
        schema: schema(&[
            ("ID", ColumnType::Int),
            ("DATE", ColumnType::Date),
            ("REF", ColumnType::Date),
        ]),
        first: Value::Str("2024-01-01".into()),
        fail_at: 2,
        failure: RefCell::new(Some(Box::new(Opaque(Rc::new(()))))),
        reads: RefCell::default(),
    };
    let admitted = plan(&table)
        .with_conversion_handlers(vec![ConversionHandler {
            assignment_path: "columns.DAY.derivation".into(),
            handler: LiteralHandler {
                spec_path: "columns.DAY.derivation.unconvertible".into(),
                value: Value::Int(7),
            },
        }])
        .unwrap();
    let attempted = admitted.execute_observed(&table, limits());
    let ExecutionError::ScalarDiagnostic(finding) = *attempted.result.err().unwrap() else {
        panic!("typed temporal diagnostic required")
    };
    assert_eq!(finding.definition().requirement, Some("REQ-0606"));
    assert_eq!(
        finding.spec_paths,
        ["columns.DAY.derivation.study_day.date"]
    );
    assert_eq!(
        table
            .reads
            .borrow()
            .iter()
            .copied()
            .filter(|&c| c != 0)
            .collect::<Vec<_>>(),
        [1]
    );
    assert!(table.failure.borrow().is_some());
    assert!(attempted
        .handler_counts
        .iter()
        .all(|counter| counter.count == 0));
}
