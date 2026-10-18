//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod dataset_odm {
        //! Independently specified item scopes and original opaque table effects.
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::*;
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            diagnostic::ConditionCode,
            expression::ErrorKind,
            odm_source::{FIELDS, HIERARCHY, IDENTIFYING},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

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
                yamaa_core::expression::compile(
                    text,
                    yamaa_core::expression::Context::Predicate,
                    path,
                )
                .unwrap(),
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
                source: None,
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
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            .execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            match *attempt.result.unwrap_err() {
                ExecutionError::SemanticDiagnostic {
                    diagnostic,
                    identity,
                } => {
                    assert_eq!(diagnostic.code, ConditionCode::OdmNotUnique);
                    assert_eq!(diagnostic.spec_paths, ["columns.VAL.derivation.odm"]);
                    assert_eq!(
                        diagnostic.context.get("records"),
                        Some(&JsonValue::Number(
                            ("2").parse().expect("canonical integer")
                        ))
                    );
                    assert_eq!(
                        diagnostic.context.get("repeated"),
                        Some(&JsonValue::Bool(true))
                    );
                    assert_eq!(
                        diagnostic.context.get("differ"),
                        Some(&JsonValue::Object(Default::default()))
                    );
                    assert_eq!(
                        diagnostic.context.get("scope"),
                        Some(&JsonValue::Object(
                            HIERARCHY
                                .into_iter()
                                .map(|column| (
                                    FIELDS[column].into(),
                                    JsonValue::from(source.rows[0][column].clone())
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
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
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

        fn deferred_read(driver: usize, groups: Option<Vec<usize>>) -> BoundOdm {
            let mut declaration = read(OdmScope::Records, None).read().clone();
            declaration.row = None;
            declaration.source = Some(if driver == 0 {
                AggregateSource::Primary
            } else {
                AggregateSource::Secondary(driver - 1)
            });
            declaration.scope = OdmScope::Templates(
                vec![OdmTemplateScope {
                    row: "after-window".into(),
                    driver,
                    groups,
                }]
                .into(),
            );
            BoundOdm::new(declaration).unwrap()
        }

        fn deferred_plan(
            source: &Table,
            driver: usize,
            mode: RowMode,
            bound: BoundOdm,
        ) -> Result<DatasetPlan, PlanError> {
            let grouped = matches!(mode, RowMode::Groups(_));
            let output = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Str,
                },
                Column {
                    name: "SEQ".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "VAL".into(),
                    kind: ColumnType::Str,
                },
            ])
            .unwrap();
            let primary = if driver == 0 {
                source.schema.clone()
            } else {
                schema(&["ordinary"])
            };
            let secondary = if driver == 0 {
                vec![]
            } else {
                vec![SecondarySource {
                    name: "SRC".into(),
                    schema: source.schema.clone(),
                }]
            };
            DatasetPlan::new_with_sources(
                primary,
                secondary,
                output,
                vec![RowTemplate {
                    mode: if driver == 0 {
                        mode
                    } else {
                        RowMode::Driver {
                            source: driver,
                            mode: Box::new(mode),
                        }
                    },
                    assignments: vec![Assignment {
                        column: 0,
                        expression: Expression::Literal(Value::Str("one".into())),
                        path: "rows[0].derivations.ID".into(),
                    }],
                    filter: if grouped {
                        None
                    } else {
                        Some(predicate("SRC.ItemOID = 'IT.ANCHOR'", 9, "rows[0].filter"))
                    },
                }],
                vec![
                    Assignment {
                        column: 1,
                        expression: Expression::Window(Window {
                            kind: WindowKind::RowNumber,
                            group_by: vec![],
                            order_by: vec![OrderTerm {
                                column: 0,
                                descending: false,
                                nulls_first: false,
                            }],
                            filter: None,
                        }),
                        path: "columns.SEQ.derivation".into(),
                    },
                    Assignment {
                        column: 2,
                        expression: Expression::Odm(Box::new(bound)),
                        path: "columns.VAL.derivation".into(),
                    },
                ],
                vec![0, 1],
                vec![],
            )
        }

        #[test]
        fn deferred_record_scope_keeps_each_original_occurrence_after_row_numbers() {
            let mut second_anchor = record("IT.ANCHOR", "anchor2");
            second_anchor[6] = Value::Str("2".into());
            let mut second_item = record("IT.TEST.OID", "second");
            second_item[6] = Value::Str("2".into());
            let source = table(vec![
                record("IT.ANCHOR", "anchor1"),
                record("IT.TEST.OID", "first"),
                second_anchor,
                second_item,
            ]);
            let result = deferred_plan(&source, 0, RowMode::Records, deferred_read(0, None))
                .unwrap()
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[
                    vec![
                        Value::Str("one".into()),
                        Value::Int(1),
                        Value::Str("first".into())
                    ],
                    vec![
                        Value::Str("one".into()),
                        Value::Int(2),
                        Value::Str("second".into())
                    ],
                ]
            );
        }

        #[test]
        fn deferred_secondary_driver_borrows_original_table_without_reading_primary() {
            let source = table(vec![
                record("IT.ANCHOR", "anchor"),
                record("IT.TEST.OID", "Y"),
            ]);
            let mut primary = table(vec![vec![Value::Str("poison".into())]]);
            primary.schema = schema(&["ordinary"]);
            primary.fail = Some((0, 0));
            let attempt = deferred_plan(&source, 1, RowMode::Records, deferred_read(1, None))
                .unwrap()
                .execute(
                    &primary,
                    &[&source],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                &[vec![
                    Value::Str("one".into()),
                    Value::Int(1),
                    Value::Str("Y".into())
                ]]
            );
            assert!(primary.reads.borrow().is_empty());
            let mut expected = scope_prefix(2);
            expected.push((1, 10));
            assert_eq!(*source.reads.borrow(), expected);
        }

        #[test]
        fn deferred_value_access_keeps_the_original_error_and_authored_leaf_path() {
            let mut source = table(vec![
                record("IT.ANCHOR", "anchor"),
                record("IT.TEST.OID", "Y"),
            ]);
            source.fail = Some((1, 10));
            let attempt = deferred_plan(&source, 0, RowMode::Records, deferred_read(0, None))
                .unwrap()
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            match *attempt.result.unwrap_err() {
                ExecutionError::Cell {
                    path,
                    source_row: 1,
                    error: CellError::Access(Original(payload)),
                } => {
                    assert_eq!(path, "columns.VAL.derivation.odm");
                    assert!(Rc::ptr_eq(&payload, &source.payload));
                }
                error => panic!("unexpected error: {error:?}"),
            }
            let mut expected = scope_prefix(2);
            expected.push((1, 10));
            assert_eq!(*source.reads.borrow(), expected);
            assert!(attempt.handler_counts.is_empty());
        }

        #[test]
        fn forged_template_count_driver_and_incomplete_group_scope_fail_before_cells() {
            let source = table(vec![record("IT.ANCHOR", "anchor")]);
            assert_eq!(
                deferred_plan(&source, 0, RowMode::Records, deferred_read(1, None)).unwrap_err(),
                PlanError::InvalidSource
            );
            assert_eq!(
                deferred_plan(
                    &source,
                    0,
                    RowMode::Groups(vec![0, 2, 9]),
                    deferred_read(0, Some(vec![0]))
                )
                .unwrap_err(),
                PlanError::InvalidSource
            );
            let mut declaration = deferred_read(0, None).read().clone();
            let OdmScope::Templates(contexts) = &mut declaration.scope else {
                unreachable!()
            };
            *contexts = vec![contexts[0].clone(), contexts[0].clone()].into();
            assert_eq!(
                deferred_plan(
                    &source,
                    0,
                    RowMode::Records,
                    BoundOdm::new(declaration).unwrap()
                )
                .unwrap_err(),
                PlanError::InvalidSource
            );
            assert!(source.reads.borrow().is_empty());
        }

        #[test]
        fn deferred_scope_is_rejected_in_row_assignments_before_any_cell() {
            let source = table(vec![record("IT.ANCHOR", "anchor")]);
            let admitted =
                deferred_plan(&source, 0, RowMode::Records, deferred_read(0, None)).unwrap();
            let mut templates = admitted.templates().to_vec();
            templates[0].assignments.push(admitted.columns()[1].clone());
            let result = DatasetPlan::new(
                source.schema.clone(),
                admitted.output().clone(),
                templates,
                vec![admitted.columns()[0].clone()],
                admitted.keys().to_vec(),
                vec![],
            );
            assert_eq!(result.unwrap_err(), PlanError::InvalidSource);
            assert!(source.reads.borrow().is_empty());
        }

        #[test]
        fn deferred_metadata_retains_the_complete_label_without_a_storage_capacity_quota() {
            let source = table(vec![record("IT.ANCHOR", "anchor")]);
            let mut declaration = deferred_read(0, None).read().clone();
            let mut row = String::with_capacity(16 * 1024 * 1024);
            row.push_str("small label with excessive retained capacity");
            declaration.scope = OdmScope::Templates(
                vec![OdmTemplateScope {
                    row,
                    driver: 0,
                    groups: None,
                }]
                .into(),
            );
            let admitted = BoundOdm::new(declaration).unwrap();
            let OdmScope::Templates(contexts) = &admitted.read().scope else {
                panic!("the admitted declaration must retain its authored template scope");
            };
            assert_eq!(contexts.len(), 1);
            assert_eq!(
                contexts[0].row,
                "small label with excessive retained capacity"
            );
            assert_eq!(contexts[0].driver, 0);
            assert_eq!(contexts[0].groups, None);
            assert!(source.reads.borrow().is_empty());
        }

        #[test]
        fn forged_nondeferred_foreign_row_owner_is_rejected_before_poison_cells() {
            let source = table(vec![record("IT.ANCHOR", "anchor")]);
            let mut donor = table(vec![record("IT.TEST.OID", "poison")]);
            donor.fail = Some((0, 10));
            let mut declaration = read(OdmScope::Records, None).read().clone();
            declaration.source = Some(AggregateSource::Secondary(0));
            declaration.phase = AggregatePhase::RowConstruction;
            let bound = BoundOdm::new(declaration.clone()).unwrap();
            let result = DatasetPlan::new_with_sources(
                source.schema.clone(),
                vec![SecondarySource {
                    name: "DONOR".into(),
                    schema: donor.schema.clone(),
                }],
                schema(&["ID", "VAL"]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    filter: None,
                    assignments: vec![
                        Assignment {
                            column: 0,
                            expression: Expression::Literal(Value::Str("one".into())),
                            path: "rows[0].derivations.ID".into(),
                        },
                        Assignment {
                            column: 1,
                            expression: Expression::Odm(Box::new(bound)),
                            path: "rows[0].derivations.VAL".into(),
                        },
                    ],
                }],
                vec![],
                vec![0],
                vec![],
            );
            assert_eq!(result.unwrap_err(), PlanError::InvalidSource);
            declaration.phase = AggregatePhase::Derivation;
            let result = DatasetPlan::new_with_sources(
                source.schema.clone(),
                vec![SecondarySource {
                    name: "DONOR".into(),
                    schema: donor.schema.clone(),
                }],
                schema(&["ID", "VAL"]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    filter: None,
                    assignments: vec![Assignment {
                        column: 0,
                        expression: Expression::Literal(Value::Str("one".into())),
                        path: "rows[0].derivations.ID".into(),
                    }],
                }],
                vec![Assignment {
                    column: 1,
                    expression: Expression::Odm(Box::new(BoundOdm::new(declaration).unwrap())),
                    path: "columns.VAL.derivation".into(),
                }],
                vec![0],
                vec![],
            );
            assert_eq!(result.unwrap_err(), PlanError::InvalidSource);
            assert!(source.reads.borrow().is_empty());
            assert!(donor.reads.borrow().is_empty());
        }
    }
    mod declarative_flags {
        //! Lazy reads, exact raw truth, handler ownership and attempt-local refusals.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::{
            bound_expression::Read,
            dataset::{
                Assignment, ConversionHandler, DatasetPlan, Expression, Flag, FlagBinding,
                FlagRead, Intermediate, IntermediateKey, MatchRead, MatchValue, RowMode,
                RowTemplate, SecondarySource, SourceSchemas,
            },
            expression::{compile, Context},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        #[derive(Debug)]
        struct Failure(Box<u8>, Rc<()>);
        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            reads: RefCell<Vec<(usize, usize)>>,
            fail: RefCell<Option<Failure>>,
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
        fn table() -> Table {
            Table {
                schema: schema(&[("ID", T::Int), ("AGE", T::Int)]),
                rows: vec![
                    vec![Value::Int(1), Value::Int(70)],
                    vec![Value::Int(2), Value::Int(5)],
                    vec![Value::Int(3), Value::Missing],
                ],
                reads: RefCell::default(),
                fail: RefCell::default(),
            }
        }
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("columns.C{column}.derivation.flag"),
            }
        }
        fn flag(expr: &str, read: FlagRead, true_value: Value) -> Expression {
            Expression::Flag(
                Flag::new(
                    compile(
                        expr,
                        Context::Predicate,
                        "columns.C1.derivation.flag.condition",
                    )
                    .unwrap(),
                    vec![FlagBinding {
                        name: "AGE".into(),
                        read,
                    }],
                    true_value,
                    Some(Value::Int(2)),
                    Some(Value::Int(3)),
                )
                .unwrap(),
            )
        }
        fn plan(t: &Table, true_value: Value) -> DatasetPlan {
            DatasetPlan::new(
                t.schema.clone(),
                schema(&[("ID", T::Int), ("F", T::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            flag("AGE >= 65", FlagRead::Read(Read::Source(1)), true_value),
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
        #[test]
        fn three_truths_only_selected_literal_conversion_and_one_outer_handler_owner() {
            let table = table();
            let plan = plan(&table, Value::Str("bad".into()))
                .with_conversion_handlers(vec![ConversionHandler {
                    assignment_path: "columns.C1.derivation.flag".into(),
                    handler: yamaa_core::conversion::LiteralHandler {
                        value: Value::Int(99),
                        spec_path: "columns.C1.derivation.unconvertible".into(),
                    },
                }])
                .unwrap();
            let attempt = plan.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [
                    vec![Value::Int(1), Value::Int(99)],
                    vec![Value::Int(2), Value::Int(2)],
                    vec![Value::Int(3), Value::Int(3)]
                ]
            );
            // The UNKNOWN row takes the declared `missing` and is counted there.
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.unconvertible", 1),
                    ("columns.C1.derivation.flag.missing", 1)
                ]
            );
            assert_eq!(
                *table.reads.borrow(),
                [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
            );
        }
        #[test]
        fn opaque_read_moves_once_and_never_selects_a_branch_or_later_row() {
            let table = table();
            let marker = Rc::new(());
            let payload = Box::new(7);
            let address = &*payload as *const u8;
            *table.fail.borrow_mut() = Some(Failure(payload, marker.clone()));
            let attempt = plan(&table, Value::Int(1)).execute(
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
                panic!("opaque")
            };
            assert_eq!(path, "columns.C1.derivation.flag.condition");
            assert_eq!(source_row, 0);
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&marker, &retained));
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.flag.missing", 0)]
            );
            assert_eq!(*table.reads.borrow(), [(0, 0), (0, 1)]);
        }
        #[test]
        fn repeated_named_operands_share_choice_and_suppress_inherited_no_match_counts() {
            let table = table();
            let donor = Table {
                schema: schema(&[("K", T::Int), ("V", T::Int)]),
                rows: vec![
                    vec![Value::Int(1), Value::Int(70)],
                    vec![Value::Int(2), Value::Int(5)],
                ],
                reads: RefCell::default(),
                fail: RefCell::default(),
            };
            let flag = flag(
                "AGE > 65 AND AGE < 100",
                FlagRead::Read(Read::Intermediate {
                    index: 0,
                    column: 1,
                }),
                Value::Int(1),
            );
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: table.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "TAB".into(),
                        schema: donor.schema.clone(),
                    }],
                },
                vec![Intermediate {
                    identifier: "MATCHED".into(),
                    path: "intermediates[0]".into(),
                    source: yamaa_core::dataset::IntermediateSource::Input(0),
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "intermediates[0].key.K".into(),
                        value: MatchValue::Variable(MatchRead::Output {
                            column: 0,
                            identifier: "ID".into(),
                        }),
                    }],
                    filter: None,
                    selection: None,
                    no_match: Some(Value::Missing),
                    require_match: false,
                    unique: Vec::new(),
                    derivations: None,
                    pending_schema: None,

                    range: None,
                    columns: None,
                }],
                schema(&[("ID", T::Int), ("F", T::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![assign(1, flag)],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &table,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [
                    vec![Value::Int(1), Value::Int(1)],
                    vec![Value::Int(2), Value::Int(2)],
                    vec![Value::Int(3), Value::Int(3)]
                ]
            );
            // Predicate reads suppress `no_match`; the UNKNOWN condition counts.
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.flag.missing", 1)]
            );
            assert_eq!(
                donor.reads.borrow().iter().filter(|(_, c)| *c == 1).count(),
                4
            );
            assert_eq!(
                donor.reads.borrow().iter().filter(|(_, c)| *c == 0).count(),
                6
            );
        }
        #[test]
        fn repeated_flag_attempts_read_each_original_operand_once() {
            let table = table();
            for _ in 0..2 {
                assert!(plan(&table, Value::Int(1))
                    .execute(
                        &table,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .is_ok());
                assert_eq!(table.reads.borrow().len(), 6);
                table.reads.borrow_mut().clear();
            }
        }

        #[test]
        fn implicit_collections_refuse_ambiguity_with_complete_authored_key_identity() {
            let mut table = table();
            table.rows = vec![
                vec![Value::Int(1), Value::Int(70)],
                vec![Value::Int(1), Value::Int(5)],
            ];
            let plan = DatasetPlan::new(
                table.schema.clone(),
                schema(&[("ID", T::Int), ("F", T::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![assign(
                    1,
                    flag(
                        "AGE >= 65",
                        FlagRead::Collect {
                            column: 1,
                            identifier: "SRC.AGE".into(),
                        },
                        Value::Int(1),
                    ),
                )],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::MultipleValues {
                path,
                identifier,
                value_count,
                identity,
            } = *attempt.result.unwrap_err()
            else {
                panic!("ambiguity")
            };
            assert_eq!(path, "columns.C1.derivation.flag.condition");
            assert_eq!(identifier, "SRC.AGE");
            assert_eq!(value_count, 2);
            let identity = identity.unwrap();
            assert_eq!(identity.position, 0);
            assert_eq!(identity.values, [Value::Int(1)]);
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("columns.C1.derivation.flag.missing", 0)]
            );
            assert_eq!(
                table
                    .reads
                    .borrow()
                    .iter()
                    .filter(|(_, c)| *c == 1)
                    .copied()
                    .collect::<Vec<_>>(),
                [(0, 1), (1, 1)]
            );
        }
        #[test]
        fn constant_flag_needs_only_identity_reads_and_never_counts_unknown() {
            let table = table();
            let constant = Expression::Flag(
                Flag::new(
                    compile("TRUE", Context::Predicate, "flag.condition").unwrap(),
                    vec![],
                    Value::Str("Y".into()),
                    None,
                    None,
                )
                .unwrap(),
            );
            let plan = DatasetPlan::new(
                table.schema.clone(),
                schema(&[("ID", T::Int), ("F", T::Str)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0)), assign(1, constant)],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [
                    vec![Value::Int(1), Value::Str("Y".into())],
                    vec![Value::Int(2), Value::Str("Y".into())],
                    vec![Value::Int(3), Value::Str("Y".into())],
                ]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.count))
                    .collect::<Vec<_>>(),
                [("flag.missing", 0)]
            );
            assert_eq!(*table.reads.borrow(), [(0, 0), (1, 0), (2, 0)]);
        }
        fn named_plan(table: &Table, donor: &Table, filtered: bool) -> DatasetPlan {
            use yamaa_core::bound_expression::{Binding, BoundPredicate};
            let filter = filtered.then(|| {
                BoundPredicate::new(
                    compile("TAB.V > 0", Context::Predicate, "intermediates[0].filter").unwrap(),
                    vec![Binding {
                        name: "TAB.V".into(),
                        read: Read::Source(1),
                    }],
                )
                .unwrap()
            });
            DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: table.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "TAB".into(),
                        schema: donor.schema.clone(),
                    }],
                },
                vec![Intermediate {
                    identifier: "MATCHED".into(),
                    path: "intermediates[0]".into(),
                    source: yamaa_core::dataset::IntermediateSource::Input(0),
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "intermediates[0].key.K".into(),
                        value: MatchValue::Variable(MatchRead::Output {
                            column: 0,
                            identifier: "ID".into(),
                        }),
                    }],
                    filter: filter.map(Into::into),
                    selection: None,
                    no_match: Some(Value::Missing),
                    require_match: false,
                    unique: Vec::new(),
                    derivations: None,
                    pending_schema: None,

                    range: None,
                    columns: None,
                }],
                schema(&[("ID", T::Int), ("F", T::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![assign(
                    1,
                    flag(
                        "AGE > 65 AND AGE < 100",
                        FlagRead::Read(Read::Intermediate {
                            index: 0,
                            column: 1,
                        }),
                        Value::Int(1),
                    ),
                )],
                vec![0],
                vec![],
            )
            .unwrap()
        }
        #[test]
        fn lazy_filter_and_donor_opaque_failures_keep_original_payload_and_effect_boundary() {
            for filtered in [false, true] {
                let table = table();
                let marker = Rc::new(());
                let payload = Box::new(9);
                let address = &*payload as *const u8;
                let donor = Table {
                    schema: schema(&[("K", T::Int), ("V", T::Int)]),
                    rows: vec![vec![Value::Int(1), Value::Int(70)]],
                    reads: RefCell::default(),
                    fail: RefCell::new(Some(Failure(payload, marker.clone()))),
                };
                let attempt = named_plan(&table, &donor, filtered).execute(
                    &table,
                    &[&donor],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let failure = if filtered {
                    let ExecutionError::Predicate { source_row, error } =
                        *attempt.result.unwrap_err()
                    else {
                        panic!("filter")
                    };
                    assert_eq!(source_row, 0);
                    assert_eq!(error.spec_path, "intermediates[0].filter");
                    let yamaa_core::expression::ErrorKind::Resolution {
                        identifier,
                        error: CellError::Access(failure),
                    } = error.kind
                    else {
                        panic!("opaque")
                    };
                    assert_eq!(identifier, "TAB.V");
                    // The record matches its key before the filter narrows it.
                    assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1)]);
                    failure
                } else {
                    let ExecutionError::SecondaryCell {
                        path,
                        source,
                        source_row,
                        error: CellError::Access(failure),
                    } = *attempt.result.unwrap_err()
                    else {
                        panic!("donor")
                    };
                    assert_eq!(path, "intermediates[0]");
                    assert_eq!(source, 0);
                    assert_eq!(source_row, 0);
                    assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1)]);
                    failure
                };
                assert_eq!(&*failure.0 as *const u8, address);
                assert!(Rc::ptr_eq(&failure.1, &marker));
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|count| (count.spec_path.as_str(), count.count))
                        .collect::<Vec<_>>(),
                    [("columns.C1.derivation.flag.missing", 0)]
                );
            }
        }
    }
    mod declarative_intermediate_keys {
        //! Destination-free authored key execution, effect boundaries and exact equality.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            dataset::{
                Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, MatchRead,
                MatchValue, PlanError, RowMode, RowTemplate, SecondarySource, SourceSchemas,
            },
            expression::{compile, Context},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            text_pad::StringPad,
            value::{ColumnType as T, Value},
        };
        #[derive(Debug)]
        struct Failure(Box<u8>, Rc<()>);
        type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
        struct Table {
            name: &'static str,
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            fail: RefCell<Option<(usize, Failure)>>,
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
                    .fail
                    .borrow()
                    .as_ref()
                    .is_some_and(|(c, _)| *c == column)
                {
                    return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().1));
                }
                Ok(ValueRef::from(&self.rows[row][column]))
            }
        }
        fn tables(raw: Value, kind: T, donor: Value) -> (Table, Table, Trace) {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let primary = Table {
                name: "SRC",
                schema: schema(&[("ID", T::Int), ("RAW", kind)]),
                rows: vec![vec![Value::Int(1), raw]],
                fail: RefCell::default(),
                trace: trace.clone(),
            };
            let secondary = Table {
                name: "TAB",
                schema: schema(&[
                    ("K", kind),
                    ("VAL", T::Str),
                    ("K2", T::Str),
                    ("FILTER", T::Int),
                ]),
                rows: vec![vec![
                    donor,
                    Value::Str("7".into()),
                    Value::Str("unused".into()),
                    Value::Int(1),
                ]],
                fail: RefCell::default(),
                trace: trace.clone(),
            };
            (primary, secondary, trace)
        }
        fn read() -> MatchRead {
            MatchRead::Record {
                column: 1,
                identifier: "SRC.RAW".into(),
            }
        }
        fn key(value: MatchValue) -> IntermediateKey {
            IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key.K".into(),
                value,
            }
        }
        fn item(keys: Vec<IntermediateKey>) -> Intermediate {
            Intermediate {
                identifier: "LOOK".into(),
                path: "intermediates[0]".into(),
                source: yamaa_core::dataset::IntermediateSource::Input(0),
                keys,
                filter: None,
                selection: None,
                no_match: Some(Value::Missing),
                require_match: false,
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,

                range: None,
                columns: None,
            }
        }
        fn pad(width: &str) -> MatchValue {
            MatchValue::StrPad {
                read: read(),
                operation: Some(
                    StringPad::new("SRC.RAW", "intermediates[0].key.K.str_pad", width).unwrap(),
                ),
            }
        }
        fn plan(
            src: &Table,
            tab: &Table,
            item: Intermediate,
            target: T,
        ) -> Result<DatasetPlan, PlanError> {
            let assign = |column, expression| Assignment {
                column,
                path: format!("columns.C{column}.derivation.source"),
                expression,
            };
            DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: src.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "TAB".into(),
                        schema: tab.schema.clone(),
                    }],
                },
                vec![item],
                schema(&[("ID", T::Int), ("V", target), ("V2", target)]),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![
                    assign(
                        1,
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    ),
                    assign(
                        2,
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    ),
                ],
                vec![0],
                vec![],
            )
        }
        fn filter() -> BoundPredicate {
            BoundPredicate::new(
                compile(
                    "TAB.FILTER > 0",
                    Context::Predicate,
                    "intermediates[0].filter",
                )
                .unwrap(),
                vec![Binding {
                    name: "TAB.FILTER".into(),
                    read: Read::Source(3),
                }],
            )
            .unwrap()
        }
        #[test]
        fn padding_without_dummy_output_preserves_unicode_width_and_single_cached_selection() {
            let (src, tab, trace) = tables(
                Value::Str("\u{e9}\u{1f980}".into()),
                T::Str,
                Value::Str("  \u{e9}\u{1f980}".into()),
            );
            let mut selected = item(vec![key(pad("4"))]);
            selected.filter = Some(filter().into());
            let plan = plan(&src, &tab, selected, T::Int).unwrap();
            for _ in 0..2 {
                trace.borrow_mut().clear();
                let observed = plan.execute(
                    &src,
                    &[&tab],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    observed.result.unwrap().dataset.rows(),
                    [vec![Value::Int(1), Value::Int(7), Value::Int(7)]]
                );
                assert!(observed.handler_counts.is_empty());
                // The record matches its padded key before the filter narrows it.
                assert_eq!(
                    *trace.borrow(),
                    [
                        ("SRC", 0, 0),
                        ("SRC", 0, 1),
                        ("TAB", 0, 0),
                        ("TAB", 0, 3),
                        ("TAB", 0, 1),
                        ("TAB", 0, 1)
                    ]
                );
            }
        }
        #[test]
        fn expression_dependency_opaque_failure_precedes_filter_and_preserves_payload_identity() {
            let (src, tab, trace) = tables(Value::Missing, T::Str, Value::Str("".into()));
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            let marker = Rc::new(());
            *src.fail.borrow_mut() = Some((1, Failure(payload, marker.clone())));
            *tab.fail.borrow_mut() = Some((3, Failure(Box::new(8), Rc::new(()))));
            let mut selected = item(vec![key(pad("4"))]);
            selected.filter = Some(filter().into());
            let observed = plan(&src, &tab, selected, T::Str).unwrap().execute(
                &src,
                &[&tab],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::Cell {
                path,
                source_row,
                error: CellError::Access(Failure(retained, identity)),
            } = *observed.result.unwrap_err()
            else {
                panic!("original current dependency failure")
            };
            assert_eq!(&*retained as *const u8, address);
            assert!(Rc::ptr_eq(&identity, &marker));
            assert_eq!(path, "intermediates[0]");
            assert_eq!(source_row, 0);
            assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("SRC", 0, 1)]);
            assert!(tab.fail.borrow().is_some());
            assert!(observed.handler_counts.is_empty());
        }
        /// An ordinary selector resolves its current match value before any
        /// donor record is matched or narrowed, as an expression selector does
        /// (LOOKUP-25).
        #[test]
        fn ordinary_selector_resolves_its_raw_dependency_before_the_filter_and_keeps_its_payload() {
            let (src, tab, trace) = tables(Value::Missing, T::Str, Value::Str("".into()));
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            let marker = Rc::new(());
            *src.fail.borrow_mut() = Some((1, Failure(payload, marker.clone())));
            *tab.fail.borrow_mut() = Some((3, Failure(Box::new(8), Rc::new(()))));
            let mut selected = item(vec![key(MatchValue::Variable(read()))]);
            selected.filter = Some(filter().into());
            let observed = plan(&src, &tab, selected, T::Str).unwrap().execute(
                &src,
                &[&tab],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::Cell {
                path,
                source_row,
                error: CellError::Access(Failure(retained, identity)),
            } = *observed.result.unwrap_err()
            else {
                panic!("original current dependency failure")
            };
            assert_eq!(&*retained as *const u8, address);
            assert!(Rc::ptr_eq(&identity, &marker));
            assert_eq!(path, "intermediates[0]");
            assert_eq!(source_row, 0);
            assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("SRC", 0, 1)]);
            assert!(tab.fail.borrow().is_some());
            assert!(observed.handler_counts.is_empty());
        }
        #[test]
        fn missing_first_key_does_not_skip_later_padding_refusal_or_current_dependency() {
            let (src, tab, trace) = tables(Value::Str("present".into()), T::Str, Value::Missing);
            let mut later = key(pad(&"9".repeat(81)));
            later.source_column = 2;
            later.path = "intermediates[0].key.K2".into();
            let selected = item(vec![key(MatchValue::Literal(Value::Missing)), later]);
            let observed = plan(&src, &tab, selected, T::Str).unwrap().execute(
                &src,
                &[&tab],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *observed.result.unwrap_err(),
                ExecutionError::Allocation
            ));
            assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("SRC", 0, 1)]);
            assert!(observed.handler_counts.is_empty());
        }
        #[test]
        fn key_missingness_never_matches_and_counts_each_reading_handler() {
            let (src, tab, _) = tables(Value::Missing, T::Str, Value::Missing);
            let observed = plan(&src, &tab, item(vec![key(pad("4"))]), T::Str)
                .unwrap()
                .execute(
                    &src,
                    &[&tab],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
            assert_eq!(
                observed.result.unwrap().dataset.rows(),
                [vec![Value::Int(1), Value::Missing, Value::Missing]]
            );
            assert_eq!(
                observed
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.count))
                    .collect::<Vec<_>>(),
                [
                    ("columns.C1.derivation.source.no_match", 1),
                    ("columns.C2.derivation.source.no_match", 1)
                ]
            );
        }
        #[test]
        fn matching_reads_the_selected_donor_payload() {
            let (src, tab, trace) =
                tables(Value::Str("ab".into()), T::Str, Value::Str("ab".into()));
            let plan = plan(
                &src,
                &tab,
                item(vec![key(MatchValue::Source(read()))]),
                T::Str,
            )
            .unwrap();
            assert!(plan
                .execute(
                    &src,
                    &[&tab],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            assert!(trace
                .borrow()
                .iter()
                .any(|(name, _, col)| *name == "TAB" && *col == 1));
        }
        #[test]
        fn typed_numeric_matching_preserves_precision_and_defers_the_sole_output_conversion() {
            for (current, donor, matched) in [
                (7, 7., true),
                (9007199254740993, 9007199254740992., false),
                (i64::MIN, -9223372036854775808., true),
                (i64::MAX, 9223372036854775808., false),
            ] {
                let (src, mut tab, _) = tables(Value::Int(current), T::Int, Value::float(donor));
                tab.schema = schema(&[
                    ("K", T::Float),
                    ("VAL", T::Str),
                    ("K2", T::Str),
                    ("FILTER", T::Int),
                ]);
                let observed = plan(
                    &src,
                    &tab,
                    item(vec![key(MatchValue::Source(read()))]),
                    T::Int,
                )
                .unwrap()
                .execute(
                    &src,
                    &[&tab],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    observed.result.unwrap().dataset.rows(),
                    [vec![
                        Value::Int(1),
                        if matched {
                            Value::Int(7)
                        } else {
                            Value::Missing
                        },
                        if matched {
                            Value::Int(7)
                        } else {
                            Value::Missing
                        }
                    ]]
                );
            }
            let (src, mut tab, trace) =
                tables(Value::Str("x".into()), T::Str, Value::Str("x".into()));
            tab.rows.push(tab.rows[0].clone());
            tab.rows[0][1] = Value::Str("bad".into());
            let observed = plan(
                &src,
                &tab,
                item(vec![key(MatchValue::Source(read()))]),
                T::Int,
            )
            .unwrap()
            .execute(
                &src,
                &[&tab],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(matches!(
                *observed.result.unwrap_err(),
                ExecutionError::MultipleMatches { match_count: 2, .. }
            ));
            assert!(trace
                .borrow()
                .iter()
                .all(|(name, _, col)| *name != "TAB" || *col == 0));
        }
        #[test]
        fn malformed_bound_coordinates_conflicting_names_and_unadmitted_width_never_reach_ports() {
            let (src, tab, trace) = tables(Value::Str("x".into()), T::Str, Value::Str("x".into()));
            for value in [
                MatchValue::Source(MatchRead::Record {
                    column: 99,
                    identifier: "SRC.RAW".into(),
                }),
                MatchValue::StrPad {
                    read: read(),
                    operation: None,
                },
            ] {
                assert!(matches!(
                    plan(&src, &tab, item(vec![key(value)]), T::Str),
                    Err(PlanError::InvalidIntermediate)
                ));
            }
            let first = key(MatchValue::Source(read()));
            let mut second = key(MatchValue::Source(MatchRead::Record {
                column: 0,
                identifier: "SRC.RAW".into(),
            }));
            second.source_column = 2;
            assert!(matches!(
                plan(&src, &tab, item(vec![first, second]), T::Str),
                Err(PlanError::InvalidIntermediate)
            ));
            assert!(trace.borrow().is_empty());
        }
        /// The padded match value is complete before any record is matched,
        /// and the filter is asked only about a record that matched it.
        #[test]
        fn padding_precedes_the_match_and_the_filter_narrows_only_a_matched_record() {
            for (width, failed) in [("1000000", false), ("7", true)] {
                let (src, mut tab, trace) = tables(
                    Value::Str("present".into()),
                    T::Str,
                    Value::Str("present".into()),
                );
                tab.rows[0][3] = Value::Str("bad".into());
                let mut selected = item(vec![key(pad(width))]);
                selected.filter = Some(filter().into());
                let observed = plan(&src, &tab, selected, T::Str).unwrap().execute(
                    &src,
                    &[&tab],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                if failed {
                    assert!(matches!(
                        *observed.result.unwrap_err(),
                        ExecutionError::Predicate { .. }
                    ));
                    assert_eq!(
                        *trace.borrow(),
                        [("SRC", 0, 0), ("SRC", 0, 1), ("TAB", 0, 0), ("TAB", 0, 3)]
                    );
                    assert!(observed.handler_counts.is_empty());
                } else {
                    assert_eq!(
                        observed.result.unwrap().dataset.rows(),
                        [vec![Value::Int(1), Value::Missing, Value::Missing]]
                    );
                    assert_eq!(
                        *trace.borrow(),
                        [("SRC", 0, 0), ("SRC", 0, 1), ("TAB", 0, 0)]
                    );
                }
            }
        }
        #[test]
        fn donor_filter_keeps_only_true_in_written_order_before_match_count_and_value() {
            let (src, mut tab, trace) =
                tables(Value::Str("x".into()), T::Str, Value::Str("x".into()));
            tab.rows[0][3] = Value::Missing;
            let mut excluded = tab.rows[0].clone();
            excluded[3] = Value::Int(-1);
            let mut kept = tab.rows[0].clone();
            kept[1] = Value::Str("8".into());
            kept[3] = Value::Int(1);
            tab.rows.extend([excluded, kept]);
            let mut selected = item(vec![key(MatchValue::Source(read()))]);
            selected.filter = Some(filter().into());
            let observed = plan(&src, &tab, selected, T::Int).unwrap().execute(
                &src,
                &[&tab],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                observed.result.unwrap().dataset.rows(),
                [vec![Value::Int(1), Value::Int(8), Value::Int(8)]]
            );
            // Each record matches its key, then the filter narrows it.
            assert_eq!(
                *trace.borrow(),
                [
                    ("SRC", 0, 0),
                    ("SRC", 0, 1),
                    ("TAB", 0, 0),
                    ("TAB", 0, 3),
                    ("TAB", 1, 0),
                    ("TAB", 1, 3),
                    ("TAB", 2, 0),
                    ("TAB", 2, 3),
                    ("TAB", 2, 1),
                    ("TAB", 2, 1)
                ]
            );
        }
        #[test]
        fn typed_calendar_and_datetime_keys_compare_calendar_values_including_collected_precision()
        {
            use yamaa_core::temporal::{Date, DateTime, Precision};
            let year = Date::new(2025, 1, 1, Precision::Year).unwrap();
            let day = Date::new(2025, 1, 1, Precision::Day).unwrap();
            let midnight = DateTime::new(day, 0, 0, 0, Precision::Day).unwrap();
            let second = DateTime::new(day, 0, 0, 0, Precision::Second).unwrap();
            for (current, donor, kind) in [
                (Value::Date(year), Value::Date(day), T::Date),
                (
                    Value::DateTime(midnight),
                    Value::DateTime(second),
                    T::DateTime,
                ),
                (Value::float(-0.), Value::float(0.), T::Float),
            ] {
                let (src, tab, _) = tables(current, kind, donor);
                let observed = plan(
                    &src,
                    &tab,
                    item(vec![key(MatchValue::Source(read()))]),
                    T::Str,
                )
                .unwrap()
                .execute(
                    &src,
                    &[&tab],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    observed.result.unwrap().dataset.rows(),
                    [vec![
                        Value::Int(1),
                        Value::Str("7".into()),
                        Value::Str("7".into())
                    ]]
                );
            }
        }
    }
    mod declarative_rounding {
        use std::cell::RefCell;
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::{
            conversion::LiteralHandler,
            dataset::{
                Assignment, ConversionHandler, DatasetPlan, Expression, Rounding, RowMode,
                RowTemplate,
            },
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
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
            fn cell(
                &self,
                _: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
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
                                    path: "columns.C1.derivation.round_half_away_from_zero.source"
                                        .into(),
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
        #[test]
        fn opaque_source_failure_precedes_scalar_type_validation_without_a_retry() {
            let source = table(Value::Str("F".into()), true);
            let result = plan(&source, ColumnType::Float).execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
        fn missing_precedes_runtime_type_validation_and_rounding_failure_cannot_use_conversion_handler(
        ) {
            let source = table(Value::Missing, false);
            let result = plan(&source, ColumnType::Float).execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let result = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
                *plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
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
            let result = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [vec![Value::Int(1), Value::Int(7)]]
            );
            assert_eq!(result.handler_counts.len(), 1);
            assert_eq!(result.handler_counts[0].count, 1);
        }
    }
    mod declarative_temporal {
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::{
            dataset::{
                Assignment, DatasetPlan, Expression, RowMode, RowTemplate, Temporal, TemporalKind,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        // Opaque caller-thread errors need neither cloning nor formatting.
        struct Opaque(Rc<()>);
        struct Table {
            schema: TableSchema,
            first: Value,
            fail_at: usize,
            failure: RefCell<Option<Box<Opaque>>>,
            reads: RefCell<Vec<usize>>,
        }
        impl TableAccess for Table {
            type Error = Box<Opaque>;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                1
            }
            fn cell(
                &self,
                _: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.reads.borrow_mut().push(column);
                if column == self.fail_at {
                    return Err(CellError::Access(
                        self.failure.borrow_mut().take().expect("single effect"),
                    ));
                }
                Ok(match column {
                    0 => ValueRef::Int(1),
                    1 => ValueRef::from(&self.first),
                    2 => ValueRef::Date("2024-01-01".parse().unwrap()),
                    _ => panic!("invalid table column"),
                })
            }
        }
        fn plan(table: &Table) -> DatasetPlan {
            let path = "columns.DAY.derivation.study_day";
            DatasetPlan::new(
                table.schema.clone(),
                schema(&[("ID", ColumnType::Int), ("DAY", ColumnType::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        Assignment {
                            column: 0,
                            expression: Expression::Source(0),
                            path: "columns.ID.derivation".into(),
                        },
                        Assignment {
                            column: 1,
                            path: "columns.DAY.derivation".into(),
                            expression: Expression::Temporal {
                                operands: vec![
                                    Assignment {
                                        column: 1,
                                        expression: Expression::Source(1),
                                        path: format!("{path}.date"),
                                    },
                                    Assignment {
                                        column: 1,
                                        expression: Expression::Source(2),
                                        path: format!("{path}.reference"),
                                    },
                                ],
                                operation: Temporal::new(
                                    TemporalKind::StudyDay,
                                    vec!["SRC.DATE".into(), "SRC.REF".into()],
                                    path.into(),
                                )
                                .unwrap(),
                            },
                        },
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
        fn temporal_operands_stop_on_first_failure_and_missing_does_not_skip_later_authority() {
            for (first, fail_at, expected_reads) in [
                (Value::Missing, 1, vec![1]),
                (Value::Missing, 2, vec![1, 2]),
                (Value::Date("2024-01-02".parse().unwrap()), 2, vec![1, 2]),
            ] {
                let token = Rc::new(());
                let original = Box::new(Opaque(token.clone()));
                let address = &*original as *const Opaque;
                let table = Table {
                    schema: schema(&[
                        ("ID", ColumnType::Int),
                        ("DATE", ColumnType::Date),
                        ("REF", ColumnType::Date),
                    ]),
                    first,
                    fail_at,
                    failure: RefCell::new(Some(original)),
                    reads: RefCell::default(),
                };
                let attempted = plan(&table).execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let ExecutionError::Cell {
                    error: CellError::Access(error),
                    ..
                } = *attempted.result.err().unwrap()
                else {
                    panic!("original cell error required")
                };
                assert_eq!(&*error as *const Opaque, address);
                assert!(Rc::ptr_eq(&error.0, &token));
                assert_eq!(
                    table
                        .reads
                        .borrow()
                        .iter()
                        .copied()
                        .filter(|&c| c != 0)
                        .collect::<Vec<_>>(),
                    expected_reads
                );
                assert!(table.failure.borrow().is_none());
                assert!(attempted.handler_counts.is_empty());
            }
        }
        #[test]
        fn temporal_type_failure_precedes_later_operand_and_cannot_use_conversion_handler() {
            use yamaa_core::{conversion::LiteralHandler, dataset::ConversionHandler};
            let table = Table {
                schema: schema(&[
                    ("ID", ColumnType::Int),
                    ("DATE", ColumnType::Date),
                    ("REF", ColumnType::Date),
                ]),
                first: Value::Str("2024-01-01".into()),
                fail_at: 2,
                failure: RefCell::new(Some(Box::new(Opaque(Rc::new(()))))),
                reads: RefCell::default(),
            };
            let admitted = plan(&table)
                .with_conversion_handlers(vec![ConversionHandler {
                    assignment_path: "columns.DAY.derivation".into(),
                    handler: LiteralHandler {
                        spec_path: "columns.DAY.derivation.unconvertible".into(),
                        value: Value::Int(7),
                    },
                }])
                .unwrap();
            let attempted = admitted.execute(
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::ScalarDiagnostic(finding) = *attempted.result.err().unwrap() else {
                panic!("typed temporal diagnostic required")
            };
            assert_eq!(finding.definition().requirement, Some("REQ-0606"));
            assert_eq!(
                finding.spec_paths,
                ["columns.DAY.derivation.study_day.date"]
            );
            assert_eq!(
                table
                    .reads
                    .borrow()
                    .iter()
                    .copied()
                    .filter(|&c| c != 0)
                    .collect::<Vec<_>>(),
                [1]
            );
            assert!(table.failure.borrow().is_some());
            assert!(attempted
                .handler_counts
                .iter()
                .all(|counter| counter.count == 0));
        }
    }
    mod declarative_temporal_imputation {
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{dataset::ExecutionError, numeric_lifecycle::HandlerKind};
        use yamaa_core::{
            dataset::{
                completion_policy, Assignment, DatasetPlan, DayDeclaration as D, Expression,
                Imputation, ImputationKind as K, RowMode, RowTemplate, TemporalType as Target,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            temporal::Precision as P,
            value::{ColumnType as T, Value},
        };
        #[derive(Debug, PartialEq)]
        struct Failure(Box<u8>, Rc<()>);
        struct Table {
            schema: TableSchema,
            source: Value,
            bound: Value,
            fail: RefCell<Option<Failure>>,
            bad_column: usize,
            reads: RefCell<Vec<usize>>,
        }
        impl TableAccess for Table {
            type Error = Failure;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                1
            }
            fn cell(&self, _: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
                self.reads.borrow_mut().push(column);
                if column == self.bad_column {
                    if let Some(failure) = self.fail.borrow_mut().take() {
                        return Err(CellError::Access(failure));
                    }
                }
                Ok(match column {
                    0 => ValueRef::Int(1),
                    1 => ValueRef::from(&self.source),
                    2 => ValueRef::from(&self.bound),
                    _ => panic!("column"),
                })
            }
        }
        fn make_table(source: Value) -> Table {
            Table {
                schema: schema(&[("ID", T::Int), ("TEXT", T::Str), ("REF", T::Date)]),
                source,
                bound: Value::Date("2024-02-20".parse().unwrap()),
                fail: RefCell::default(),
                bad_column: usize::MAX,
                reads: RefCell::default(),
            }
        }
        fn date_kind(day: D<'_>) -> K {
            K::Impute(
                completion_policy(Target::Date, Some("6"), Some(day), P::Year, None, "impute")
                    .unwrap(),
            )
        }
        fn plan_with(
            table: &Table,
            output: T,
            operation: Imputation,
            operands: Vec<Assignment>,
        ) -> DatasetPlan {
            DatasetPlan::new(
                table.schema.clone(),
                schema(&[("ID", T::Int), ("DATE", output)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        Assignment {
                            column: 0,
                            path: "columns.ID.derivation".into(),
                            expression: Expression::Source(0),
                        },
                        Assignment {
                            column: 1,
                            path: "columns.DATE.derivation".into(),
                            expression: Expression::Imputation {
                                operation,
                                operands,
                            },
                        },
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap()
        }
        /// Each declared operand after the source reads the table's REF date.
        fn operands(path: &str, operation: &Imputation) -> Vec<Assignment> {
            operation
                .operands()
                .map(|(field, _)| Assignment {
                    column: 1,
                    path: format!("{path}.{field}"),
                    expression: Expression::Source(if field == "source" { 1 } else { 2 }),
                })
                .collect()
        }
        fn make_plan(table: &Table, invalid: Option<Value>) -> DatasetPlan {
            let path = "columns.DATE.derivation.impute";
            let operation = Imputation::new(
                date_kind(D::Position("first")),
                "SRC.TEXT".into(),
                Some("SRC.REF".into()),
                None,
                None,
                path.into(),
                invalid,
            )
            .unwrap();
            let operands = operands(path, &operation);
            plan_with(table, T::Date, operation, operands)
        }
        fn run(
            plan: &DatasetPlan,
            table: &Table,
        ) -> yamaa_core::dataset::ExecutionAttempt<Failure> {
            plan.execute(
                table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            )
        }
        #[test]
        fn complete_sources_and_recovered_values_never_read_the_bound() {
            for source in [
                Value::Str("2024-02-01".into()),
                Value::Missing,
                Value::Str("invalid".into()),
            ] {
                let mut table = make_table(source);
                table.bad_column = 2;
                *table.fail.borrow_mut() = Some(Failure(Box::new(7), Rc::new(())));
                let result = run(&make_plan(&table, Some(Value::Missing)), &table);
                assert!(result.result.is_ok());
                assert_eq!(*table.reads.borrow(), [0, 1]);
                assert!(table.fail.borrow().is_some());
                // A missing source is counted as a default substitution at
                // `missing`, beside the declared `invalid` handler.
                let expected_missing = u64::from(table.source == Value::Missing);
                let expected_invalid = u64::from(table.source == Value::Str("invalid".into()));
                assert_eq!(
                    result
                        .handler_counts
                        .iter()
                        .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                        .collect::<Vec<_>>(),
                    [
                        (
                            "columns.DATE.derivation.impute.missing",
                            HandlerKind::Missing,
                            expected_missing
                        ),
                        (
                            "columns.DATE.derivation.impute.invalid",
                            HandlerKind::Invalid,
                            expected_invalid
                        )
                    ]
                );
            }
        }
        #[test]
        fn opaque_bound_failure_preserves_nonclone_payload_identity_and_is_never_retried() {
            let mut table = make_table(Value::Str("2024-02".into()));
            table.bad_column = 2;
            let marker = Rc::new(());
            let failure = Failure(Box::new(9), marker.clone());
            let address = &*failure.0 as *const u8;
            *table.fail.borrow_mut() = Some(failure);
            let result = run(&make_plan(&table, Some(Value::Missing)), &table);
            let ExecutionError::Cell {
                path,
                error: CellError::Access(Failure(payload, retained_marker)),
                ..
            } = *result.result.unwrap_err()
            else {
                panic!("original source failure")
            };
            assert_eq!(path, "columns.DATE.derivation.impute.not_before");
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&marker, &retained_marker));
            assert_eq!(*table.reads.borrow(), [0, 1, 2]);
            assert!(result.handler_counts.iter().all(|count| count.count == 0));
        }
        #[test]
        fn runtime_impute_failure_retains_current_key_and_is_not_answered_by_invalid_handler() {
            let mut table = make_table(Value::Str("2024-02".into()));
            let path = "columns.DATE.derivation.impute";
            // A real invalid completion cannot be answered by the text-invalid handler.
            let operation = Imputation::new(
                date_kind(D::Number("31")),
                "SRC.TEXT".into(),
                Some("SRC.REF".into()),
                None,
                None,
                path.into(),
                Some(Value::Missing),
            )
            .unwrap();
            let operands = operands(path, &operation);
            let plan = plan_with(&table, T::Date, operation, operands);
            let attempt = run(&plan, &table);
            let ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            } = *attempt.result.unwrap_err()
            else {
                panic!("calendar cause")
            };
            assert_eq!(diagnostic.definition().condition, "invalid_calendar_date");
            assert_eq!(identity.unwrap().values, [Value::Int(1)]);
            // Completion precedes the bounds, so a failed completion never reads them.
            assert_eq!(*table.reads.borrow(), [0, 1]);
            assert!(attempt.handler_counts.iter().all(|count| count.count == 0));
            table.reads.borrow_mut().clear();
            table.bad_column = 2;
            *table.fail.borrow_mut() = Some(Failure(Box::new(6), Rc::new(())));
            assert!(matches!(
                *run(&plan, &table).result.unwrap_err(),
                ExecutionError::SemanticDiagnostic { .. }
            ));
            assert_eq!(*table.reads.borrow(), [0, 1]);
        }

        /// Declare the named bounds, each reading the table's REF date (2024-02-20).
        fn bounded_plan(table: &Table, not_before: bool, not_after: bool) -> DatasetPlan {
            let path = "columns.DATE.derivation.impute";
            let operation = Imputation::new(
                date_kind(D::Position("last")),
                "SRC.TEXT".into(),
                not_before.then(|| "SRC.REF".into()),
                not_after.then(|| "SRC.REF".into()),
                None,
                path.into(),
                None,
            )
            .unwrap();
            let operands = operands(path, &operation);
            plan_with(table, T::Date, operation, operands)
        }
        #[test]
        fn upper_and_paired_bounds_read_their_operands_in_declaration_order() {
            for (text, not_before, not_after, expected, reads) in [
                ("2024-02", false, true, "2024-02-20", &[0, 1, 2][..]),
                ("2024-02", true, true, "2024-02-20", &[0, 1, 2, 2][..]),
                ("2024-01", false, true, "2024-01-31", &[0, 1, 2][..]),
                // A bound outside the collected month is ignored.
                ("2024-03", false, true, "2024-03-31", &[0, 1, 2][..]),
                ("2024-01", true, true, "2024-01-31", &[0, 1, 2, 2][..]),
                ("2024-02-25", false, true, "2024-02-25", &[0, 1][..]),
            ] {
                let table = make_table(Value::Str(text.into()));
                let attempt = run(&bounded_plan(&table, not_before, not_after), &table);
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    [vec![Value::Int(1), Value::Date(expected.parse().unwrap())]],
                    "{text}"
                );
                assert_eq!(*table.reads.borrow(), reads, "{text}");
                // Equal bounds do not cross.
                assert!(attempt
                    .handler_counts
                    .iter()
                    .all(|count| count.handler != HandlerKind::CrossedBounds || count.count == 0));
            }
            let mut table = make_table(Value::Str("2024-02".into()));
            table.bad_column = 2;
            *table.fail.borrow_mut() = Some(Failure(Box::new(4), Rc::new(())));
            let ExecutionError::Cell { path, .. } =
                *run(&bounded_plan(&table, false, true), &table)
                    .result
                    .unwrap_err()
            else {
                panic!("bound read failure")
            };
            assert_eq!(path, "columns.DATE.derivation.impute.not_after");
        }

        #[test]
        fn crossed_bounds_resolve_to_not_after_and_are_counted() {
            let path = "columns.DATE.derivation.impute";
            let table = make_table(Value::Str("2024-02".into()));
            // not_before reads REF (2024-02-20) and not_after is 2024-02-10: both
            // lie inside the collected month and cross, so not_after wins.
            let operation = Imputation::new(
                date_kind(D::Number("15")),
                "SRC.TEXT".into(),
                Some("SRC.REF".into()),
                Some("SRC.EARLY".into()),
                None,
                path.into(),
                None,
            )
            .unwrap();
            let operands = vec![
                Assignment {
                    column: 1,
                    path: format!("{path}.source"),
                    expression: Expression::Source(1),
                },
                Assignment {
                    column: 1,
                    path: format!("{path}.not_before"),
                    expression: Expression::Source(2),
                },
                Assignment {
                    column: 1,
                    path: format!("{path}.not_after"),
                    expression: Expression::Literal(Value::Date("2024-02-10".parse().unwrap())),
                },
            ];
            let attempt = run(&plan_with(&table, T::Date, operation, operands), &table);
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![
                    Value::Int(1),
                    Value::Date("2024-02-10".parse().unwrap())
                ]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "columns.DATE.derivation.impute.missing",
                        HandlerKind::Missing,
                        0
                    ),
                    (
                        "columns.DATE.derivation.impute.not_after",
                        HandlerKind::CrossedBounds,
                        1
                    )
                ]
            );
        }

        #[test]
        fn a_fallback_is_read_only_for_a_missing_source_and_counted() {
            let path = "columns.DATE.derivation.impute";
            for (source, expected, reads, fired) in [
                (Value::Missing, "2024-02-20", &[0, 1, 2][..], 1),
                (Value::Str("2024-03".into()), "2024-03-01", &[0, 1][..], 0),
            ] {
                let table = make_table(source);
                let operation = Imputation::new(
                    date_kind(D::Position("first")),
                    "SRC.TEXT".into(),
                    None,
                    None,
                    Some("SRC.REF".into()),
                    path.into(),
                    None,
                )
                .unwrap();
                let operands = operands(path, &operation);
                let attempt = run(&plan_with(&table, T::Date, operation, operands), &table);
                let rows = attempt.result.unwrap().dataset.rows().to_vec();
                assert_eq!(
                    rows,
                    [vec![Value::Int(1), Value::Date(expected.parse().unwrap())]]
                );
                let Value::Date(date) = rows[0][1] else {
                    panic!("date")
                };
                assert_eq!(
                    date.collected_precision(),
                    if fired == 1 { P::None } else { P::Month }
                );
                assert_eq!(*table.reads.borrow(), reads);
                assert_eq!(
                    attempt
                        .handler_counts
                        .iter()
                        .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                        .collect::<Vec<_>>(),
                    [(
                        "columns.DATE.derivation.impute.fallback",
                        HandlerKind::Missing,
                        fired
                    )]
                );
            }
        }

        #[test]
        fn recovered_datetime_literal_is_converted_once_and_its_handler_is_counted() {
            let table = make_table(Value::Str("bad".into()));
            let operation = Imputation::new(
                K::Impute(
                    completion_policy(
                        Target::DateTime,
                        None,
                        None,
                        P::Day,
                        Some(yamaa_core::dataset::TimeRule::First),
                        "impute",
                    )
                    .unwrap(),
                ),
                "SRC.TEXT".into(),
                None,
                None,
                None,
                "impute".into(),
                Some(Value::Str("2024-02-29T12:13".into())),
            )
            .unwrap();
            let plan = plan_with(
                &table,
                T::DateTime,
                operation,
                vec![Assignment {
                    column: 1,
                    path: "impute.source".into(),
                    expression: Expression::Source(1),
                }],
            );
            let attempt = run(&plan, &table);
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [vec![
                    Value::Int(1),
                    Value::DateTime("2024-02-29T12:13:00".parse().unwrap())
                ]]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.spec_path.as_str(), count.handler, count.count))
                    .collect::<Vec<_>>(),
                [
                    ("impute.missing", HandlerKind::Missing, 0),
                    ("impute.invalid", HandlerKind::Invalid, 1)
                ]
            );
            assert_eq!(*table.reads.borrow(), [0, 1]);
        }
    }
}
