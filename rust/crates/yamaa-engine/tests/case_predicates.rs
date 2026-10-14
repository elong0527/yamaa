//! Independent grain, ordering and original opaque-error controls for case predicates.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    predicate_compiler,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{
    Assignment, CaseBranch, Check, ColumnVerifications, ConversionHandler, DatasetExecution,
    DatasetPlan, ExecutionError, Expression, Limits, RowMode, RowTemplate, Verification,
};

struct Source {
    schema: TableSchema,
    values: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    fail: Option<(usize, usize)>,
    error: Rc<String>,
}
impl TableAccess for Source {
    type Error = Rc<String>;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.values.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access(self.error.clone()));
        }
        self.values
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
fn source() -> Source {
    Source {
        schema: schema(&[
            ("ID", ColumnType::Int),
            ("FLAG", ColumnType::Int),
            ("BAD", ColumnType::Str),
        ]),
        values: vec![
            vec![Value::Int(1), Value::Int(1), Value::Str("bad".into())],
            vec![Value::Int(1), Value::Missing, Value::Str("bad".into())],
            vec![Value::Int(2), Value::Missing, Value::Str("bad".into())],
            vec![Value::Int(3), Value::Int(-1), Value::Str("bad".into())],
        ],
        reads: RefCell::default(),
        fail: Some((0, 2)),
        error: Rc::new("opaque case source error".into()),
    }
}
fn assignment(column: usize, expression: Expression, path: &str) -> Assignment {
    Assignment {
        column,
        expression,
        path: path.into(),
    }
}
fn predicate() -> BoundPredicate {
    BoundPredicate::new_contextual(
        predicate_compiler::compile(
            "SRC.FLAG > 0",
            "columns.V.derivation.case[0].when",
            Default::default(),
        )
        .unwrap(),
        vec![Binding {
            name: "SRC.FLAG".into(),
            read: Read::Source(1),
        }],
        vec!["SRC.FLAG".into()],
    )
    .unwrap()
}
fn plan(source: &Source, when: BoundPredicate) -> DatasetPlan {
    DatasetPlan::new(
        source.schema.clone(),
        schema(&[
            ("ID", ColumnType::Int),
            ("REC", ColumnType::Int),
            ("V", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assignment(
                0,
                Expression::Source(0),
                "columns.ID.derivation.source",
            )],
            filter: None,
        }],
        vec![
            assignment(
                1,
                Expression::Literal(Value::Str("invalid".into())),
                "columns.REC.derivation.literal",
            ),
            assignment(
                2,
                Expression::Case(vec![
                    CaseBranch {
                        when: Some(when),
                        assignment: Box::new(assignment(
                            2,
                            Expression::Literal(Value::Int(7)),
                            "columns.V.derivation.case[0].then.literal",
                        )),
                    },
                    CaseBranch {
                        when: Some(
                            BoundPredicate::new(
                                predicate_compiler::compile(
                                    "FALSE",
                                    "columns.V.derivation.case[1].when",
                                    Default::default(),
                                )
                                .unwrap(),
                                vec![],
                            )
                            .unwrap(),
                        ),
                        assignment: Box::new(assignment(
                            2,
                            Expression::Collect {
                                column: 2,
                                identifier: "SRC.BAD".into(),
                                filter: None,
                                selection: None,
                            },
                            "columns.V.derivation.case[1].then.source",
                        )),
                    },
                    CaseBranch {
                        when: None,
                        assignment: Box::new(assignment(
                            2,
                            Expression::Literal(Value::Int(8)),
                            "columns.V.derivation.case[1].otherwise.literal",
                        )),
                    },
                ]),
                "columns.V.derivation.case",
            ),
        ],
        vec![0],
        vec![],
    )
    .unwrap()
    .with_conversion_handlers(vec![ConversionHandler {
        assignment_path: "columns.REC.derivation.literal".into(),
        handler: yamaa_core::conversion::LiteralHandler {
            value: Value::Int(9),
            spec_path: "columns.REC.derivation.unconvertible".into(),
        },
    }])
    .unwrap()
    .with_column_verifications(vec![ColumnVerifications {
        column: 1,
        checks: vec![Verification {
            path: "columns.REC.verifications[0].not_missing".into(),
            check: Check::NotMissing,
        }],
    }])
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
fn case_collects_distinct_present_values_and_skips_unselected_branch_reads() {
    let source = source();
    let attempt = plan(&source, predicate()).execute_observed(&source, limits());
    let completed = attempt.result.unwrap();
    assert_eq!(
        completed.dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(9), Value::Int(7)],
            vec![Value::Int(2), Value::Int(9), Value::Int(8)],
            vec![Value::Int(3), Value::Int(9), Value::Int(8)]
        ]
    );
    assert_eq!(
        *source.reads.borrow(),
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1)
        ]
    );
    assert_eq!(attempt.handler_counts.len(), 1);
    assert_eq!(attempt.handler_counts[0].count, 3);
    assert!(attempt.retained_verifications.is_empty());
    assert_eq!(completed.verifications.len(), 1);
    assert_eq!(
        completed.verifications[0].path,
        "columns.REC.verifications[0].not_missing"
    );
    assert_eq!(completed.verifications[0].evaluated_count, 3);
}
#[test]
fn case_operand_failure_keeps_original_identity_and_completed_literal_and_check_prefix() {
    let mut source = source();
    source.fail = Some((1, 1));
    let attempt = plan(&source, predicate()).execute_observed(&source, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(error),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original source error");
    };
    assert_eq!(path, "columns.V.derivation.case[0].when");
    assert_eq!(source_row, 1);
    assert!(Rc::ptr_eq(&error, &source.error));
    assert_eq!(
        *source.reads.borrow(),
        [(0, 0), (1, 0), (2, 0), (3, 0), (0, 1), (1, 1)]
    );
    assert_eq!(attempt.handler_counts.len(), 1);
    assert_eq!(attempt.handler_counts[0].count, 3);
    assert_eq!(attempt.retained_verifications.len(), 1);
}
#[test]
fn case_multiplicity_is_not_a_predicate_unknown_or_an_otherwise_fallback() {
    let mut source = source();
    source.fail = None;
    source.values[1][1] = Value::Int(2);
    let attempt = plan(&source, predicate()).execute_observed(&source, limits());
    let ExecutionError::MultipleValues {
        path,
        identifier,
        value_count,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("distinct source values must fail");
    };
    assert_eq!(path, "columns.V.derivation.case[0].when");
    assert_eq!(identifier, "SRC.FLAG");
    assert_eq!(value_count, 2);
    assert_eq!(identity.unwrap().values, [Value::Int(1)]);
    assert_eq!(
        *source.reads.borrow(),
        [(0, 0), (1, 0), (2, 0), (3, 0), (0, 1), (1, 1)]
    );
    assert_eq!(attempt.handler_counts[0].count, 3);
    assert_eq!(attempt.retained_verifications.len(), 1);
}

#[test]
fn false_boolean_operand_keeps_the_written_eager_source_read_and_original_error() {
    let source = source();
    let when = BoundPredicate::new_contextual(
        predicate_compiler::compile(
            "FALSE AND SRC.BAD = 'bad'",
            "columns.V.derivation.case[0].when",
            Default::default(),
        )
        .unwrap(),
        vec![Binding {
            name: "SRC.BAD".into(),
            read: Read::Source(2),
        }],
        vec!["SRC.BAD".into()],
    )
    .unwrap();
    let attempt = plan(&source, when).execute_observed(&source, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(error),
    } = *attempt.result.unwrap_err()
    else {
        panic!("FALSE AND must still resolve its written operand");
    };
    assert_eq!(path, "columns.V.derivation.case[0].when");
    assert_eq!(source_row, 0);
    assert!(Rc::ptr_eq(&error, &source.error));
    assert_eq!(
        *source.reads.borrow(),
        [(0, 0), (1, 0), (2, 0), (3, 0), (0, 2)]
    );
    assert_eq!(attempt.handler_counts[0].count, 3);
    assert_eq!(attempt.retained_verifications.len(), 1);
}
