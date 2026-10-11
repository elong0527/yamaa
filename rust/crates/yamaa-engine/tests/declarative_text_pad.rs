use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    text_case::{Case, StringCase},
    text_pad::StringPad,
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits, Resource};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    fail: RefCell<Option<Failure>>,
    reads: RefCell<Vec<(usize, usize)>>,
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
fn schema() -> TableSchema {
    TableSchema::new(
        ["ID", "TEXT"]
            .into_iter()
            .map(|name| Column {
                name: name.into(),
                kind: T::Str,
            })
            .collect(),
    )
    .unwrap()
}
fn make_table(text: Value) -> Table {
    Table {
        schema: schema(),
        rows: vec![vec![Value::Str("row1".into()), text]],
        fail: RefCell::default(),
        reads: RefCell::default(),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 1000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}

fn assignment(width: &str) -> Assignment {
    let path = "columns.TEXT.derivation.str_pad";
    Assignment {
        column: 1,
        path: path.into(),
        expression: Expression::StrPad {
            source: Box::new(Assignment {
                column: 1,
                path: format!("{path}.source"),
                expression: Expression::Source(1),
            }),
            operation: StringPad::new("SRC.TEXT", path, width).unwrap(),
        },
    }
}
fn plan(width: &str) -> DatasetPlan {
    DatasetPlan::new(
        schema(),
        schema(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                assignment(width),
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
fn one_named_read_unicode_character_width_no_truncation_and_no_handlers() {
    for (text, width, expected) in [
        ("\u{e9}\u{1f980}", "4", "  \u{e9}\u{1f980}"),
        ("e\u{301}", "4", "  e\u{301}"),
        ("a\0b", "5", "  a\0b"),
        ("longer", "1", "longer"),
        ("", "3", "   "),
    ] {
        let table = make_table(Value::Str(text.into()));
        let attempt = plan(width).execute_observed(&table, limits());
        assert_eq!(
            attempt.result.unwrap().dataset.rows(),
            [vec![Value::Str("row1".into()), Value::Str(expected.into())]]
        );
        assert!(attempt.handler_counts.is_empty());
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    }
}
#[test]
fn missing_skips_unrepresentable_positive_width_but_present_refuses_whole_output() {
    let width = "9".repeat(81);
    let table = make_table(Value::Missing);
    let attempt = plan(&width).execute_observed(&table, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [vec![Value::Str("row1".into()), Value::Missing]]
    );
    assert!(attempt.handler_counts.is_empty());
    let table = make_table(Value::Str("".into()));
    let attempt = plan(&width).execute_observed(&table, limits());
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            required: None,
            ..
        }
    ));
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn original_nonclone_error_is_consumed_once_and_preserves_allocation_and_rc_identity() {
    let table = make_table(Value::Missing);
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *table.fail.borrow_mut() = Some(Failure(payload, marker.clone()));
    let attempt = plan("4").execute_observed(&table, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original error")
    };
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&retained, &marker));
    assert_eq!(path, "columns.TEXT.derivation.str_pad.source");
    assert_eq!(source_row, 0);
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn recursive_padding_and_case_count_only_the_reached_authored_missing_handler() {
    let mut templates = plan("8").templates().to_vec();
    let Expression::StrPad { source, .. } = &mut templates[0].assignments[1].expression else {
        panic!("pad")
    };
    source.expression = Expression::StrCase {
        source: Box::new(Assignment {
            column: 1,
            path: "columns.TEXT.inner.str_case.source".into(),
            expression: Expression::Source(1),
        }),
        operation: StringCase::new(
            "SRC.TEXT".into(),
            "columns.TEXT.inner.str_case".into(),
            Case::Upper,
            Some(Value::Str("MiXeD".into())),
        )
        .unwrap(),
    };
    let declaration =
        DatasetPlan::new(schema(), schema(), templates, vec![], vec![0], vec![]).unwrap();
    let table = make_table(Value::Missing);
    let attempt = declaration.execute_observed(&table, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [vec![
            Value::Str("row1".into()),
            Value::Str("   MiXeD".into())
        ]]
    );
    assert_eq!(attempt.handler_counts.len(), 1);
    assert_eq!(
        attempt.handler_counts[0].spec_path,
        "columns.TEXT.inner.str_case.missing"
    );
    assert_eq!(attempt.handler_counts[0].count, 1);
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn work_and_cumulative_text_refuse_before_later_reads_and_retry_uses_fresh_budget() {
    let table = make_table(Value::Str("\u{e9}\u{1f980}".into()));
    let mut quota = limits();
    quota.work_cells = 0;
    assert!(matches!(
        *plan("4").execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(table.reads.borrow().is_empty());
    let table = Table {
        schema: schema(),
        rows: vec![
            vec![Value::Str("row1".into()), Value::Str("a".into())],
            vec![Value::Str("row2".into()), Value::Str("b".into())],
        ],
        fail: RefCell::default(),
        reads: RefCell::default(),
    };
    let mut quota = limits();
    quota.scalar_text_bytes = 12;
    assert!(matches!(
        *plan("4").execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (1, 0)]);
    table.reads.borrow_mut().clear();
    let result = plan("4").execute(&table, limits()).unwrap();
    assert_eq!(
        result.dataset.rows(),
        [
            vec![Value::Str("row1".into()), Value::Str("   a".into())],
            vec![Value::Str("row2".into()), Value::Str("   b".into())]
        ]
    );
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (1, 0), (1, 1)]);
}
