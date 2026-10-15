use serde_json::Value as JsonValue;
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{dataset::ExecutionError, numeric_lifecycle::HandlerKind};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    diagnostic::ConditionCode,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    text_template::StringTemplate,
    value::{ColumnType as T, Value},
};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    failure: RefCell<Option<(usize, Failure)>>,
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
        if self
            .failure
            .borrow()
            .as_ref()
            .is_some_and(|(selected, _)| *selected == column)
        {
            let (_, error) = self.failure.borrow_mut().take().unwrap();
            return Err(CellError::Access(error));
        }
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: T::Str,
            })
            .collect(),
    )
    .unwrap()
}
fn table(a: Value, b: Value) -> Table {
    Table {
        schema: schema(&["ID", "A", "B"]),
        rows: vec![vec![Value::Str("row1".into()), a, b]],
        failure: RefCell::default(),
        reads: RefCell::default(),
    }
}
const PATH: &str = "columns.RESULT.derivation.str_template";
fn assignment(template: &str, missing: Option<Value>) -> Assignment {
    let operation = StringTemplate::new(template.into(), PATH.into(), missing).unwrap();
    let operands = operation
        .identifiers()
        .iter()
        .map(|name| Assignment {
            column: 1,
            path: format!("{PATH}.template"),
            expression: Expression::Source(if name == "A" { 1 } else { 2 }),
        })
        .collect();
    Assignment {
        column: 1,
        path: PATH.into(),
        expression: Expression::StrTemplate {
            operands,
            operation,
        },
    }
}
fn from_assignment(assignment: Assignment) -> DatasetPlan {
    DatasetPlan::new(
        schema(&["ID", "A", "B"]),
        schema(&["ID", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                assignment,
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
fn plan(template: &str, missing: Option<Value>) -> DatasetPlan {
    from_assignment(assignment(template, missing))
}
#[test]
fn repeated_occurrences_are_independent_reads_in_authored_order() {
    let table = table(Value::Str("alpha".into()), Value::Str("beta".into()));
    let result = plan("{A}-{A}-{B}", None).execute(
        &table,
        &[],
        &mut yamaa_core::function_invocation::NoFunctions::new(),
        None,
    );
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [vec![
            Value::Str("row1".into()),
            Value::Str("alpha-alpha-beta".into())
        ]]
    );
    assert!(result.handler_counts.is_empty());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (0, 1), (0, 2)]);
}
#[test]
fn first_missing_recovery_stops_before_later_failure_and_retains_literal_presence() {
    for missing in [None, Some(Value::Missing), Some(Value::Str("MiXeD".into()))] {
        let table = table(Value::Missing, Value::Str("later".into()));
        *table.failure.borrow_mut() = Some((2, Failure(Box::new(9), Rc::new(()))));
        let result = plan("{A}:{B}", missing.clone()).execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        );
        if let Some(expected) = missing {
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [vec![Value::Str("row1".into()), expected]]
            );
            assert_eq!(result.handler_counts.len(), 1);
            assert_eq!(result.handler_counts[0].handler, HandlerKind::Missing);
            assert_eq!(result.handler_counts[0].count, 1);
        } else {
            let ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            } = *result.result.unwrap_err()
            else {
                panic!("missing cause")
            };
            assert_eq!(diagnostic.code, ConditionCode::StrCaseMissing);
            assert_eq!(diagnostic.spec_paths, [PATH]);
            assert_eq!(
                diagnostic.context,
                [("variable".into(), JsonValue::String("A".into()))].into()
            );
            assert_eq!(identity.unwrap().values, [Value::Str("row1".into())]);
            assert!(result.handler_counts.is_empty());
        }
        assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        assert!(table.failure.borrow().is_some());
    }
}
#[test]
fn opaque_nonclone_allocation_and_rc_move_without_retry_or_later_effects() {
    let table = table(Value::Missing, Value::Missing);
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *table.failure.borrow_mut() = Some((1, Failure(payload, marker.clone())));
    let result = plan("{A}:{B}", Some(Value::Str("replacement".into()))).execute(
        &table,
        &[],
        &mut yamaa_core::function_invocation::NoFunctions::new(),
        None,
    );
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(payload, retained)),
    } = *result.result.unwrap_err()
    else {
        panic!("original cell error")
    };
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained));
    assert_eq!(path, format!("{PATH}.template"));
    assert_eq!(source_row, 0);
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(result.handler_counts.iter().all(|c| c.count == 0));
}
#[test]
fn type_failure_precedes_later_error_without_formatting_or_recovery() {
    let table = table(Value::Int(7), Value::Missing);
    *table.failure.borrow_mut() = Some((2, Failure(Box::new(9), Rc::new(()))));
    let result = plan("{A}:{B}", Some(Value::Missing)).execute(
        &table,
        &[],
        &mut yamaa_core::function_invocation::NoFunctions::new(),
        None,
    );
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *result.result.unwrap_err()
    else {
        panic!("type")
    };
    assert_eq!(diagnostic.code, ConditionCode::StrCaseInput);
    assert_eq!(diagnostic.spec_paths, [format!("{PATH}.template")]);
    assert_eq!(
        diagnostic.context,
        [
            ("source".into(), JsonValue::String("A".into())),
            ("expected".into(), JsonValue::String("str".into())),
            ("actual".into(), JsonValue::String("int".into()))
        ]
        .into()
    );
    assert!(identity.is_none());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(result.handler_counts.iter().all(|c| c.count == 0));
}
#[test]
fn repeated_templates_keep_each_original_occurrence() {
    let table = table(Value::Str("a".repeat(40)), Value::Str("b".repeat(40)));
    let plan = plan("{A}-{A}-{B}", Some(Value::Missing));
    for _ in 0..2 {
        assert_eq!(
            plan.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None
            )
            .result
            .unwrap()
            .dataset
            .rows()[0][1],
            Value::Str(format!(
                "{}-{}-{}",
                "a".repeat(40),
                "a".repeat(40),
                "b".repeat(40)
            ))
        );
    }
}
#[test]
fn nested_text_handlers_register_and_count_only_reached_sites() {
    use yamaa_core::text_case::{Case, StringCase};
    for template_outer in [false, true] {
        let table = table(Value::Missing, Value::Str("beta".into()));
        let mut template = assignment("{A}:{B}", Some(Value::Str("MiXeD".into())));
        let text = Assignment {
            column: 1,
            path: "columns.RESULT.derivation.str_case".into(),
            expression: Expression::StrCase {
                source: Box::new(Assignment {
                    column: 1,
                    path: "source".into(),
                    expression: Expression::Source(1),
                }),
                operation: StringCase::new(
                    "A".into(),
                    "columns.RESULT.derivation.str_case".into(),
                    Case::Upper,
                    Some(Value::Str("lower".into())),
                )
                .unwrap(),
            },
        };
        let outer = if template_outer {
            let Expression::StrTemplate { operands, .. } = &mut template.expression else {
                unreachable!()
            };
            operands[0] = text;
            template
        } else {
            let mut text = text;
            let Expression::StrCase { source, .. } = &mut text.expression else {
                unreachable!()
            };
            **source = template;
            text
        };
        let result = from_assignment(outer).execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None,
        );
        assert_eq!(
            result.result.unwrap().dataset.rows()[0][1],
            Value::Str(
                if template_outer {
                    "lower:beta"
                } else {
                    "MIXED"
                }
                .into()
            )
        );
        assert_eq!(result.handler_counts.len(), 2);
        let reached = result
            .handler_counts
            .iter()
            .filter(|c| c.count > 0)
            .collect::<Vec<_>>();
        assert_eq!(reached.len(), 1);
        assert_eq!(reached[0].count, 1);
    }
}
#[test]
fn template_and_later_assignment_keep_original_read_order() {
    let table = table(Value::Str("abc".into()), Value::Str("later".into()));
    let output = schema(&["ID", "RESULT", "LATER"]);
    let plan = DatasetPlan::new(
        table.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID".into(),
                    expression: Expression::Source(0),
                },
                assignment("{A}", None),
                Assignment {
                    column: 2,
                    path: "columns.LATER".into(),
                    expression: Expression::Source(2),
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert!(plan
        .execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None
        )
        .result
        .is_ok());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (0, 2)]);
}
#[test]
fn numeric_targets_convert_template_text_and_nested_extraction_can_shrink_it() {
    let table = table(Value::Str("7".into()), Value::Str("later".into()));
    let output = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: T::Str,
        },
        Column {
            name: "RESULT".into(),
            kind: T::Int,
        },
    ])
    .unwrap();
    let plan = DatasetPlan::new(
        table.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID".into(),
                    expression: Expression::Source(0),
                },
                assignment("{A}", Some(Value::Missing)),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None
        )
        .result
        .unwrap()
        .dataset
        .rows()[0],
        [Value::Str("row1".into()), Value::Int(7)]
    );
    let table = self::table(Value::Str("not-int".into()), Value::Missing);
    assert!(matches!(
        *plan
            .execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None
            )
            .result
            .unwrap_err(),
        ExecutionError::Conversion { .. }
    ));
    // The nested extraction sees the complete raw template before conversion.
    use yamaa_core::text_extract::StringExtract;
    let table = self::table(Value::Str("abcdef".into()), Value::Missing);
    let outer = Assignment {
        column: 1,
        path: "columns.RESULT.str_extract".into(),
        expression: Expression::StrExtract {
            source: Box::new(assignment("{A}", None)),
            operation: StringExtract::new(
                "A".into(),
                "columns.RESULT.str_extract".into(),
                "(.)".into(),
                "1".into(),
                None,
                None,
            )
            .unwrap(),
        },
    };
    assert_eq!(
        from_assignment(outer)
            .execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None
            )
            .result
            .unwrap()
            .dataset
            .rows()[0][1],
        Value::Str("a".into())
    );
}
#[test]
fn selected_template_case_preserves_unselected_effects() {
    use yamaa_core::dataset::CaseBranch;
    let table = table(Value::Str("abc".into()), Value::Missing);
    let when = yamaa_core::bound_expression::BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("ID = 'row1'", "columns.RESULT.case[0].when")
            .unwrap(),
        vec![yamaa_core::bound_expression::Binding {
            name: "ID".into(),
            read: yamaa_core::bound_expression::Read::Column(0),
        }],
    )
    .unwrap();
    let case = Assignment {
        column: 1,
        path: "columns.RESULT.case".into(),
        expression: Expression::Case(vec![
            CaseBranch {
                when: Some(when),
                assignment: Box::new(assignment("{A}", None)),
            },
            CaseBranch {
                when: None,
                assignment: Box::new(assignment("{B}", Some(Value::Str("unselected".into())))),
            },
        ]),
    };
    let plan = from_assignment(case);
    *table.failure.borrow_mut() = Some((2, Failure(Box::new(9), Rc::new(()))));
    assert!(plan
        .execute(
            &table,
            &[],
            &mut yamaa_core::function_invocation::NoFunctions::new(),
            None
        )
        .result
        .is_ok());
    assert!(table.failure.borrow().is_some());
    assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
}
