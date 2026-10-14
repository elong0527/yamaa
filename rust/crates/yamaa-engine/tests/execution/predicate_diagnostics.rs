//! Predicate projection inspects one completed native failure without port effects.
use std::cell::RefCell;
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    diagnostic::{ContextValue, PredicateDiagnosticError},
    predicate::{Comparison, ErrorKind, Limits as PredicateLimits, Node, Plan, Scalar},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{
    Assignment, DatasetExecution, DatasetPlan, ExecutionError, Expression, Limits, RowMode,
    RowTemplate,
};

// Neither Clone nor Debug is available to diagnostic code.
struct Opaque(Box<u8>);
struct Source {
    schema: TableSchema,
    error: RefCell<Option<Opaque>>,
    reads: RefCell<Vec<(usize, usize)>>,
}
impl TableAccess for Source {
    type Error = Opaque;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        2
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Opaque>> {
        self.reads.borrow_mut().push((row, column));
        if let Some(error) = self.error.borrow_mut().take() {
            Err(CellError::Access(error))
        } else {
            Ok(ValueRef::Int(7))
        }
    }
}
fn source(error: Option<Opaque>) -> Source {
    Source {
        schema: TableSchema::new(vec![Column {
            name: "A".into(),
            kind: ColumnType::Int,
        }])
        .unwrap(),
        error: RefCell::new(error),
        reads: RefCell::default(),
    }
}
fn plan(schema: &TableSchema, right: Value) -> DatasetPlan {
    let filter = BoundPredicate::new(
        Plan::new(
            vec![Node::Compare {
                operator: Comparison::Equal,
                left: Scalar::Identifier("A".into()),
                right: Scalar::Literal(right),
            }],
            0,
            "rows[0].filter".into(),
            "A = original\0🙂".into(),
            PredicateLimits::default(),
        )
        .unwrap(),
        vec![Binding {
            name: "A".into(),
            read: Read::Source(0),
        }],
    )
    .unwrap();
    DatasetPlan::new(
        schema.clone(),
        schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![],
            filter: Some(filter),
        }],
        vec![Assignment {
            column: 0,
            expression: Expression::Source(0),
            path: "columns.A.derivation".into(),
        }],
        vec![0],
        vec![],
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 10,
        key_cells: 10,
        work_cells: 100,
        scalar_text_bytes: 100,
        output_text_bytes: 100,
        identity_cells: 100,
        identity_text_bytes: 100,
    }
}
#[test]
fn opaque_cell_identity_survives_repeated_projection_without_retry_or_later_reads() {
    let payload = Opaque(Box::new(91));
    let address = &*payload.0 as *const u8;
    let table = source(Some(payload));
    let plan = plan(&table.schema, Value::Int(7));
    assert!(table.reads.borrow().is_empty());
    let Err(failure) = plan.execute(&table, limits()) else {
        panic!("source failure must abort")
    };
    let ExecutionError::Predicate { source_row, error } = &*failure else {
        panic!("predicate source failure")
    };
    assert_eq!(*source_row, 0);
    assert_eq!(error.spec_path, "rows[0].filter");
    assert_eq!(error.expression, "A = original\0🙂");
    for maximum in [0, usize::MAX, 1] {
        let Err(PredicateDiagnosticError::Resolution(CellError::Access(payload))) =
            error.diagnostic_with_limit(maximum)
        else {
            panic!("original resolver failure has precedence over projection quota")
        };
        assert_eq!(&*payload.0 as *const u8, address);
    }
    let ErrorKind::Resolution {
        identifier,
        error: CellError::Access(payload),
    } = &error.kind
    else {
        panic!("original cell error retained")
    };
    assert_eq!(identifier, "A");
    assert_eq!(&*payload.0 as *const u8, address);
    assert_eq!(*table.reads.borrow(), [(0, 0)]);
}
#[test]
fn semantic_filter_failure_precedes_assignments_and_the_next_source_record() {
    let table = source(None);
    let plan = plan(&table.schema, Value::Str("incompatible".into()));
    let Err(failure) = plan.execute(&table, limits()) else {
        panic!("incompatible comparison must abort")
    };
    let ExecutionError::Predicate { source_row, error } = &*failure else {
        panic!("predicate semantic failure")
    };
    assert_eq!(*source_row, 0);
    let diagnostic = error
        .diagnostic()
        .unwrap_or_else(|_| panic!("semantic finding"));
    assert_eq!(diagnostic.definition().phase, "validation");
    assert_eq!(diagnostic.definition().condition, "incompatible_input_type");
    assert_eq!(diagnostic.definition().requirement, Some("REQ-0190"));
    assert_eq!(diagnostic.spec_paths, ["rows[0].filter"]);
    assert_eq!(
        diagnostic.context,
        [
            (
                "left_type".into(),
                ContextValue::Scalar(Value::Str("int".into()))
            ),
            (
                "right_type".into(),
                ContextValue::Scalar(Value::Str("str".into()))
            ),
        ]
        .into()
    );
    assert_eq!(diagnostic.source_span, None);
    assert_eq!(diagnostic.operand_route, Some(vec![]));
    assert!(matches!(
        error.diagnostic_with_limit(1),
        Err(PredicateDiagnosticError::OutputLimit)
    ));
    assert_eq!(
        error
            .diagnostic()
            .unwrap_or_else(|_| panic!("retained finding")),
        diagnostic
    );
    assert_eq!(*table.reads.borrow(), [(0, 0)]);
}
