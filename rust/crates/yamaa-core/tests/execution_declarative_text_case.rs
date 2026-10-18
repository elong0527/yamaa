//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod declarative_text_case {
        use crate::support::text_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{dataset::ExecutionError, numeric_lifecycle::HandlerKind};
        use yamaa_core::{
            dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            text_case::{Case, StringCase},
            value::Value,
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
        fn make_table(text: Value) -> Table {
            Table {
                schema: schema(),
                rows: vec![vec![Value::Str("row1".into()), text]],
                fail: RefCell::default(),
                reads: RefCell::default(),
            }
        }
        fn assignment(column: usize, mode: Case, missing: Option<Value>) -> Assignment {
            let path = "columns.TEXT.derivation.str_case";
            Assignment {
                column,
                path: path.into(),
                expression: Expression::StrCase {
                    source: Box::new(Assignment {
                        column,
                        path: format!("{path}.source"),
                        expression: Expression::Source(1),
                    }),
                    operation: StringCase::new("SRC.TEXT".into(), path.into(), mode, missing)
                        .unwrap(),
                },
            }
        }
        fn plan(mode: Case, missing: Option<Value>) -> DatasetPlan {
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
                        assignment(1, mode, missing),
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
        fn all_ascii_modes_preserve_non_ascii_and_perform_one_source_read() {
            for (mode, expected) in [
                (Case::Upper, "AB\u{00e9}C_D"),
                (Case::Lower, "ab\u{00e9}c_d"),
                (Case::Sentence, "Ab\u{00e9}c_d"),
                (Case::Title, "Ab\u{00e9}C_D"),
            ] {
                let table = make_table(Value::Str("aB\u{00e9}c_D".into()));
                let attempt = plan(mode, Some(Value::Missing)).execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    [vec![Value::Str("row1".into()), Value::Str(expected.into())]]
                );
                assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
                assert_eq!(attempt.handler_counts.len(), 1);
                assert_eq!(attempt.handler_counts[0].handler, HandlerKind::Missing);
                assert_eq!(attempt.handler_counts[0].count, 0);
            }
        }
        #[test]
        fn literal_recovery_is_not_case_transformed_and_is_converted_once() {
            for (handler, expected) in [
                (Value::Missing, Value::Missing),
                (Value::Str("MiXeD".into()), Value::Str("MiXeD".into())),
                (Value::Int(7), Value::Str("7".into())),
            ] {
                let table = make_table(Value::Missing);
                let attempt = plan(Case::Lower, Some(handler)).execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    [vec![Value::Str("row1".into()), expected]]
                );
                assert_eq!(
                    attempt.handler_counts[0].spec_path,
                    "columns.TEXT.derivation.str_case.missing"
                );
                assert_eq!(attempt.handler_counts[0].count, 1);
                assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
            }
        }
        #[test]
        fn semantic_missing_gives_missing_counted_as_the_default() {
            // Without `missing`, a missing source gives missing and the default
            // substitution is counted under `missing` (COLUMN-10, COLUMN-14).
            let table = make_table(Value::Missing);
            let attempt = plan(Case::Upper, None).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![Value::Str("row1".into()), Value::Missing]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                    .collect::<Vec<_>>(),
                [(
                    "columns.TEXT.derivation.str_case.missing",
                    HandlerKind::Missing,
                    1
                )]
            );
            assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        }
        #[test]
        fn nested_owned_case_operands_register_and_count_each_handler_once() {
            let mut declarations = plan(Case::Lower, Some(Value::Missing)).templates().to_vec();
            let Expression::StrCase { source, .. } = &mut declarations[0].assignments[1].expression
            else {
                panic!("outer case");
            };
            **source = assignment(1, Case::Upper, Some(Value::Str("MiXeD".into())));
            // Distinct authored occurrences retain distinct handler identities.
            let Expression::StrCase { operation, .. } = &mut source.expression else {
                panic!("inner case");
            };
            *operation = StringCase::new(
                "SRC.TEXT".into(),
                "columns.TEXT.derivation.inner.str_case".into(),
                Case::Upper,
                Some(Value::Str("MiXeD".into())),
            )
            .unwrap();
            let declaration =
                DatasetPlan::new(schema(), schema(), declarations, vec![], vec![0], vec![])
                    .unwrap();
            let table = make_table(Value::Missing);
            let attempt = declaration.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![Value::Str("row1".into()), Value::Str("mixed".into())]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.TEXT.derivation.str_case.missing", 0),
                    ("columns.TEXT.derivation.inner.str_case.missing", 1)
                ]
            );
            assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        }
        #[test]
        fn original_nonclone_error_allocation_and_rc_identity_return_without_retry_or_handler() {
            let table = make_table(Value::Missing);
            let marker = Rc::new(());
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            *table.fail.borrow_mut() = Some(Failure(payload, marker.clone()));
            let attempt = plan(Case::Upper, Some(Value::Str("fallback".into()))).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::Cell {
                path,
                source_row,
                error: CellError::Access(Failure(payload, retained)),
            } = *attempt.result.unwrap_err()
            else {
                panic!("original access error");
            };
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&retained, &marker));
            assert_eq!(path, "columns.TEXT.derivation.str_case.source");
            assert_eq!(source_row, 0);
            assert_eq!(attempt.handler_counts[0].count, 0);
            assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        }
    }
    mod declarative_text_extract {
        use crate::support::text_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{dataset::ExecutionError, numeric_lifecycle::HandlerKind};
        use yamaa_core::{
            dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            text_extract::StringExtract,
            value::Value,
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
        fn make_table(text: Value) -> Table {
            Table {
                schema: schema(),
                rows: vec![vec![Value::Str("row1".into()), text]],
                fail: RefCell::default(),
                reads: RefCell::default(),
            }
        }
        fn assignment(
            column: usize,
            pattern: &str,
            group: &str,
            missing: Option<Value>,
        ) -> Assignment {
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
                let result = plan(pattern, group, Some(Value::Str("literal".into()))).execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
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
            let attempt = plan("(a)", "1", Some(Value::Str("replacement".into()))).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
                let observed = plan.execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
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
        fn omitted_missing_handler_gives_missing_counted_as_the_default() {
            // A missing source is never an extraction failure: without
            // `missing` it gives missing, counted under `missing`.
            let table = make_table(Value::Missing);
            let attempt = plan("(a)", "1", None).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![Value::Str("row1".into()), Value::Missing]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "columns.TEXT.derivation.str_extract.missing",
                        HandlerKind::Missing,
                        1
                    ),
                    (
                        "columns.TEXT.derivation.str_extract.no_match",
                        HandlerKind::NoMatch,
                        0
                    )
                ]
            );
        }
        #[test]
        fn catastrophic_pattern_reaches_no_match_and_small_matches_start_fresh() {
            // (a+)+$ cannot match before the final "!". The linear-time search
            // says so, and the declared no_match handler answers the row.
            let table = make_table(Value::Str("a".repeat(30) + "!"));
            let attempt = plan("(a+)+$", "1", Some(Value::Missing)).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![
                    Value::Str("row1".into()),
                    Value::Str("fallback".into())
                ]]
            );
            let reached = attempt
                .handler_counts
                .iter()
                .filter(|count| count.count > 0)
                .collect::<Vec<_>>();
            assert_eq!(reached.len(), 1);
            assert_eq!(reached[0].handler, HandlerKind::NoMatch);
            assert_eq!(
                reached[0].spec_path,
                "columns.TEXT.derivation.str_extract.no_match"
            );
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
            for _ in 0..2 {
                assert_eq!(
                    plan("(a)", "1", None)
                        .execute(
                            &table,
                            &[],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None
                        )
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    [
                        vec![Value::Str("row1".into()), Value::Str("a".into())],
                        vec![Value::Str("row2".into()), Value::Str("a".into())],
                    ]
                );
                assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (1, 0), (1, 1)]);
                table.reads.borrow_mut().clear();
            }
        }
    }
    mod declarative_text_pad {
        use crate::support::text_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{dataset::ExecutionError, numeric_lifecycle::HandlerKind};
        use yamaa_core::{
            dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            text_case::{Case, StringCase},
            text_extract::StringExtract,
            text_pad::StringPad,
            text_template::StringTemplate,
            value::Value,
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
        fn make_table(text: Value) -> Table {
            Table {
                schema: schema(),
                rows: vec![vec![Value::Str("row1".into()), text]],
                fail: RefCell::default(),
                reads: RefCell::default(),
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
            from_assignment(assignment(width))
        }
        fn from_assignment(text: Assignment) -> DatasetPlan {
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
                        text,
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap()
        }
        fn padded(expression: Expression) -> DatasetPlan {
            let mut outer = assignment("4");
            let Expression::StrPad { source, .. } = &mut outer.expression else {
                unreachable!()
            };
            source.expression = expression;
            from_assignment(outer)
        }
        #[test]
        fn nested_extraction_keeps_all_declared_handler_sites_in_order_under_padding() {
            let path = "columns.TEXT.derivation.str_pad.source.str_extract";
            for (input, expected, missing, no_match) in [
                (Value::Missing, "   M", 1, 0),
                (Value::Str("b".into()), "   N", 0, 1),
                (Value::Str("a".into()), "   a", 0, 0),
            ] {
                let table = make_table(input);
                let plan = padded(Expression::StrExtract {
                    source: Box::new(Assignment {
                        column: 1,
                        path: format!("{path}.source"),
                        expression: Expression::Source(1),
                    }),
                    operation: StringExtract::new(
                        "SRC.TEXT".into(),
                        path.into(),
                        "(a)".into(),
                        "1".into(),
                        Some(Value::Str("M".into())),
                        Some(Value::Str("N".into())),
                    )
                    .unwrap(),
                });
                let attempt = plan.execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    [vec![Value::Str("row1".into()), Value::Str(expected.into())]]
                );
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                        .collect::<Vec<_>>(),
                    [
                        (
                            format!("{path}.missing").as_str(),
                            HandlerKind::Missing,
                            missing
                        ),
                        (
                            format!("{path}.no_match").as_str(),
                            HandlerKind::NoMatch,
                            no_match
                        ),
                    ]
                );
                assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
            }
        }
        #[test]
        fn nested_template_retains_zero_and_reached_missing_handler_sites_under_padding() {
            let path = "columns.TEXT.derivation.str_pad.source.str_template";
            for (input, expected, count) in [
                (Value::Missing, "   T", 1),
                (Value::Str("go".into()), "  go", 0),
                (Value::Str("".into()), "    ", 0),
            ] {
                let table = make_table(input);
                let plan = padded(Expression::StrTemplate {
                    operands: vec![Assignment {
                        column: 1,
                        path: format!("{path}.template"),
                        expression: Expression::Source(1),
                    }],
                    operation: StringTemplate::new(
                        "{TEXT}".into(),
                        path.into(),
                        Some(Value::Str("T".into())),
                    )
                    .unwrap(),
                });
                let attempt = plan.execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    [vec![Value::Str("row1".into()), Value::Str(expected.into())]]
                );
                assert_eq!(attempt.handler_counts.len(), 1);
                assert_eq!(
                    attempt.handler_counts[0].spec_path,
                    format!("{path}.missing")
                );
                assert_eq!(attempt.handler_counts[0].handler, HandlerKind::Missing);
                assert_eq!(attempt.handler_counts[0].count, count);
                assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
            }
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
                let attempt = plan(width).execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
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
            let attempt = plan(&width).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![Value::Str("row1".into()), Value::Missing]]
            );
            assert!(attempt.handler_counts.is_empty());
            let table = make_table(Value::Str("".into()));
            let attempt = plan(&width).execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::Allocation
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
            let attempt = plan("4").execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let Expression::StrPad { source, .. } = &mut templates[0].assignments[1].expression
            else {
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
            let attempt = declaration.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
        fn padding_keeps_all_original_rows_and_operand_order() {
            let table = Table {
                schema: schema(),
                rows: vec![
                    vec![Value::Str("row1".into()), Value::Str("a".into())],
                    vec![Value::Str("row2".into()), Value::Str("b".into())],
                ],
                fail: RefCell::default(),
                reads: RefCell::default(),
            };
            let result = plan("4")
                .execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                [
                    vec![Value::Str("row1".into()), Value::Str("   a".into())],
                    vec![Value::Str("row2".into()), Value::Str("   b".into())]
                ]
            );
            assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1), (1, 0), (1, 1)]);
        }
    }
    mod declarative_text_template {
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::json::Value as JsonValue;
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
                    assert_eq!(diagnostic.code, ConditionCode::TemplateMissing);
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
                yamaa_core::expression::compile(
                    "ID = 'row1'",
                    yamaa_core::expression::Context::Predicate,
                    "columns.RESULT.case[0].when",
                )
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
                        assignment: Box::new(assignment(
                            "{B}",
                            Some(Value::Str("unselected".into())),
                        )),
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
    }
    mod execution_attempts {
        //! Independent attempts retain exact values, original failures and effect order.
        use std::{
            cell::{Cell, RefCell},
            panic::{catch_unwind, AssertUnwindSafe},
            rc::Rc,
            sync::Arc,
        };
        use yamaa_core::{
            dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        use yamaa_core::{
            dataset::{ExecutionError, Resource},
            function_invocation::NoFunctions,
            limits::Limits,
        };

        #[derive(Debug)]
        struct Original {
            owner: Rc<()>,
            value: Box<u8>,
        }
        struct Table {
            schema: TableSchema,
            rows: usize,
            cells: Cell<usize>,
            error: RefCell<Option<Original>>,
            panic: Option<Arc<()>>,
        }
        impl TableAccess for Table {
            type Error = Original;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.rows
            }
            fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
                self.cells.set(self.cells.get() + 1);
                if let Some(payload) = &self.panic {
                    std::panic::panic_any(Arc::clone(payload));
                }
                if let Some(error) = self.error.borrow_mut().take() {
                    return Err(CellError::Access(error));
                }
                if row >= self.rows || column != 0 {
                    return Err(CellError::OutOfBounds { row, column });
                }
                Ok(ValueRef::Int(row as i64 + 1))
            }
        }
        fn schema(kind: ColumnType) -> TableSchema {
            TableSchema::new(vec![Column {
                name: "ID".into(),
                kind,
            }])
            .unwrap()
        }
        fn fixture(expression: Expression) -> (Table, DatasetPlan) {
            let schema = schema(ColumnType::Int);
            let plan = DatasetPlan::new(
                schema.clone(),
                schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![Assignment {
                        column: 0,
                        path: "columns.ID".into(),
                        expression,
                    }],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            (
                Table {
                    schema,
                    rows: 1,
                    cells: Cell::new(0),
                    error: RefCell::new(None),
                    panic: None,
                },
                plan,
            )
        }
        #[test]
        fn repeated_literal_attempts_keep_exact_rows_without_source_reads() {
            let (table, plan) = fixture(Expression::Literal(Value::Int(1)));
            for _ in 0..2 {
                let attempted = plan.execute(&table, &[], &mut NoFunctions::new(), None);
                assert_eq!(
                    attempted.result.unwrap().dataset.rows(),
                    [vec![Value::Int(1)]]
                );
                assert_eq!(table.cells.get(), 0);
            }
        }

        #[test]
        fn schema_and_fixed_shape_refusals_precede_source_reads() {
            let (mut table, plan) = fixture(Expression::Literal(Value::Int(1)));
            plan.execute(&table, &[], &mut NoFunctions::new(), None)
                .result
                .unwrap();
            table.schema = schema(ColumnType::Str);
            assert!(matches!(
                *plan
                    .execute(&table, &[], &mut NoFunctions::new(), None)
                    .result
                    .unwrap_err(),
                ExecutionError::SchemaMismatch
            ));
            assert_eq!(table.cells.get(), 0);
            table.schema = schema(ColumnType::Int);
            table.rows = Limits::TABLE_ROWS + 1;
            *table.error.borrow_mut() = Some(Original {
                owner: Rc::new(()),
                value: Box::new(0),
            });
            assert!(matches!(
                *plan.execute(&table, &[], &mut NoFunctions::new(), None).result.unwrap_err(),
                ExecutionError::SourceLimit {
                    resource: Resource::SourceRows,
                    limit: Limits::TABLE_ROWS,
                    required: Some(required),
                    source_index: 0,
                } if required == Limits::TABLE_ROWS + 1
            ));
            assert_eq!(table.cells.get(), 0);
            assert!(table.error.borrow().is_some());
        }

        #[test]
        fn returned_port_failure_keeps_its_original_owner_and_completed_observations() {
            let (table, plan) = fixture(Expression::Source(0));
            let owner = Rc::new(());
            let value = Box::new(27);
            let original = &*value as *const u8;
            *table.error.borrow_mut() = Some(Original {
                owner: Rc::clone(&owner),
                value,
            });
            let attempted = plan.execute(&table, &[], &mut NoFunctions::new(), None);
            let ExecutionError::Cell {
                error: CellError::Access(error),
                source_row: 0,
                ..
            } = *attempted.result.unwrap_err()
            else {
                panic!("original access owner was replaced")
            };
            assert!(Rc::ptr_eq(&error.owner, &owner));
            assert_eq!(&*error.value as *const u8, original);
            assert_eq!(table.cells.get(), 1);
            assert!(attempted.handler_counts.is_empty());
            assert!(attempted.retained_verifications.is_empty());
        }

        #[test]
        fn unwind_keeps_its_owner_and_a_fresh_attempt_uses_the_same_immutable_plan() {
            let (mut table, plan) = fixture(Expression::Source(0));
            let original = Arc::new(());
            table.panic = Some(Arc::clone(&original));
            let raised = catch_unwind(AssertUnwindSafe(|| {
                plan.execute(&table, &[], &mut NoFunctions::new(), None)
            }))
            .unwrap_err();
            assert!(Arc::ptr_eq(
                raised.downcast_ref::<Arc<()>>().unwrap(),
                &original
            ));
            assert_eq!(table.cells.get(), 1);
            table.panic = None;
            assert_eq!(
                plan.execute(&table, &[], &mut NoFunctions::new(), None)
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                [vec![Value::Int(1)]]
            );
            assert_eq!(table.cells.get(), 2);
        }

        fn literal_rows(table: &Table, count: usize, kind: ColumnType, text: bool) -> DatasetPlan {
            DatasetPlan::new(
                table.schema.clone(),
                schema(kind),
                (0..count)
                    .map(|row| RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![Assignment {
                            column: 0,
                            path: "columns.ID".into(),
                            expression: Expression::Literal(if text {
                                Value::Str("abcdef".into())
                            } else {
                                Value::Int(row as i64 + 1)
                            }),
                        }],
                        filter: None,
                    })
                    .collect(),
                vec![],
                vec![0],
                vec![],
            )
            .unwrap()
        }
        #[test]
        fn separate_attempts_keep_their_original_output_geometry_and_text() {
            let (table, _) = fixture(Expression::Literal(Value::Int(1)));
            let seven = literal_rows(&table, 7, ColumnType::Int, false);
            let one = literal_rows(&table, 1, ColumnType::Int, false);
            assert_eq!(
                seven
                    .execute(&table, &[], &mut NoFunctions::new(), None)
                    .result
                    .unwrap()
                    .dataset
                    .row_count(),
                7
            );
            assert_eq!(
                one.execute(&table, &[], &mut NoFunctions::new(), None)
                    .result
                    .unwrap()
                    .dataset
                    .row_count(),
                1
            );
            let text = literal_rows(&table, 1, ColumnType::Str, true);
            for _ in 0..2 {
                assert_eq!(
                    text.execute(&table, &[], &mut NoFunctions::new(), None)
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    [vec![Value::Str("abcdef".into())]]
                );
            }
        }

        #[test]
        fn duplicate_identity_failures_remain_fatal_on_each_attempt() {
            let (table, _) = fixture(Expression::Literal(Value::Int(1)));
            let duplicate = literal_rows(&table, 2, ColumnType::Str, true);
            for _ in 0..2 {
                assert!(matches!(
                    *duplicate
                        .execute(&table, &[], &mut NoFunctions::new(), None)
                        .result
                        .unwrap_err(),
                    ExecutionError::KeyFailures(_)
                ));
            }
        }

        #[test]
        fn repeated_column_patterns_keep_their_original_successful_findings() {
            use yamaa_core::dataset::{Check, ColumnVerifications, Verification};
            let (table, _) = fixture(Expression::Literal(Value::Int(1)));
            let text = literal_rows(&table, 1, ColumnType::Str, true);
            let pattern = yamaa_core::portable_regex::Pattern::compile("abcdef").unwrap();
            let checked = text
                .with_column_verifications(vec![ColumnVerifications {
                    column: 0,
                    checks: (0..12)
                        .map(|index| Verification {
                            path: format!("columns.ID.verifications[{index}].matches"),
                            check: Check::Matches(pattern.clone()),
                        })
                        .collect(),
                }])
                .unwrap();
            let result = checked
                .execute(&table, &[], &mut NoFunctions::new(), None)
                .result
                .unwrap();
            assert_eq!(result.dataset.rows(), [vec![Value::Str("abcdef".into())]]);
            assert_eq!(result.verifications.len(), 12);
            assert!(result
                .verifications
                .iter()
                .all(|check| check.evaluated_count == 1 && check.failed_count == 0));
        }

        #[test]
        fn repeated_predicate_attempts_keep_the_original_source_values() {
            use yamaa_core::bound_expression::{Binding, BoundPredicate, Read};
            use yamaa_core::expression::{compile, Context};
            let (table, _) = fixture(Expression::Literal(Value::Int(1)));
            let filter = BoundPredicate::new(
                compile("x > 0", Context::Predicate, "rows[0].filter").unwrap(),
                vec![Binding {
                    name: "x".into(),
                    read: Read::Source(0),
                }],
            )
            .unwrap();
            let plan = DatasetPlan::new(
                table.schema.clone(),
                table.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![Assignment {
                        column: 0,
                        path: "columns.ID".into(),
                        expression: Expression::Source(0),
                    }],
                    filter: Some(filter),
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            for _ in 0..2 {
                assert_eq!(
                    plan.execute(&table, &[], &mut NoFunctions::new(), None)
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    [vec![Value::Int(1)]]
                );
            }
        }
    }
    mod intermediate_ranges {
        //! Range narrowing preserves borrowed endpoint reads, native failures and observations.
        use crate::support::table_schema as schema;
        use std::cell::RefCell;
        use yamaa_core::{dataset::ExecutionError, numeric_lifecycle::HandlerKind};
        use yamaa_core::{
            dataset::{
                Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
                IntermediateKey, IntermediateSource, MatchRead, MatchValue, RelationRange,
                RelationRangeBound, RowMode, RowTemplate, SecondarySource, SourceSchemas,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value, ValueType},
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
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
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
        fn table(
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            failure: Option<(usize, usize)>,
        ) -> Table {
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
                require_match: false,
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
        #[test]
        fn missing_current_skips_all_donor_reads_and_records_the_absence_site() {
            let (primary, donor, plan) = fixture(Value::Missing, Value::Int(0), Some((0, 1)));
            let result = plan.execute(
                &primary,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let result = plan.execute(
                &primary,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let result = plan.execute(
                &primary,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let result = plan.execute(
                &primary,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
    }
    mod key_source_selection {
        //! Key choices share one physical attempt, preserve opaque failures and finish before columns.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{
            dataset::ExecutionError,
            function_invocation::{Argument, FunctionPort, HostError},
            function_signature::{FunctionIdentity, InvocationPlan},
        };
        use yamaa_core::{
            dataset::{
                Assignment, BoundFunction, DatasetPlan, Expression, FirstAvailable, Intermediate,
                IntermediateKey, IntermediateSource, MatchRead, MatchValue, RowMode, RowTemplate,
                SecondarySource, SelectionRead, SelectionSource, SourceSchemas,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        #[derive(Debug)]
        struct Failure(Box<u8>, Rc<()>);
        type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
        struct Table {
            name: &'static str,
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            failure: RefCell<Option<(usize, usize, Failure)>>,
            trace: Trace,
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
                self.trace.borrow_mut().push((self.name, row, column));
                if self
                    .failure
                    .borrow()
                    .as_ref()
                    .is_some_and(|(r, c, _)| *r == row && *c == column)
                {
                    return Err(CellError::Access(
                        self.failure.borrow_mut().take().unwrap().2,
                    ));
                }
                Ok(ValueRef::from(&self.rows[row][column]))
            }
        }
        fn tables() -> (Table, Table, Trace) {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let text = |s: &str| Value::Str(s.into());
            let table = |name, columns, rows| Table {
                name,
                schema: schema(columns),
                rows,
                failure: RefCell::default(),
                trace: trace.clone(),
            };
            (
                table(
                    "SRC",
                    &[("ID", T::Str), ("K", T::Str), ("UNUSED", T::Str)],
                    vec![
                        vec![text("1"), text("x"), text("hidden")],
                        vec![text("2"), text("y"), text("hidden")],
                        vec![text("3"), text("z"), text("hidden")],
                    ],
                ),
                table(
                    "DONOR",
                    &[("K", T::Str), ("V", T::Str)],
                    vec![
                        vec![text("x"), text("X")],
                        vec![text("y"), text("Y")],
                        vec![text("z"), text("Z")],
                    ],
                ),
                trace,
            )
        }
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("columns.C{column}.derivation.source"),
            }
        }
        fn plan(src: &Table, donor: &Table, id_type: T, column: Expression) -> DatasetPlan {
            DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: src.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "DONOR".into(),
                        schema: donor.schema.clone(),
                    }],
                },
                vec![Intermediate {
                    identifier: "P".into(),
                    path: "intermediates[0]".into(),
                    source: IntermediateSource::Input(0),
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "intermediates[0].key".into(),
                        value: MatchValue::Variable(MatchRead::Record {
                            column: 1,
                            identifier: "SRC.K".into(),
                        }),
                    }],
                    filter: None,
                    selection: None,
                    no_match: None,
                    require_match: true,
                    unique: vec![],
                    derivations: None,
                    pending_schema: None,
                    range: None,
                    columns: None,
                }],
                schema(&[
                    ("ID", id_type),
                    ("FIRST", T::Str),
                    ("SECOND", T::Str),
                    ("LATER", T::Str),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    filter: None,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            Expression::FirstAvailable(Box::new(FirstAvailable::new(
                                "columns.C1.first_available".into(),
                                vec![
                                    SelectionSource {
                                        path: "columns.C1.first_available.sources[0]".into(),
                                        read: SelectionRead::Expression(Box::new(assign(
                                            1,
                                            Expression::Intermediate {
                                                index: 0,
                                                column: 1,
                                            },
                                        ))),
                                    },
                                    SelectionSource {
                                        path: "columns.C1.first_available.sources[1]".into(),
                                        read: SelectionRead::Collect {
                                            column: 2,
                                            identifier: "SRC.UNUSED".into(),
                                            filter: None,
                                        },
                                    },
                                ],
                                Value::Missing,
                            ))),
                        ),
                        assign(
                            2,
                            Expression::Intermediate {
                                index: 0,
                                column: 1,
                            },
                        ),
                    ],
                }],
                vec![assign(3, column)],
                vec![0, 1, 2],
                vec![],
            )
            .unwrap()
        }

        #[test]
        fn two_key_reads_share_the_choice_but_columns_and_fresh_attempts_select_again() {
            let (src, donor, trace) = tables();
            *src.failure.borrow_mut() = Some((0, 2, Failure(Box::new(1), Rc::new(()))));
            let p = plan(
                &src,
                &donor,
                T::Str,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            );
            for _ in 0..2 {
                trace.borrow_mut().clear();
                let result = p
                    .execute(
                        &src,
                        &[&donor],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap();
                assert_eq!(
                    result
                        .dataset
                        .rows()
                        .iter()
                        .map(|r| r[1..].to_vec())
                        .collect::<Vec<_>>(),
                    [
                        vec![Value::Str("X".into()); 3],
                        vec![Value::Str("Y".into()); 3],
                        vec![Value::Str("Z".into()); 3]
                    ]
                );
                assert_eq!(
                    trace
                        .borrow()
                        .iter()
                        .filter(|&&(name, _, col)| name == "DONOR" && col == 0)
                        .count(),
                    18,
                    "one scan per key candidate and another per column candidate"
                );
                assert_eq!(
                    trace
                        .borrow()
                        .iter()
                        .filter(|&&(name, _, col)| name == "DONOR" && col == 1)
                        .count(),
                    9,
                    "sharing the choice still reads each requested value"
                );
                assert!(
                    !trace
                        .borrow()
                        .iter()
                        .any(|&(name, _, col)| name == "SRC" && col == 2),
                    "first_available never reaches its later operand"
                );
            }
            assert!(
                src.failure.borrow().is_some(),
                "unreached source failure remains owned by the source"
            );
        }

        #[test]
        fn selected_key_cell_failure_moves_once_and_prevents_the_next_key_or_column() {
            let (src, donor, trace) = tables();
            let payload = Box::new(7);
            let address = &*payload as *const u8;
            let marker = Rc::new(());
            *donor.failure.borrow_mut() = Some((0, 1, Failure(payload, marker.clone())));
            let p = plan(
                &src,
                &donor,
                T::Str,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            );
            let ExecutionError::SecondaryCell {
                error: CellError::Access(Failure(held, retained)),
                source_row,
                ..
            } = *p
                .execute(
                    &src,
                    &[&donor],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("original donor failure")
            };
            assert_eq!(source_row, 0);
            assert_eq!(&*held as *const u8, address);
            assert!(Rc::ptr_eq(&retained, &marker));
            assert_eq!(
                trace
                    .borrow()
                    .iter()
                    .filter(|&&(name, _, col)| name == "DONOR" && col == 1)
                    .count(),
                1
            );
            assert!(!trace
                .borrow()
                .iter()
                .any(|&(name, row, _)| name == "SRC" && row > 0));
        }

        struct Calls {
            signature: InvocationPlan,
            count: usize,
        }
        impl FunctionPort for Calls {
            type Error = Failure;
            fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
                (slot == 0).then_some(&self.signature)
            }
            fn call(&mut self, _: usize, _: &[Argument<'_>]) -> Result<Value, HostError<Failure>> {
                self.count += 1;
                Ok(Value::Str("called".into()))
            }
        }
        #[test]
        fn all_key_conversions_precede_callbacks_and_work_refusals_keep_the_existing_limit() {
            let (mut src, donor, trace) = tables();
            src.rows[1][0] = Value::Str("bad".into());
            let signature = InvocationPlan::new(
                FunctionIdentity {
                    name: "later".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "project.later".into(),
                },
                vec![],
                T::Str,
                false,
            )
            .unwrap();
            let p = plan(
                &src,
                &donor,
                T::Int,
                Expression::Function(BoundFunction::new(0, signature.clone(), vec![]).unwrap()),
            );
            let mut calls = Calls {
                signature,
                count: 0,
            };
            assert!(matches!(
                *p.execute(&src, &[&donor], &mut calls, None)
                    .result
                    .unwrap_err(),
                ExecutionError::Conversion { .. }
            ));
            assert_eq!(calls.count, 0);
            assert!(!trace
                .borrow()
                .iter()
                .any(|&(name, row, _)| name == "SRC" && row == 2));
            trace.borrow_mut().clear();
        }
    }
}
