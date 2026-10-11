use std::{cell::RefCell, convert::Infallible};
use yamaa_core::{
    diagnostic::{ContextValue as V, ProjectionError},
    reduction::NumericReducer,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::dataset::{
    Assignment, DatasetExecution, DatasetPlan, ExecutionError, Expression, Limits, RowIdentity,
    RowMode, RowTemplate,
};

#[test]
fn borrowed_failure_projection_retains_every_identity_key_partition_and_host_payload() {
    // Projection compiles for a payload with neither Debug nor Clone.
    struct Payload(Box<u8>);
    let identity = || {
        Some(RowIdentity {
            position: 7,
            values: vec![Value::Int(i64::MAX), Value::Missing],
        })
    };
    let observed = || {
        vec![
            ("flag".into(), Value::Bool(false)),
            ("missing".into(), Value::Missing),
        ]
    };
    let date = Value::Date(Date::new(2024, 2, 29, DatePrecision::Month).unwrap());
    let faults: Vec<ExecutionError<Payload>> = vec![
        ExecutionError::MultipleValues {
            path: "columns.A.source".into(),
            identifier: "SRC.A".into(),
            value_count: 3,
            identity: identity(),
        },
        ExecutionError::UnmatchedKey {
            path: "columns.A.source".into(),
            dataset: "DM".into(),
            intermediate: "I".into(),
            matched_key: observed(),
            identity: identity(),
        },
        ExecutionError::MultipleMatches {
            path: "columns.A.source".into(),
            dataset: "DM".into(),
            intermediate: "I".into(),
            match_count: 2,
            matched_key: observed(),
            identity: identity(),
        },
        ExecutionError::BaselineAmbiguity {
            path: "columns.A.baseline".into(),
            column: "A".into(),
            date,
            match_count: 2,
            partition: observed(),
        },
        ExecutionError::ReductionType {
            path: "columns.A.aggregate".into(),
            expression: "SUM(SRC.A)".into(),
            reducer: NumericReducer::Sum,
            source: "SRC.A".into(),
            actual: ValueType::Str,
        },
    ];
    for (error, requirement) in faults
        .iter()
        .zip(["REQ-0075", "REQ-0124", "REQ-0127", "REQ-0322", "REQ-0510"])
    {
        let finding = error.diagnostic().unwrap();
        assert_eq!(finding.definition().requirement, Some(requirement));
        assert_eq!(error.diagnostic(), Some(finding));
        assert_eq!(error.diagnostic_with_limit(32), Err(ProjectionError::Limit));
        match error {
            ExecutionError::MultipleValues {
                identity: Some(value),
                ..
            }
            | ExecutionError::UnmatchedKey {
                identity: Some(value),
                ..
            }
            | ExecutionError::MultipleMatches {
                identity: Some(value),
                ..
            } => {
                assert_eq!(value.position, 7);
                assert_eq!(value.values, [Value::Int(i64::MAX), Value::Missing]);
            }
            ExecutionError::BaselineAmbiguity { partition, .. } => {
                assert_eq!(*partition, observed())
            }
            ExecutionError::ReductionType { source, .. } => assert_eq!(source, "SRC.A"),
            _ => panic!("retained cause changed"),
        }
        if let ExecutionError::MultipleMatches { matched_key, .. }
        | ExecutionError::UnmatchedKey { matched_key, .. } = error
        {
            assert_eq!(*matched_key, observed());
        }
    }
    let payload = Payload(Box::new(9));
    let address = &*payload.0 as *const u8;
    let error = ExecutionError::Cell {
        path: "source".into(),
        source_row: 9,
        error: CellError::Access(payload),
    };
    assert!(error.diagnostic().is_none());
    assert_eq!(
        error.diagnostic_with_limit(4096),
        Err(ProjectionError::InvalidContext)
    );
    let ExecutionError::Cell {
        error: CellError::Access(Payload(value)),
        ..
    } = error
    else {
        panic!("original host error changed")
    };
    assert_eq!(&*value as *const u8, address);
}

struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    fail: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access("later access failure"));
        }
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
fn schema(columns: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
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
        identity_cells: 10000,
        identity_text_bytes: 10000,
    }
}
#[test]
fn projection_never_replays_reads_and_later_source_failure_keeps_its_precedence() {
    let mut source = Table {
        schema: schema(&[("ID", ColumnType::Int), ("A", ColumnType::Str)]),
        rows: vec![
            vec![Value::Int(1), Value::Str("07".into())],
            vec![Value::Int(1), Value::Str("7".into())],
            vec![Value::Int(1), Value::Str("8".into())],
        ],
        reads: RefCell::default(),
        fail: None,
    };
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("A", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![Assignment {
                column: 0,
                path: "columns.ID.source".into(),
                expression: Expression::Source(0),
            }],
            filter: None,
        }],
        vec![Assignment {
            column: 1,
            path: "columns.A.source".into(),
            expression: Expression::Collect {
                column: 1,
                identifier: "SRC.A".into(),
                filter: None,
                selection: None,
            },
        }],
        vec![0],
        vec![],
    )
    .unwrap();
    let error = plan.execute(&source, limits()).unwrap_err();
    let reads = source.reads.borrow().clone();
    let finding = error.diagnostic().unwrap();
    assert_eq!(finding.definition().requirement, Some("REQ-0075"));
    assert_eq!(finding.context["value_count"], V::Scalar(Value::Int(3)));
    assert_eq!(error.diagnostic_with_limit(32), Err(ProjectionError::Limit));
    assert_eq!(*source.reads.borrow(), reads);
    assert!(
        matches!(&*error,ExecutionError::MultipleValues {identity:Some(value),..} if value.values == [Value::Int(1)])
    );
    source.fail = Some((2, 1));
    let error = plan.execute(&source, limits()).unwrap_err();
    assert!(error.diagnostic().is_none());
    assert!(matches!(
        &*error,
        ExecutionError::Cell {
            source_row: 2,
            error: CellError::Access("later access failure"),
            ..
        }
    ));
    // A type-level use also confirms projection imposes no table/error bound.
    let _: Option<yamaa_core::diagnostic::Diagnostic> =
        ExecutionError::<Infallible>::SchemaMismatch.diagnostic();
}
