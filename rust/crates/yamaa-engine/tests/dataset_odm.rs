//! Independently specified item scopes and original opaque table effects.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    diagnostic::{ConditionCode, ContextValue},
    odm_source::{FIELDS, HIERARCHY, IDENTIFYING},
    predicate::ErrorKind,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::*;

#[derive(Debug, PartialEq)]
struct Original(Rc<String>);
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    fail: Option<(usize, usize)>,
    payload: Rc<String>,
}
impl TableAccess for Table {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        self.reads.borrow_mut().push((row, column));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access(Original(Rc::clone(&self.payload))));
        }
        self.rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .unwrap()
}
fn record(item: &str, value: &str) -> Vec<Value> {
    ["S", "M", "P", "E", "", "F", "", "G", "", item, value]
        .iter()
        .map(|text| {
            if text.is_empty() {
                Value::Missing
            } else {
                Value::Str((*text).into())
            }
        })
        .collect()
}
fn table(rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: schema(&FIELDS),
        rows,
        reads: Default::default(),
        fail: None,
        payload: Rc::new("original non-Send payload".into()),
    }
}
fn predicate(text: &str, field: usize, path: &str) -> BoundPredicate {
    BoundPredicate::new(
        yamaa_core::predicate_compiler::compile(text, path, Default::default()).unwrap(),
        vec![Binding {
            name: format!("SRC.{}", FIELDS[field]),
            read: Read::Source(field),
        }],
    )
    .unwrap()
}
fn read(scope: OdmScope, filter: Option<BoundPredicate>) -> BoundOdm {
    BoundOdm::new(OdmRead {
        dataset: "SRC".into(),
        item: "SRC.IT.TEST.OID".into(),
        item_oid: "IT.TEST.OID".into(),
        path: "columns.VAL.derivation.odm".into(),
        row: Some("occurrence".into()),
        phase: AggregatePhase::Derivation,
        scope,
        selectors: [None, None, None],
        filter,
    })
    .unwrap()
}
fn plan(table: &Table, bound: BoundOdm) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        table.schema.clone(),
        schema(&["ID", "VAL"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![Assignment {
                column: 0,
                expression: Expression::Literal(Value::Str("one".into())),
                path: "rows[0].derivations.ID".into(),
            }],
            filter: Some(predicate("SRC.ItemOID = 'IT.ANCHOR'", 9, "rows[0].filter")),
        }],
        vec![Assignment {
            column: 1,
            expression: Expression::Odm(Box::new(bound)),
            path: "columns.VAL.derivation".into(),
        }],
        vec![0],
        vec![],
    )
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
fn scope_prefix(rows: usize) -> Vec<(usize, usize)> {
    let mut trace = (0..rows).map(|row| (row, 9)).collect::<Vec<_>>();
    trace.extend(HIERARCHY.map(|column| (0, column)));
    for row in 0..rows {
        trace.extend(HIERARCHY.map(|column| (row, column)));
        trace.push((row, 9));
    }
    trace
}

#[test]
fn record_scope_reads_original_occurrence_then_item_then_predicate_then_value() {
    let mut source = table(vec![
        record("IT.ANCHOR", "anchor"),
        record("IT.TEST.OID", "Y"),
        record("IT.OTHER", "unreachable"),
    ]);
    source.fail = Some((2, 10));
    let plan = plan(
        &source,
        read(
            OdmScope::Records,
            Some(predicate(
                "SRC.Value = 'Y'",
                10,
                "columns.VAL.derivation.odm.filter",
            )),
        ),
    )
    .unwrap();
    let attempt = plan.execute_observed(&source, limits());
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        &[vec![Value::Str("one".into()), Value::Str("Y".into())]]
    );
    let mut trace = scope_prefix(3);
    // The predicate runs as its matching record is encountered; final Value
    // resolution happens only after uniqueness over the complete relation.
    trace.insert(3 + HIERARCHY.len() + 2 * (HIERARCHY.len() + 1), (1, 10));
    trace.push((1, 10));
    assert_eq!(*source.reads.borrow(), trace);
}

#[test]
fn a_matching_predicate_failure_moves_the_original_payload_without_a_value_retry() {
    let mut source = table(vec![
        record("IT.ANCHOR", "anchor"),
        record("IT.TEST.OID", "Y"),
        record("IT.OTHER", "later"),
    ]);
    source.fail = Some((1, 10));
    let attempt = plan(
        &source,
        read(
            OdmScope::Records,
            Some(predicate(
                "SRC.Value = 'Y'",
                10,
                "columns.VAL.derivation.odm.filter",
            )),
        ),
    )
    .unwrap()
    .execute_observed(&source, limits());
    match *attempt.result.unwrap_err() {
        ExecutionError::Predicate {
            source_row: 1,
            error,
        } => {
            assert_eq!(error.spec_path, "columns.VAL.derivation.odm.filter");
            match error.kind {
                ErrorKind::Resolution {
                    identifier,
                    error: CellError::Access(Original(payload)),
                } => {
                    assert_eq!(identifier, "SRC.Value");
                    assert!(Rc::ptr_eq(&payload, &source.payload));
                }
                error => panic!("original cell failure lost: {error:?}"),
            }
        }
        error => panic!("unexpected failure: {error:?}"),
    }
    let mut trace = scope_prefix(2);
    trace.insert(2, (2, 9));
    trace.push((1, 10));
    assert_eq!(*source.reads.borrow(), trace);
    assert!(attempt.handler_counts.is_empty());
}

#[test]
fn repeated_records_do_not_read_value_and_report_complete_identity_scope() {
    let mut source = table(vec![
        record("IT.ANCHOR", "anchor"),
        record("IT.TEST.OID", "Y"),
        record("IT.TEST.OID", "N"),
    ]);
    source.fail = Some((2, 10));
    let attempt = plan(&source, read(OdmScope::Records, None))
        .unwrap()
        .execute_observed(&source, limits());
    match *attempt.result.unwrap_err() {
        ExecutionError::SemanticDiagnostic {
            diagnostic,
            identity,
        } => {
            assert_eq!(diagnostic.code, ConditionCode::OdmNotUnique);
            assert_eq!(diagnostic.spec_paths, ["columns.VAL.derivation.odm"]);
            assert_eq!(
                diagnostic.context.get("records"),
                Some(&ContextValue::Integer("2".into()))
            );
            assert_eq!(
                diagnostic.context.get("repeated"),
                Some(&ContextValue::Scalar(Value::Bool(true)))
            );
            assert_eq!(
                diagnostic.context.get("differ"),
                Some(&ContextValue::Object(Default::default()))
            );
            assert_eq!(
                diagnostic.context.get("scope"),
                Some(&ContextValue::Object(
                    HIERARCHY
                        .into_iter()
                        .map(|column| (
                            FIELDS[column].into(),
                            ContextValue::Scalar(source.rows[0][column].clone())
                        ))
                        .collect()
                ))
            );
            assert!(identity.is_some());
        }
        error => panic!("unexpected failure: {error:?}"),
    }
    let mut trace = scope_prefix(3);
    for column in IDENTIFYING {
        trace.extend([(1, column), (2, column)]);
    }
    assert_eq!(*source.reads.borrow(), trace);
}

#[test]
fn later_identifying_cell_failure_outranks_a_would_be_ambiguity_with_no_fallback() {
    let mut source = table(vec![
        record("IT.ANCHOR", "anchor"),
        record("IT.TEST.OID", "Y"),
        record("IT.TEST.OID", "N"),
    ]);
    source.fail = Some((2, 1));
    let attempt = plan(&source, read(OdmScope::Records, None))
        .unwrap()
        .execute_observed(&source, limits());
    match *attempt.result.unwrap_err() {
        ExecutionError::Cell {
            path,
            source_row: 2,
            error: CellError::Access(Original(payload)),
        } => {
            assert_eq!(path, "columns.VAL.derivation.odm");
            assert!(Rc::ptr_eq(&payload, &source.payload));
        }
        error => panic!("unexpected failure: {error:?}"),
    }
    let mut trace = scope_prefix(3);
    trace.extend([(1, 0), (2, 0), (1, 1), (2, 1)]);
    assert_eq!(*source.reads.borrow(), trace);
    assert!(attempt.handler_counts.is_empty());
}

#[test]
fn invalid_fixed_schema_or_scope_is_rejected_before_any_cell_effect() {
    let mut source = table(vec![record("IT.ANCHOR", "anchor")]);
    assert_eq!(
        plan(&source, read(OdmScope::Groups(vec![99]), None)).unwrap_err(),
        PlanError::InvalidSource
    );
    source.schema = TableSchema::new(
        FIELDS
            .iter()
            .enumerate()
            .map(|(index, name)| Column {
                name: (*name).into(),
                kind: if index == 10 {
                    ColumnType::Int
                } else {
                    ColumnType::Str
                },
            })
            .collect(),
    )
    .unwrap();
    assert_eq!(
        plan(&source, read(OdmScope::Records, None)).unwrap_err(),
        PlanError::InvalidSource
    );
    assert!(source.reads.borrow().is_empty());
}
