use std::cell::RefCell;
use yamaa_core::{
    conversion::LiteralHandler,
    dataset::{
        Assignment, ConversionHandler, DatasetPlan, Expression, Rounding, RowMode, RowTemplate,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits};
struct Table {
    schema: TableSchema,
    value: Value,
    fail: bool,
    reads: RefCell<Vec<usize>>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, _: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push(column);
        if column == 0 {
            return Ok(ValueRef::Int(1));
        }
        if self.fail {
            return Err(CellError::Access("opaque source failure"));
        }
        Ok(ValueRef::from(&self.value))
    }
}
fn schema(kind: ColumnType) -> TableSchema {
    TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "V".into(),
            kind,
        },
    ])
    .unwrap()
}
fn table(value: Value, fail: bool) -> Table {
    Table {
        schema: schema(ColumnType::Str),
        value,
        fail,
        reads: RefCell::default(),
    }
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
}
fn plan(source: &Table, target: ColumnType) -> DatasetPlan {
    DatasetPlan::new(
        source.schema.clone(),
        schema(target),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Round {
                        source: Box::new(Assignment {
                            column: 1,
                            expression: Expression::Source(1),
                            path: "columns.C1.derivation.round_half_away_from_zero.source".into(),
                        }),
                        rounding: Rounding::new(
                            "SRC.V",
                            "columns.C1.derivation.round_half_away_from_zero",
                            "1",
                        )
                        .unwrap(),
                    },
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
fn opaque_source_failure_precedes_scalar_type_validation_without_a_retry() {
    let source = table(Value::Str("F".into()), true);
    let result = plan(&source, ColumnType::Float).execute_observed(&source, limits());
    assert!(matches!(
        *result.result.unwrap_err(),
        ExecutionError::Cell {
            error: CellError::Access("opaque source failure"),
            ..
        }
    ));
    assert_eq!(
        source
            .reads
            .borrow()
            .iter()
            .filter(|&&column| column == 1)
            .count(),
        1
    );
    assert!(result.handler_counts.is_empty());
}
#[test]
fn missing_precedes_runtime_type_validation_and_rounding_failure_cannot_use_conversion_handler() {
    let source = table(Value::Missing, false);
    let result = plan(&source, ColumnType::Float).execute_observed(&source, limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [vec![Value::Int(1), Value::Missing]]
    );
    let source = table(Value::Str("F".into()), false);
    let plan = plan(&source, ColumnType::Float)
        .with_conversion_handlers(vec![ConversionHandler {
            assignment_path: "columns.C1.derivation".into(),
            handler: LiteralHandler {
                spec_path: "columns.C1.derivation.unconvertible".into(),
                value: Value::float(9.0),
            },
        }])
        .unwrap();
    let result = plan.execute_observed(&source, limits());
    let ExecutionError::ScalarDiagnostic(diagnostic) = *result.result.unwrap_err() else {
        panic!("expected source-type cause")
    };
    assert_eq!(diagnostic.definition().requirement, Some("REQ-0418"));
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.C1.derivation.round_half_away_from_zero.source"]
    );
    assert_eq!(diagnostic.context.len(), 3);
    assert_eq!(diagnostic.source_span, None);
    assert!(result.handler_counts.iter().all(|count| count.count == 0));
}
#[test]
fn output_conversion_occurs_once_after_rounding_and_retains_its_handler_accounting() {
    let source = table(Value::float(2.45), false);
    let plan = plan(&source, ColumnType::Int);
    assert!(matches!(
        *plan.execute_observed(&source, limits()).result.unwrap_err(),
        ExecutionError::Conversion { .. }
    ));
    let plan = plan
        .with_conversion_handlers(vec![ConversionHandler {
            assignment_path: "columns.C1.derivation".into(),
            handler: LiteralHandler {
                spec_path: "columns.C1.derivation.unconvertible".into(),
                value: Value::Int(7),
            },
        }])
        .unwrap();
    let result = plan.execute_observed(&source, limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [vec![Value::Int(1), Value::Int(7)]]
    );
    assert_eq!(result.handler_counts.len(), 1);
    assert_eq!(result.handler_counts[0].count, 1);
}
