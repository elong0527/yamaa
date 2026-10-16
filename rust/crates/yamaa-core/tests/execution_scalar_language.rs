//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod scalar_language {
        use std::{cell::RefCell, convert::Infallible};
        use yamaa_core::numeric_lifecycle::HandlerKind;
        use yamaa_core::{
            dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        struct Table {
            schema: TableSchema,
            values: Vec<Value>,
            reads: RefCell<Vec<usize>>,
        }
        impl TableAccess for Table {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                1
            }
            fn cell(
                &self,
                _row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                self.reads.borrow_mut().push(column);
                Ok((&self.values[column]).into())
            }
        }
        fn schema(names: &[&str]) -> TableSchema {
            TableSchema::new(
                names
                    .iter()
                    .map(|&name| Column {
                        name: name.into(),
                        kind: ColumnType::Str,
                    })
                    .collect(),
            )
            .unwrap()
        }
        #[test]
        fn missing_concat_short_circuits_source_reads_and_retains_nested_handler_observations() {
            let source = schema(&["ID", "FIRST", "LAST"]);
            let table = Table {
                schema: source.clone(),
                values: vec![
                    Value::Str("a".into()),
                    Value::Missing,
                    Value::Str("unread".into()),
                ],
                reads: RefCell::default(),
            };
            let path = "columns.RESULT.derivation.str_concat";
            let expression = Expression::StrConcat {
                operands: vec![
                    Assignment {
                        column: 1,
                        path: format!("{path}.sources[0]"),
                        expression: Expression::StrConcat {
                            operands: vec![Assignment {
                                column: 1,
                                path: format!("{path}.sources[0].str_concat.sources[0]"),
                                expression: Expression::Source(1),
                            }],
                            missing: Some(Value::Missing),
                            path: format!("{path}.sources[0].str_concat"),
                        },
                    },
                    Assignment {
                        column: 1,
                        path: format!("{path}.sources[1]"),
                        expression: Expression::Source(2),
                    },
                ],
                missing: Some(Value::Str("fallback".into())),
                path: path.into(),
            };
            let plan = DatasetPlan::new(
                source,
                schema(&["ID", "RESULT"]),
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
                            path: path.into(),
                            expression,
                        },
                    ],
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
                [vec![Value::Str("a".into()), Value::Str("fallback".into())]]
            );
            assert_eq!(*table.reads.borrow(), [0, 1]);
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|count| (count.handler, count.count))
                    .collect::<Vec<_>>(),
                [(HandlerKind::Missing, 1), (HandlerKind::Missing, 1)]
            );
        }
        #[test]
        fn equal_extremes_retain_the_first_dates_collected_precision() {
            use yamaa_core::temporal::{Date, DatePrecision};
            let source = schema(&["ID"]);
            let table = Table {
                schema: source.clone(),
                values: vec![Value::Str("a".into())],
                reads: RefCell::default(),
            };
            let output = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Str,
                },
                Column {
                    name: "RESULT".into(),
                    kind: ColumnType::Date,
                },
            ])
            .unwrap();
            for largest in [true, false] {
                let operands = [DatePrecision::Month, DatePrecision::Day]
                    .into_iter()
                    .enumerate()
                    .map(|(index, precision)| Assignment {
                        column: 1,
                        path: format!("columns.RESULT.derivation.greatest.sources[{index}]"),
                        expression: Expression::Literal(Value::Date(
                            Date::new(2025, 1, 12, precision).unwrap(),
                        )),
                    })
                    .collect();
                let plan = DatasetPlan::new(
                    source.clone(),
                    output.clone(),
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
                                path: "columns.RESULT.derivation.greatest".into(),
                                expression: Expression::Extreme { operands, largest },
                            },
                        ],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap();
                let result = plan
                    .execute(
                        &table,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap();
                let Value::Date(date) = result.dataset.rows()[0][1] else {
                    panic!("date result")
                };
                assert_eq!(date.collected_precision(), DatePrecision::Month);
            }
        }

        #[test]
        fn equal_numeric_extremes_keep_first_present_signed_zero_and_collect_every_operand() {
            let source = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Str,
                },
                Column {
                    name: "INTEGER".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "FLOAT".into(),
                    kind: ColumnType::Float,
                },
                Column {
                    name: "MISSING".into(),
                    kind: ColumnType::Float,
                },
            ])
            .unwrap();
            let output = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Str,
                },
                Column {
                    name: "RESULT".into(),
                    kind: ColumnType::Float,
                },
            ])
            .unwrap();
            for largest in [true, false] {
                for (columns, expected) in [([3, 1, 2], 0.0_f64), ([3, 2, 1], -0.0_f64)] {
                    let table = Table {
                        schema: source.clone(),
                        values: vec![
                            Value::Str("a".into()),
                            Value::Int(0),
                            Value::float(-0.0),
                            Value::Missing,
                        ],
                        reads: RefCell::default(),
                    };
                    let operation = if largest { "greatest" } else { "least" };
                    let path = format!("columns.RESULT.derivation.{operation}");
                    let operands = columns
                        .into_iter()
                        .enumerate()
                        .map(|(index, column)| Assignment {
                            column: 1,
                            path: format!("{path}.sources[{index}]"),
                            expression: Expression::Source(column),
                        })
                        .collect();
                    let plan = DatasetPlan::new(
                        source.clone(),
                        output.clone(),
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
                                    path,
                                    expression: Expression::Extreme { operands, largest },
                                },
                            ],
                            filter: None,
                        }],
                        vec![],
                        vec![0],
                        vec![],
                    )
                    .unwrap();
                    let result = plan
                        .execute(
                            &table,
                            &[],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None,
                        )
                        .result
                        .unwrap();
                    let Value::Float(value) = result.dataset.rows()[0][1] else {
                        panic!("float output")
                    };
                    assert_eq!(
                        value.get().to_bits(),
                        expected.to_bits(),
                        "{operation} {columns:?}"
                    );
                    assert_eq!(
                        *table.reads.borrow(),
                        [0, columns[0], columns[1], columns[2]]
                    );
                }
            }
        }
    }
    mod self_relations {
        //! SELF selection reads only borrowed completed donors; opaque source prefix is exact.
        use crate::support::table_schema as schema;
        use std::cell::RefCell;
        use yamaa_core::{
            dataset::{
                Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey,
                IntermediateSource, Keep, MatchRead, MatchValue, OrderTerm, RowMode, RowTemplate,
                SecondarySource, SourceSchemas, SourceSelection,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            reads: RefCell<Vec<(usize, usize)>>,
        }
        impl TableAccess for Table {
            type Error = std::convert::Infallible;
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
                Ok((&self.rows[row][column]).into())
            }
        }
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("read.C{column}"),
            }
        }
        #[test]
        fn completed_grouped_and_stored_driver_templates_add_no_study_reads() {
            let primary = Table {
                schema: schema(&[
                    ("ID", T::Int),
                    ("K", T::Int),
                    ("SEQ", T::Int),
                    ("V", T::Str),
                ]),
                rows: vec![
                    vec![
                        Value::Int(1),
                        Value::Int(7),
                        Value::Int(2),
                        Value::Str("chosen Ω".into()),
                    ],
                    vec![
                        Value::Int(2),
                        Value::Int(7),
                        Value::Int(1),
                        Value::Str("other".into()),
                    ],
                ],
                reads: RefCell::default(),
            };
            let other = Table {
                schema: schema(&[("ID", T::Int), ("K", T::Int)]),
                rows: vec![
                    vec![Value::Int(4), Value::Int(7)],
                    vec![Value::Int(5), Value::Int(7)],
                ],
                reads: RefCell::default(),
            };
            let output = schema(&[
                ("ID", T::Int),
                ("K", T::Int),
                ("SEQ", T::Int),
                ("V", T::Str),
                ("FINAL", T::Str),
            ]);
            let item = Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::SelfRows {
                    schema: output.clone(),
                    columns: vec![0, 1, 2, 3],
                },
                keys: vec![IntermediateKey {
                    source_column: 1,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 1,
                        identifier: "K".into(),
                    }),
                }],
                filter: None,
                selection: Some(SourceSelection {
                    order_by: vec![OrderTerm {
                        column: 2,
                        descending: false,
                        nulls_first: false,
                    }],
                    keep: Keep::Last,
                }),
                no_match: None,
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,

                range: None,
                columns: None,
            };
            let read = || Expression::Intermediate {
                index: 0,
                column: 3,
            };
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: primary.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "OTHER".into(),
                        schema: other.schema.clone(),
                    }],
                },
                vec![item],
                output,
                vec![
                    RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1)),
                            assign(2, Expression::Source(2)),
                            assign(3, Expression::Source(3)),
                        ],
                        filter: None,
                    },
                    RowTemplate {
                        mode: RowMode::Groups(vec![1]),
                        assignments: vec![
                            assign(0, Expression::Literal(Value::Int(3))),
                            assign(1, Expression::Source(1)),
                            assign(2, Expression::Literal(Value::Int(8))),
                            assign(3, read()),
                        ],
                        filter: None,
                    },
                    RowTemplate {
                        mode: RowMode::Driver {
                            source: 1,
                            mode: Box::new(RowMode::Records),
                        },
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1)),
                            assign(2, Expression::Literal(Value::Int(9))),
                            assign(3, read()),
                        ],
                        filter: None,
                    },
                ],
                vec![assign(4, read())],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &primary,
                &[&other],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let result = attempt.result.unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[
                    vec![
                        Value::Int(1),
                        Value::Int(7),
                        Value::Int(2),
                        Value::Str("chosen Ω".into()),
                        Value::Str("chosen Ω".into())
                    ],
                    vec![
                        Value::Int(2),
                        Value::Int(7),
                        Value::Int(1),
                        Value::Str("other".into()),
                        Value::Str("chosen Ω".into())
                    ],
                    vec![
                        Value::Int(3),
                        Value::Int(7),
                        Value::Int(8),
                        Value::Str("chosen Ω".into()),
                        Value::Str("chosen Ω".into())
                    ],
                    vec![
                        Value::Int(4),
                        Value::Int(7),
                        Value::Int(9),
                        Value::Str("chosen Ω".into()),
                        Value::Str("chosen Ω".into())
                    ],
                    vec![
                        Value::Int(5),
                        Value::Int(7),
                        Value::Int(9),
                        Value::Str("chosen Ω".into()),
                        Value::Str("chosen Ω".into())
                    ],
                ]
            );
            assert_eq!(
                *primary.reads.borrow(),
                vec![
                    (0, 0),
                    (0, 1),
                    (0, 2),
                    (0, 3),
                    (1, 0),
                    (1, 1),
                    (1, 2),
                    (1, 3),
                    (0, 1),
                    (1, 1),
                    (0, 1)
                ]
            );
            assert_eq!(*other.reads.borrow(), vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        }
    }
    mod source_diagnostics {
        //! Actual capture/decoder ordering with payloads that are neither Clone nor Debug nor Send.
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_core::json::Value as V;
        use yamaa_core::{
            application_observer::{Noop, Scope},
            function_invocation::NoFunctions,
            specification_run::{
                self as run, CapturedAttempt, PortError, RunError, SourceDecoder,
                SpecificationSourcePort,
            },
        };
        use yamaa_core::{
            diagnostic::{source_diagnostics, ConditionCode as C, Diagnostic},
            schema::{Document, SpecificationDocument},
            specification::{PreparedSpecification, SourceDeclaration},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            typed_csv::Error as TypingError,
            value::ColumnType,
        };

        use crate::support::document::Tree;

        fn prepared(names: &[&str]) -> PreparedSpecification {
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                ("base", Tree::text("SRC")),
                (
                    "input",
                    Tree::map(
                        names
                            .iter()
                            .map(|&name| (name, Tree::map(vec![("path", Tree::text("held.csv"))])))
                            .collect(),
                    ),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![Tree::map(vec![
                        ("name", Tree::text("ID")),
                        ("type", Tree::text("int")),
                        (
                            "derivation",
                            Tree::map(vec![(
                                "value",
                                Tree::map(vec![(
                                    "source",
                                    Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                                )]),
                            )]),
                        ),
                    ])]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        ("columns", Tree::List(vec![Tree::text("ID")])),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            let model = SpecificationDocument::admit(document).unwrap().unwrap();
            PreparedSpecification::prepare(&model).unwrap()
        }
        type Trace = Rc<RefCell<Vec<String>>>;
        struct Opaque {
            marker: Rc<()>,
            cause: Option<TypingError>,
            name: &'static str,
        }
        struct NoTable;
        impl TableAccess for NoTable {
            type Error = Opaque;
            fn schema(&self) -> &TableSchema {
                panic!("failed decoder must not bind a table")
            }
            fn row_count(&self) -> usize {
                panic!("failed decoder must not inspect table rows")
            }
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Opaque>> {
                panic!("failed decoder must not read study cells")
            }
        }
        struct Port {
            trace: Trace,
            held: Vec<Arc<[u8]>>,
            reads: usize,
            marker: Rc<()>,
            fail_at: Option<&'static str>,
        }
        impl SpecificationSourcePort for Port {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = Opaque;
            fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Opaque> {
                self.trace
                    .borrow_mut()
                    .push(format!("inspect:{}", source.name));
                Ok(())
            }
            fn capture_reads(&self) -> usize {
                self.reads
            }
            fn capture(
                &mut self,
                source: &SourceDeclaration,
                origin: &mut Option<run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Opaque> {
                *origin = Some(run::SourceOrigin::Resource);
                self.trace
                    .borrow_mut()
                    .push(format!("capture:{}", source.name));
                if self.fail_at == Some(source.name.as_str()) {
                    return Err(Opaque {
                        marker: Rc::clone(&self.marker),
                        cause: None,
                        name: "SECOND",
                    });
                }
                let bytes = Arc::from(source.name.as_bytes());
                self.held.push(Arc::clone(&bytes));
                self.reads += 1;
                Ok(bytes)
            }
        }
        struct Decoder {
            trace: Trace,
            pending: Vec<Option<Opaque>>,
            next: usize,
        }
        impl SourceDecoder for Decoder {
            type Error = Opaque;
            type Table = NoTable;
            fn continue_after(&self, error: &Opaque) -> bool {
                self.trace
                    .borrow_mut()
                    .push(format!("classify:{}", error.name));
                error.cause.is_some()
            }
            fn decode(
                &mut self,
                source: &SourceDeclaration,
                bytes: &[u8],
                _interpretation: yamaa_core::specification::SourceInterpretation,
                _contract: Option<&yamaa_core::producer_contract::Contract>,
            ) -> Result<NoTable, Opaque> {
                self.trace
                    .borrow_mut()
                    .push(format!("decode:{}", source.name));
                assert_eq!(bytes, source.name.as_bytes());
                let error = self.pending[self.next]
                    .take()
                    .expect("each original allocation moves once");
                self.next += 1;
                Err(error)
            }
        }
        fn fixtures(opaque_third: bool) -> (Port, Decoder, Rc<()>, *const u8) {
            let trace = Trace::default();
            let marker = Rc::new(());
            let third = "\0\"\\β".repeat(2000);
            let pointer = third.as_ptr();
            let pending = vec![
                Some(Opaque {
                    marker: Rc::clone(&marker),
                    name: "SRC",
                    cause: Some(TypingError::UnknownField {
                        field: "MISSING".into(),
                    }),
                }),
                Some(Opaque {
                    marker: Rc::clone(&marker),
                    name: "SECOND",
                    cause: Some(TypingError::FieldParse {
                        field: "B".into(),
                        target: ColumnType::Int,
                        value: "bad".into(),
                    }),
                }),
                Some(Opaque {
                    marker: Rc::clone(&marker),
                    name: "THIRD",
                    cause: if opaque_third {
                        None
                    } else {
                        Some(TypingError::FieldParse {
                            field: "F".into(),
                            target: ColumnType::Int,
                            value: third,
                        })
                    },
                }),
            ];
            (
                Port {
                    trace: Rc::clone(&trace),
                    held: vec![],
                    reads: 0,
                    marker: Rc::clone(&marker),
                    fail_at: None,
                },
                Decoder {
                    trace,
                    pending,
                    next: 0,
                },
                marker,
                pointer,
            )
        }
        fn text(value: &str) -> V {
            V::String(value.into())
        }
        fn expected(code: C, path: &str, context: Vec<(&str, V)>) -> Diagnostic {
            Diagnostic {
                code,
                spec_paths: vec![path.into()],
                context: context
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect(),
                source_span: None,
                operand_route: None,
            }
        }

        #[test]
        fn failed_run_keeps_complete_order_all_original_allocations_and_effects_through_projection()
        {
            let prepared = prepared(&["SRC", "SECOND", "THIRD"]);
            let (mut port, mut decoder, marker, pointer) = fixtures(false);
            let mut attempt = CapturedAttempt::new(prepared.source());
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut NoFunctions::new(),
                &mut attempt,
                Scope::new(&Noop),
            );
            let Err(PortError::Run(RunError::Sources(errors))) = &attempt.result else {
                panic!("expected held decoder failures")
            };
            assert_eq!(
                errors.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
                [0, 1, 2]
            );
            for ((_, error), captured) in errors.iter().zip(&attempt.sources) {
                assert!(Rc::ptr_eq(&error.marker, &marker));
                assert!(captured.table.is_none());
                assert!(captured.read.captured);
                assert_eq!(captured.read.snapshots_created, Some(1));
            }
            for (captured, held) in attempt.sources.iter().zip(&port.held) {
                assert!(Arc::ptr_eq(captured.snapshot.as_ref().unwrap(), held));
            }
            let Some(TypingError::FieldParse { value, .. }) = &errors[2].1.cause else {
                panic!()
            };
            assert_eq!(value.as_ptr(), pointer);
            let trace = port.trace.borrow().clone();
            assert_eq!(
                trace,
                [
                    "inspect:SRC",
                    "inspect:SECOND",
                    "inspect:THIRD",
                    "capture:SRC",
                    "decode:SRC",
                    "classify:SRC",
                    "capture:SECOND",
                    "decode:SECOND",
                    "classify:SECOND",
                    "capture:THIRD",
                    "decode:THIRD",
                    "classify:THIRD"
                ]
            );
            let findings: Vec<_> = errors
                .iter()
                .map(|(index, error)| {
                    error
                        .cause
                        .as_ref()
                        .unwrap()
                        .finding(&prepared.sources()[*index].name)
                        .unwrap()
                })
                .collect();
            let actual: Vec<_> = source_diagnostics(&findings).collect();
            assert_eq!(
                actual,
                vec![
                    expected(
                        C::SourceTypeUnknownField,
                        "input.SRC.types.MISSING",
                        vec![("dataset", text("SRC")), ("field", text("MISSING"))]
                    ),
                    expected(
                        C::SourceTypeFieldParse,
                        "input.SECOND.types.B",
                        vec![
                            ("dataset", text("SECOND")),
                            ("field", text("B")),
                            ("type", text("int")),
                            ("value", text("bad"))
                        ]
                    ),
                    expected(
                        C::SourceTypeFieldParse,
                        "input.THIRD.types.F",
                        vec![
                            ("dataset", text("THIRD")),
                            ("field", text("F")),
                            ("type", text("int")),
                            ("value", text(value))
                        ]
                    ),
                ]
            );
            assert_eq!(source_diagnostics(&findings).collect::<Vec<_>>(), actual);
            assert_eq!(value.as_ptr(), pointer);
            assert_eq!(*port.trace.borrow(), trace);
            assert_eq!(port.reads, 3);
        }

        #[test]
        fn original_opaque_decoder_payload_aborts_after_completed_prefix_before_later_capture() {
            let prepared = prepared(&["SRC", "SECOND", "THIRD", "NEVER"]);
            let (mut port, mut decoder, marker, _) = fixtures(true);
            let mut attempt = CapturedAttempt::new(prepared.source());
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut NoFunctions::new(),
                &mut attempt,
                Scope::new(&Noop),
            );
            let Err(PortError::Run(RunError::Sources(errors))) = &attempt.result else {
                panic!("expected opaque decoder error")
            };
            assert_eq!(errors.len(), 3);
            assert_eq!(attempt.sources.len(), 3);
            assert!(errors[2].1.cause.is_none());
            assert!(Rc::ptr_eq(&errors[2].1.marker, &marker));
            assert!(errors[..2].iter().all(|(_, error)| error.cause.is_some()));
            assert_eq!(
                port.trace.borrow().as_slice(),
                [
                    "inspect:SRC",
                    "inspect:SECOND",
                    "inspect:THIRD",
                    "inspect:NEVER",
                    "capture:SRC",
                    "decode:SRC",
                    "classify:SRC",
                    "capture:SECOND",
                    "decode:SECOND",
                    "classify:SECOND",
                    "capture:THIRD",
                    "decode:THIRD",
                    "classify:THIRD"
                ]
            );
            let before = Rc::strong_count(&marker);
            for _ in 0..2 {
                assert!(errors.iter().any(|(_, error)| error.cause.is_none()));
            }
            assert_eq!(Rc::strong_count(&marker), before);
            assert_eq!(port.reads, 3);
        }

        #[test]
        fn original_opaque_capture_payload_retains_identity_without_decode_or_projection() {
            let prepared = prepared(&["SRC", "SECOND", "THIRD"]);
            let (mut port, mut decoder, marker, _) = fixtures(false);
            port.fail_at = Some("SECOND");
            let mut attempt = CapturedAttempt::new(prepared.source());
            run::execute_into(
                &prepared,
                &mut port,
                &mut decoder,
                &mut NoFunctions::new(),
                &mut attempt,
                Scope::new(&Noop),
            );
            let Err(PortError::Capture(error)) = &attempt.result else {
                panic!("expected original capture error")
            };
            assert!(Rc::ptr_eq(&error.marker, &marker));
            assert!(error.cause.is_none());
            assert_eq!(attempt.sources.len(), 2);
            assert!(Arc::ptr_eq(
                attempt.sources[0].snapshot.as_ref().unwrap(),
                &port.held[0]
            ));
            assert!(attempt.sources[1].snapshot.is_none());
            assert!(!attempt.sources[1].read.captured);
            assert_eq!(decoder.next, 1);
            assert_eq!(
                port.trace.borrow().as_slice(),
                [
                    "inspect:SRC",
                    "inspect:SECOND",
                    "inspect:THIRD",
                    "capture:SRC",
                    "decode:SRC",
                    "classify:SRC",
                    "capture:SECOND"
                ]
            );
        }
    }
    mod table_reduction {
        use std::cell::RefCell;
        use yamaa_core::table_reduction::{count_selected, reduce_column, TableReductionError};
        use yamaa_core::{
            numeric::{ArithmeticErrorKind, Number},
            reduction::{NumericReducer, ReductionError},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        /// Record counts never read cells; field counts visit every selected value in order.
        #[test]
        fn count_selection_preserves_access_and_absence_semantics() {
            let mut table = Table::new(vec![Value::Missing, Value::Str("".into()), Value::Missing]);
            assert_eq!(count_selected(&table, None, &[0, 1, 2]), Ok(Number::Int(3)));
            assert!(table.reads.borrow().is_empty());
            assert_eq!(
                count_selected(&table, Some(0), &[0, 1, 2]),
                Ok(Number::Int(1))
            );
            assert_eq!(*table.reads.borrow(), [0, 1, 2]);
            table.reads.borrow_mut().clear();
            assert_eq!(count_selected(&table, Some(0), &[0, 2]), Ok(Number::Int(0)));
            assert_eq!(*table.reads.borrow(), [0, 2]);
            table.reads.borrow_mut().clear();
            for column in [None, Some(0)] {
                assert_eq!(count_selected(&table, column, &[]), Ok(Number::Missing));
            }
            assert!(table.reads.borrow().is_empty());
            table.fail_at = Some(2);
            assert_eq!(
                count_selected(&table, Some(0), &[0, 1, 2]),
                Err(TableReductionError::Cell {
                    position: 2,
                    error: CellError::Access(PortError("read failed"))
                })
            );
            assert_eq!(*table.reads.borrow(), [0, 1, 2]);
        }

        /// Both count forms validate complete selections before touching any source value.
        #[test]
        fn count_coordinates_precede_reads() {
            let table = Table::new(vec![Value::Missing]);
            for column in [None, Some(0)] {
                assert_eq!(
                    count_selected(&table, column, &[1]),
                    Err(TableReductionError::InvalidRow {
                        position: 0,
                        row: 1
                    })
                );
                assert_eq!(
                    count_selected(&table, column, &[0, 0]),
                    Err(TableReductionError::UnorderedSelection { position: 1 })
                );
            }
            assert_eq!(
                count_selected(&table, Some(1), &[]),
                Err(TableReductionError::InvalidColumn { column: 1 })
            );
            assert!(table.reads.borrow().is_empty());
            let mut fieldless = Table::new(vec![Value::Missing]);
            fieldless.schema = TableSchema::new(vec![]).unwrap();
            assert_eq!(count_selected(&fieldless, None, &[0]), Ok(Number::Int(1)));
            assert!(fieldless.reads.borrow().is_empty());
        }

        // Deliberately not Clone: storage errors must remain opaque and movable.
        #[derive(Debug, PartialEq, Eq)]
        struct PortError(&'static str);
        struct Table {
            schema: TableSchema,
            values: Vec<Value>,
            fail_at: Option<usize>,
            reads: RefCell<Vec<usize>>,
        }
        impl Table {
            /// Construct a traced fake snapshot with one declared column.
            fn new(values: Vec<Value>) -> Self {
                Self {
                    schema: TableSchema::new(vec![Column {
                        name: "A".into(),
                        kind: ColumnType::Int,
                    }])
                    .unwrap(),
                    values,
                    fail_at: None,
                    reads: RefCell::new(vec![]),
                }
            }
        }
        impl TableAccess for Table {
            type Error = PortError;
            /// Return the fake schema without reading any cell.
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            /// Return fake snapshot size without touching the read trace.
            fn row_count(&self) -> usize {
                self.values.len()
            }
            /// Record valid access and inject an opaque storage failure when requested.
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                if row >= self.row_count() || column >= self.schema.columns().len() {
                    return Err(CellError::OutOfBounds { row, column });
                }
                self.reads.borrow_mut().push(row);
                if self.fail_at == Some(row) {
                    return Err(CellError::Access(PortError("read failed")));
                }
                Ok((&self.values[row]).into())
            }
        }

        #[test]
        /// Later argument access failures win over potential earlier fold failures.
        fn read_all_arguments_before_folding_and_preserve_opaque_error() {
            for first in [Value::Int(i64::MAX), Value::Str("bad".into())] {
                let mut table =
                    Table::new(vec![first, Value::Int(1), Value::Missing, Value::Int(0)]);
                table.fail_at = Some(2);
                assert_eq!(
                    reduce_column(&table, 0, &[0, 1, 2, 3], NumericReducer::Sum, "SUM(A)"),
                    Err(TableReductionError::Cell {
                        position: 2,
                        error: CellError::Access(PortError("read failed"))
                    })
                );
                assert_eq!(*table.reads.borrow(), [0, 1, 2]);
            }
        }

        #[test]
        /// Once collection succeeds, fold failures retain their own written order.
        fn arithmetic_failure_precedes_later_type_failure_after_all_reads() {
            // Mixed values intentionally exercise the consumer's defensive type checks;
            // an actual typed storage adapter validates its schema at ingestion.
            let table = Table::new(vec![
                Value::Int(i64::MAX),
                Value::Int(1),
                Value::Str("bad".into()),
            ]);
            let result = reduce_column(&table, 0, &[0, 1, 2], NumericReducer::Sum, "SUM(A)");
            let Err(TableReductionError::Reduction(ReductionError::Arithmetic { argument, error })) =
                result
            else {
                panic!("expected overflow")
            };
            assert_eq!(argument, 1);
            assert_eq!(
                error.kind,
                ArithmeticErrorKind::IntegerOverflow {
                    value: i64::MAX as i128 + 1
                }
            );
            assert_eq!(*table.reads.borrow(), [0, 1, 2]);
        }

        #[test]
        /// Bad selection coordinates fail before any storage observation.
        fn preflight_coordinates_do_not_read_cells() {
            let table = Table::new(vec![Value::Int(1), Value::Int(2)]);
            let reduce = |column, rows: &[usize]| {
                reduce_column(&table, column, rows, NumericReducer::Sum, "SUM(A)")
            };
            assert_eq!(
                reduce(1, &[]),
                Err(TableReductionError::InvalidColumn { column: 1 })
            );
            assert_eq!(
                reduce(0, &[0, 2]),
                Err(TableReductionError::InvalidRow {
                    position: 1,
                    row: 2
                })
            );
            for rows in [[1, 0], [0, 0]] {
                assert_eq!(
                    reduce(0, &rows),
                    Err(TableReductionError::UnorderedSelection { position: 1 })
                );
            }
            assert!(table.reads.borrow().is_empty());
            assert_eq!(
                table.cell(0, 1),
                Err(CellError::OutOfBounds { row: 0, column: 1 })
            );
        }

        #[test]
        /// Empty and filtered relations retain schema and never read excluded rows.
        fn empty_and_filtered_relations_preserve_schema_missing_and_order() {
            let empty = Table::new(vec![]);
            assert_eq!(empty.schema().columns()[0].name, "A");
            assert_eq!(empty.schema().columns()[0].kind, ColumnType::Int);
            assert_eq!(
                reduce_column(&empty, 0, &[], NumericReducer::Mean, "MEAN(A)"),
                Ok(Number::Missing)
            );
            assert_eq!(
                empty.cell(0, 0),
                Err(CellError::OutOfBounds { row: 0, column: 0 })
            );
            let table = Table::new(vec![
                Value::Int(2),
                Value::Str("excluded".into()),
                Value::Missing,
                Value::Int(4),
            ]);
            assert_eq!(
                reduce_column(&table, 0, &[0, 2, 3], NumericReducer::Mean, "MEAN(A)"),
                Ok(Number::float(3.0))
            );
            assert_eq!(*table.reads.borrow(), [0, 2, 3]);
            assert_eq!(table.cell(2, 0), Ok(ValueRef::Missing));
        }
    }
}
