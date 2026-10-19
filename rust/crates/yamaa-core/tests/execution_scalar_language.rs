//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
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
