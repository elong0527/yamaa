//! Range narrowing preserves borrowed endpoint reads, native failures and observations.
use std::cell::RefCell;
use yamaa_core::{
    dataset::{
        Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
        IntermediateKey, IntermediateSource, MatchRead, MatchValue, RelationRange,
        RelationRangeBound, RowMode, RowTemplate, SecondarySource, SourceSchemas,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value, ValueType},
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits},
    numeric_lifecycle::HandlerKind,
};
#[derive(Debug)]
struct Opaque(&'static str);
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    failure: Option<(usize, usize)>,
    held: RefCell<Option<Box<Opaque>>>,
}
impl TableAccess for Table {
    type Error = Box<Opaque>;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.failure == Some((row, column)) {
            return Err(CellError::Access(
                self.held
                    .borrow_mut()
                    .take()
                    .expect("one attempted opaque read"),
            ));
        }
        Ok((&self.rows[row][column]).into())
    }
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
fn table(schema: TableSchema, rows: Vec<Vec<Value>>, failure: Option<(usize, usize)>) -> Table {
    Table {
        schema,
        rows,
        reads: RefCell::default(),
        failure,
        held: RefCell::new(failure.map(|_| Box::new(Opaque("original table failure")))),
    }
}
fn fixture(
    current: Value,
    lower: Value,
    failure: Option<(usize, usize)>,
) -> (Table, Table, DatasetPlan) {
    let primary = table(
        schema(&[("ID", T::Int), ("X", T::Int)]),
        vec![vec![Value::Int(1), current]],
        None,
    );
    let donor = table(
        schema(&[
            ("ID", T::Int),
            ("LOW", T::Int),
            ("HIGH", T::Int),
            ("V", T::Str),
        ]),
        vec![
            vec![
                Value::Int(1),
                lower,
                Value::Int(9),
                Value::Str("selected".into()),
            ],
            vec![
                Value::Int(2),
                Value::Int(0),
                Value::Int(9),
                Value::Str("unmatched".into()),
            ],
        ],
        failure,
    );
    let item = Intermediate {
        identifier: "P".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Input(0),
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
        columns: Some(vec![3]),
        no_match: Some(Value::Missing),
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,
        range: Some(
            RelationRange::new(
                "intermediates[0].between".into(),
                CurrentField {
                    identifier: "SRC.X".into(),
                    read: CurrentRead::Record {
                        source: 0,
                        column: 1,
                    },
                },
                Some(ValueType::Int),
                Some(RelationRangeBound {
                    column: 1,
                    identifier: "LOW".into(),
                    kind: Some(ValueType::Int),
                }),
                Some(RelationRangeBound {
                    column: 2,
                    identifier: "HIGH".into(),
                    kind: Some(ValueType::Int),
                }),
                4096,
            )
            .unwrap(),
        ),
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: primary.schema.clone(),
            secondary: vec![SecondarySource {
                name: "D".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![item],
        schema(&[("ID", T::Int), ("A", T::Str)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.source".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: "columns.A.source".into(),
                    expression: Expression::Intermediate {
                        index: 0,
                        column: 3,
                    },
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    (primary, donor, plan)
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 100_000,
        scalar_text_bytes: 100_000,
        output_text_bytes: 100_000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
#[test]
fn missing_current_skips_all_donor_reads_and_records_the_absence_site() {
    let (primary, donor, plan) = fixture(Value::Missing, Value::Int(0), Some((0, 1)));
    let result = plan.execute_observed_sources(&primary, &[&donor], limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [vec![Value::Int(1), Value::Missing]]
    );
    assert_eq!(*primary.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(donor.reads.borrow().is_empty());
    assert!(donor.held.borrow().is_some());
    assert_eq!(
        result
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler, h.count))
            .collect::<Vec<_>>(),
        [("columns.A.source.no_match", HandlerKind::NoMatch, 1)]
    );
}
#[test]
fn missing_lower_skips_upper_and_unmatched_records_never_read_endpoints() {
    let (primary, donor, plan) = fixture(Value::Int(3), Value::Missing, Some((0, 2)));
    let result = plan.execute_observed_sources(&primary, &[&donor], limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [vec![Value::Int(1), Value::Missing]]
    );
    assert_eq!(*primary.reads.borrow(), [(0, 0), (0, 1)]);
    assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1), (1, 0)]);
    assert!(donor.held.borrow().is_some());
    assert_eq!(
        result
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler, h.count))
            .collect::<Vec<_>>(),
        [("columns.A.source.no_match", HandlerKind::NoMatch, 1)]
    );
}
#[test]
fn upper_access_precedes_lower_exclusion_and_preserves_the_original_error_owner() {
    let (primary, donor, plan) = fixture(Value::Int(3), Value::Int(4), Some((0, 2)));
    let address = donor.held.borrow().as_ref().unwrap().as_ref() as *const Opaque;
    let result = plan.execute_observed_sources(&primary, &[&donor], limits());
    let ExecutionError::SecondaryCell {
        path,
        source,
        source_row,
        error: CellError::Access(original),
    } = *result.result.unwrap_err()
    else {
        panic!("original secondary access owner expected")
    };
    assert_eq!(
        (path.as_str(), source, source_row, original.0),
        ("intermediates[0]", 0, 0, "original table failure")
    );
    assert_eq!(original.as_ref() as *const Opaque, address);
    assert_eq!(*primary.reads.borrow(), [(0, 0), (0, 1)]);
    assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1), (0, 2)]);
    assert!(result.handler_counts.is_empty());
}
#[test]
fn current_access_failure_precedes_matching_and_retains_native_owner() {
    let (mut primary, donor, plan) = fixture(Value::Int(3), Value::Int(0), None);
    primary.failure = Some((0, 1));
    *primary.held.borrow_mut() = Some(Box::new(Opaque("current value failure")));
    let address = primary.held.borrow().as_ref().unwrap().as_ref() as *const Opaque;
    let result = plan.execute_observed_sources(&primary, &[&donor], limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(original),
    } = *result.result.unwrap_err()
    else {
        panic!("original primary access owner expected")
    };
    assert_eq!(
        (path.as_str(), source_row, original.0),
        ("intermediates[0].between", 0, "current value failure")
    );
    assert_eq!(original.as_ref() as *const Opaque, address);
    assert_eq!(*primary.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(donor.reads.borrow().is_empty());
    assert!(result.handler_counts.is_empty());
}
