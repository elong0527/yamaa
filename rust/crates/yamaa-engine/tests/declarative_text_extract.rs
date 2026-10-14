use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    diagnostic::{ConditionCode, ContextValue},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    text_extract::StringExtract,
    value::{ColumnType as T, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, ExecutionError, Limits, Resource},
    numeric_lifecycle::HandlerKind,
};
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
fn assignment(column: usize, pattern: &str, group: &str, missing: Option<Value>) -> Assignment {
    let path = "columns.TEXT.derivation.str_extract";
    Assignment {
        column,
        path: path.into(),
        expression: Expression::StrExtract {
            source: Box::new(Assignment {
                column,
                path: format!("{path}.source"),
                expression: Expression::Source(1),
            }),
            operation: StringExtract::new(
                "SRC.TEXT".into(),
                path.into(),
                pattern.into(),
                group.into(),
                missing,
                Some(Value::Str("fallback".into())),
            )
            .unwrap(),
        },
    }
}
fn plan(pattern: &str, group: &str, missing: Option<Value>) -> DatasetPlan {
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
                assignment(1, pattern, group, missing),
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
fn captures_missing_empty_and_handler_counts_have_one_read_and_conversion() {
    for (pattern, group, input, expected, missing_count, no_match_count) in [
        ("(a)?b", "1", Value::Str("b".into()), Value::Missing, 0, 0),
        (
            "()b",
            "1",
            Value::Str("b".into()),
            Value::Str("".into()),
            0,
            0,
        ),
        (
            "()",
            "0",
            Value::Str("abc".into()),
            Value::Str("".into()),
            0,
            0,
        ),
        (
            "(a|ab)",
            "1",
            Value::Str("zab".into()),
            Value::Str("a".into()),
            0,
            0,
        ),
        (
            "(a)",
            "1",
            Value::Missing,
            Value::Str("literal".into()),
            1,
            0,
        ),
        (
            "(a)",
            "1",
            Value::Str("x".into()),
            Value::Str("fallback".into()),
            0,
            1,
        ),
    ] {
        let table = make_table(input);
        let result = plan(pattern, group, Some(Value::Str("literal".into())))
            .execute_observed(&table, limits());
        assert_eq!(
            result.result.unwrap().dataset.rows(),
            [vec![Value::Str("row1".into()), expected]]
        );
        assert_eq!(
            result
                .handler_counts
                .iter()
                .map(|count| (count.handler, count.count))
                .collect::<Vec<_>>(),
            [
                (HandlerKind::Missing, missing_count),
                (HandlerKind::NoMatch, no_match_count)
            ]
        );
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    }
}
#[test]
fn source_failure_preserves_original_nonclone_allocation_rc_and_effect_order() {
    let table = make_table(Value::Missing);
    let marker = Rc::new(());
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    *table.fail.borrow_mut() = Some(Failure(payload, marker.clone()));
    let attempt =
        plan("(a)", "1", Some(Value::Str("replacement".into()))).execute_observed(&table, limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *attempt.result.unwrap_err()
    else {
        panic!("original access error")
    };
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained));
    assert_eq!(source_row, 0);
    assert_eq!(path, "columns.TEXT.derivation.str_extract.source");
    assert!(attempt.handler_counts.iter().all(|count| count.count == 0));
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
#[test]
fn nested_case_and_extraction_recoveries_register_all_sites_and_reach_only_selected_ones() {
    use yamaa_core::text_case::{Case, StringCase};
    for extract_outer in [false, true] {
        let table = make_table(Value::Missing);
        let mut inner = assignment(1, "(.)", "1", Some(Value::Str("literal".into())));
        let path = "columns.TEXT.derivation.str_case";
        let case = StringCase::new(
            "SRC.TEXT".into(),
            path.into(),
            Case::Upper,
            Some(Value::Str("MiXeD".into())),
        )
        .unwrap();
        let outer = if extract_outer {
            inner.expression = Expression::StrCase {
                source: Box::new(Assignment {
                    column: 1,
                    path: format!("{path}.source"),
                    expression: Expression::Source(1),
                }),
                operation: case,
            };
            let mut outer = assignment(1, "(.)", "1", Some(Value::Missing));
            let Expression::StrExtract { source, .. } = &mut outer.expression else {
                panic!("extract")
            };
            **source = inner;
            outer
        } else {
            Assignment {
                column: 1,
                path: path.into(),
                expression: Expression::StrCase {
                    source: Box::new(inner),
                    operation: case,
                },
            }
        };
        let plan = DatasetPlan::new(
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
                    outer,
                ],
                filter: None,
            }],
            vec![],
            vec![0],
            vec![],
        )
        .unwrap();
        let observed = plan.execute_observed(&table, limits());
        assert_eq!(
            observed.result.unwrap().dataset.rows(),
            [vec![
                Value::Str("row1".into()),
                Value::Str(if extract_outer { "M" } else { "LITERAL" }.into())
            ]]
        );
        assert_eq!(observed.handler_counts.len(), 3);
        let reached = observed
            .handler_counts
            .iter()
            .filter(|item| item.count > 0)
            .collect::<Vec<_>>();
        assert_eq!(reached.len(), 1);
        assert_eq!(reached[0].count, 1);
        assert_eq!(reached[0].handler, HandlerKind::Missing);
        assert_eq!(
            reached[0].spec_path,
            if extract_outer {
                "columns.TEXT.derivation.str_case.missing"
            } else {
                "columns.TEXT.derivation.str_extract.missing"
            }
        );
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    }
}
#[test]
fn unhandled_mapping_failure_preserves_keys_and_context_admission() {
    let table = make_table(Value::Missing);
    let attempt = plan("(a)", "1", None).execute_observed(&table, limits());
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("mapping failure")
    };
    assert_eq!(diagnostic.code, ConditionCode::StrCaseMissing);
    assert_eq!(
        diagnostic.context,
        [(
            "variable".into(),
            ContextValue::Scalar(Value::Str("SRC.TEXT".into()))
        )]
        .into()
    );
    assert_eq!(identity.unwrap().values, [Value::Str("row1".into())]);
    let mut quota = limits();
    quota.identity_cells = 0;
    assert!(matches!(
        *plan("(a)", "1", None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::IdentityCells,
            ..
        }
    ));
    let mut quota = limits();
    quota.scalar_text_bytes = 20;
    assert!(matches!(
        *plan("(a)", "1", None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
}
#[test]
fn cumulative_regex_work_and_text_refusal_do_not_recover_or_retry() {
    let table = make_table(Value::Str("aaaa".into()));
    let mut quota = limits();
    quota.work_cells = 0;
    assert!(matches!(
        *plan("(a)", "1", None).execute(&table, quota).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(table.reads.borrow().is_empty());
    let mut quota = limits();
    quota.work_cells = 10;
    let attempt = plan("(x)", "1", Some(Value::Missing)).execute_observed(&table, quota);
    let error = attempt.result.unwrap_err();
    assert!(
        matches!(
            *error,
            ExecutionError::Limit {
                resource: Resource::PredicateRegexWork | Resource::PredicateRegexStateCells,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(attempt.handler_counts.iter().all(|count| count.count == 0));
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    let table = Table {
        schema: schema(),
        rows: vec![
            vec![Value::Str("row1".into()), Value::Str("a".into())],
            vec![Value::Str("row2".into()), Value::Str("a".into())],
        ],
        fail: RefCell::default(),
        reads: RefCell::default(),
    };
    let mut quota = limits();
    quota.work_cells = 12;
    let attempt = plan("(a)", "1", None).execute_observed(&table, quota);
    let error = attempt.result.unwrap_err();
    assert!(
        matches!(
            *error,
            ExecutionError::Limit {
                resource: Resource::PredicateRegexWork | Resource::PredicateRegexStateCells,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (1, 0), (1, 1)]);
    assert!(
        plan("(a)", "1", None).execute(&table, limits()).is_ok(),
        "a fresh run has fresh counters"
    );
}
