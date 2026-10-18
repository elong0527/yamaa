//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod case_predicates {
        //! Independent grain, ordering and original opaque-error controls for case predicates.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::{
            Assignment, CaseBranch, Check, ColumnVerifications, ConversionHandler, DatasetPlan,
            ExecutionError, Expression, RowMode, RowTemplate, Verification,
        };
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            expression::{compile, Context},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
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
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
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
                compile(
                    "SRC.FLAG > 0",
                    Context::Predicate,
                    "columns.V.derivation.case[0].when",
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
                                        compile(
                                            "FALSE",
                                            Context::Predicate,
                                            "columns.V.derivation.case[1].when",
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
        #[test]
        fn case_collects_distinct_present_values_and_skips_unselected_branch_reads() {
            let source = source();
            let attempt = plan(&source, predicate()).execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let attempt = plan(&source, predicate()).execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
            let attempt = plan(&source, predicate()).execute(
                &source,
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
                compile(
                    "FALSE AND SRC.BAD = 'bad'",
                    Context::Predicate,
                    "columns.V.derivation.case[0].when",
                )
                .unwrap(),
                vec![Binding {
                    name: "SRC.BAD".into(),
                    read: Read::Source(2),
                }],
                vec!["SRC.BAD".into()],
            )
            .unwrap();
            let attempt = plan(&source, when).execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
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
    }
    mod completed_lookup_assertions {
        //! Same-attempt selected assertions: actual effects, eager order and opaque ownership.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            dataset::{
                Assignment, BoundFunction, Check, DatasetPlan, Expression, Intermediate,
                IntermediateDerivations, IntermediateKey, IntermediateSource, MatchRead,
                MatchValue, RowMode, RowTemplate, SecondarySource, SourceSchemas, Verification,
            },
            expression::{compile, Context},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        use yamaa_core::{
            dataset::ExecutionError,
            function_invocation::{Argument, FunctionPort, HostError},
            function_signature::{FunctionIdentity, InvocationPlan},
        };

        #[derive(Debug)]
        struct Failure(Box<u8>, Rc<()>);
        type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
        struct Table {
            name: &'static str,
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            trace: Trace,
            fail: RefCell<Option<(usize, usize, Failure)>>,
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
                    .is_some_and(|(r, c, _)| (*r, *c) == (row, column))
                {
                    return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().2));
                }
                Ok((&self.rows[row][column]).into())
            }
        }
        fn tables() -> (Table, Table, Trace) {
            let trace = Rc::new(RefCell::new(Vec::new()));
            (
                Table {
                    name: "SRC",
                    schema: schema(&[("ID", T::Int)]),
                    rows: vec![vec![Value::Int(1)]],
                    trace: trace.clone(),
                    fail: RefCell::default(),
                },
                Table {
                    name: "DONOR",
                    schema: schema(&[("ID", T::Int), ("X", T::Int), ("Z", T::Int)]),
                    rows: vec![vec![Value::Int(1), Value::Int(2), Value::Int(3)]],
                    trace: trace.clone(),
                    fail: RefCell::default(),
                },
                trace,
            )
        }
        fn assignment(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("columns.C{column}.derivation"),
            }
        }
        fn predicate(text: &str, part: &str) -> BoundPredicate {
            let compiled = compile(
                text,
                Context::Predicate,
                &format!("verifications[1].assert.{part}"),
            )
            .unwrap();
            let bindings = compiled
                .identifiers()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(|name| Binding {
                    name: name.into(),
                    read: match name {
                        "P.X" => Read::Intermediate {
                            index: 0,
                            column: 1,
                        },
                        "P.Z" => Read::Intermediate {
                            index: 0,
                            column: 2,
                        },
                        "VALUE" => Read::Column(1),
                        _ => panic!("authored name"),
                    },
                })
                .collect();
            BoundPredicate::new_contextual(compiled, bindings, vec![]).unwrap()
        }
        fn plan(
            src: &Table,
            donor: &Table,
            when: Option<&str>,
            require: &str,
            construction_lookup: bool,
            no_match: bool,
        ) -> DatasetPlan {
            build_plan(
                src,
                donor,
                when,
                require,
                construction_lookup,
                no_match,
                None,
            )
        }
        fn build_plan(
            src: &Table,
            donor: &Table,
            when: Option<&str>,
            require: &str,
            construction_lookup: bool,
            no_match: bool,
            derived: Option<IntermediateDerivations>,
        ) -> DatasetPlan {
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
                        value: MatchValue::Variable(MatchRead::Output {
                            column: 0,
                            identifier: "ID".into(),
                        }),
                    }],
                    filter: None,
                    selection: None,
                    no_match: no_match.then_some(Value::Missing),
                    require_match: !no_match,
                    unique: vec![],
                    derivations: derived,
                    pending_schema: None,
                    range: None,
                    columns: None,
                }],
                schema(&[("ID", T::Int), ("VALUE", T::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assignment(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![assignment(
                    1,
                    if construction_lookup {
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        }
                    } else {
                        Expression::Literal(Value::Int(2))
                    },
                )],
                vec![0],
                vec![
                    Verification {
                        path: "verifications[0].unique".into(),
                        check: Check::Unique(vec![0]),
                    },
                    Verification {
                        path: "verifications[1].assert".into(),
                        check: Check::Assert {
                            when: when.map(|text| predicate(text, "when")),
                            require: predicate(require, "require"),
                        },
                    },
                    Verification {
                        path: "verifications[2].row_count".into(),
                        check: Check::RowCount {
                            min: Some(0),
                            max: None,
                        },
                    },
                ],
            )
            .unwrap()
        }

        #[test]
        fn selected_choice_is_shared_with_construction_when_require_and_next_attempt_is_fresh() {
            let (src, donor, trace) = tables();
            let plan = plan(
                &src,
                &donor,
                Some("P.X > 0"),
                "P.X = VALUE AND P.Z = 3",
                true,
                false,
            );
            for _ in 0..2 {
                trace.borrow_mut().clear();
                let attempt = plan.execute(
                    &src,
                    &[&donor],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let execution = attempt.result.unwrap();
                assert_eq!(
                    execution.dataset.rows(),
                    [vec![Value::Int(1), Value::Int(2)]]
                );
                assert_eq!(
                    execution
                        .verifications
                        .iter()
                        .map(|v| (v.path.as_str(), v.evaluated_count, v.failed_count))
                        .collect::<Vec<_>>(),
                    [
                        ("verifications[0].unique", 1, 0),
                        ("verifications[1].assert", 1, 0),
                        ("verifications[2].row_count", 1, 0)
                    ]
                );
                assert!(attempt.handler_counts.is_empty());
                assert_eq!(
                    *trace.borrow(),
                    [
                        ("SRC", 0, 0),
                        ("DONOR", 0, 0),
                        ("DONOR", 0, 1),
                        ("DONOR", 0, 1),
                        ("DONOR", 0, 1),
                        ("DONOR", 0, 2)
                    ]
                );
            }
        }
        #[test]
        fn false_when_still_evaluates_require_and_preserves_original_noncloneable_donor_error() {
            let (src, donor, trace) = tables();
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            let marker = Rc::new(());
            *donor.fail.borrow_mut() = Some((0, 1, Failure(payload, marker.clone())));
            let attempt = plan(&src, &donor, Some("FALSE"), "P.X = 2", false, false).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::SecondaryCell {
                path,
                source,
                source_row,
                error: CellError::Access(Failure(payload, retained)),
            } = *attempt.result.unwrap_err()
            else {
                panic!("original opaque source owner")
            };
            assert_eq!(
                (path.as_str(), source, source_row),
                ("intermediates[0]", 0, 0),
                "the assertion context must not invent a destination column"
            );
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&retained, &marker));
            assert_eq!(
                *trace.borrow(),
                [("SRC", 0, 0), ("DONOR", 0, 0), ("DONOR", 0, 1)]
            );
            assert_eq!(attempt.retained_verifications.len(), 1);
            assert_eq!(
                attempt.retained_verifications[0].path,
                "verifications[0].unique"
            );
            assert_eq!(attempt.retained_verifications[0].evaluated_count, 1);
            assert!(attempt.handler_counts.is_empty());
        }
        #[test]
        fn when_failure_precedes_require_and_keeps_original_opaque_payload_and_prefix() {
            let (src, donor, trace) = tables();
            let marker = Rc::new(());
            let payload = Box::new(7);
            let address = &*payload as *const u8;
            *donor.fail.borrow_mut() = Some((0, 2, Failure(payload, marker.clone())));
            let attempt = plan(&src, &donor, Some("P.Z > 0"), "P.X > 0", false, false).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::SecondaryCell {
                path,
                error: CellError::Access(Failure(payload, retained)),
                ..
            } = *attempt.result.unwrap_err()
            else {
                panic!("when opaque owner")
            };
            assert_eq!(path, "intermediates[0]");
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&marker, &retained));
            assert_eq!(
                *trace.borrow(),
                [("SRC", 0, 0), ("DONOR", 0, 0), ("DONOR", 0, 2)]
            );
            assert_eq!(attempt.retained_verifications.len(), 1);
        }
        #[test]
        fn absent_lookup_preserves_intermediate_diagnostic_identity_without_column_attribution() {
            let (mut src, donor, trace) = tables();
            src.rows[0][0] = Value::Int(8);
            let attempt = plan(&src, &donor, Some("FALSE"), "P.X > 0", false, false).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::UnmatchedKey {
                path,
                dataset,
                intermediate,
                matched_key,
                identity: Some(identity),
            } = *attempt.result.unwrap_err()
            else {
                panic!("native selected-record no-match condition")
            };
            assert_eq!(
                (path.as_str(), dataset.as_str(), intermediate.as_str()),
                ("intermediates[0]", "DONOR", "P")
            );
            assert_eq!(matched_key, [("ID".into(), Value::Int(8))]);
            assert_eq!(identity.position, 0);
            assert_eq!(identity.values, [Value::Int(8)]);
            assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("DONOR", 0, 0)]);
            assert_eq!(attempt.retained_verifications.len(), 1);
        }
        #[test]
        fn zero_output_type_admission_reads_no_donor_cells_or_functions() {
            let (mut src, donor, trace) = tables();
            src.rows.clear();
            let attempt = plan(&src, &donor, None, "P.X = 'invalid'", false, false).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::VerificationPredicate { error, records } =
                *attempt.result.unwrap_err()
            else {
                panic!("schema-only assertion type condition")
            };
            assert_eq!(error.spec_path, "verifications[1].assert.require");
            assert!(matches!(
                error.kind,
                yamaa_core::expression::ErrorKind::Condition(_)
            ));
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].evaluated_count, 0);
            assert!(trace.borrow().is_empty());
            assert!(attempt.handler_counts.is_empty());
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
                Ok(Value::Int(2))
            }
        }
        #[test]
        fn derived_donor_callback_counts_follow_one_materialization_per_attempt() {
            let (src, mut donor, trace) = tables();
            donor.schema = schema(&[("ID", T::Int)]);
            donor.rows = vec![vec![Value::Int(1)], vec![Value::Int(2)]];
            let signature = InvocationPlan::new(
                FunctionIdentity {
                    name: "chosen".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "project.chosen".into(),
                },
                vec![],
                T::Int,
                false,
            )
            .unwrap();
            let output = schema(&[("ID", T::Int), ("X", T::Int)]);
            let nested = DatasetPlan::new(
                output.clone(),
                output,
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assignment(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![assignment(
                    1,
                    Expression::Function(BoundFunction::new(0, signature.clone(), vec![]).unwrap()),
                )],
                vec![0],
                vec![],
            )
            .unwrap();
            let derived = IntermediateDerivations {
                plan: std::sync::Arc::new(nested),
                current: vec![],
                stored_columns: 1,
                typed_columns: vec![],
            };
            let plan = build_plan(
                &src,
                &donor,
                Some("P.X > 0"),
                "P.X = VALUE",
                true,
                false,
                Some(derived),
            );
            let mut calls = Calls {
                signature,
                count: 0,
            };
            for expected in [2, 4] {
                trace.borrow_mut().clear();
                let attempt = plan.execute(&src, &[&donor], &mut calls, None);
                assert_eq!(
                    attempt.result.unwrap().dataset.rows(),
                    [vec![Value::Int(1), Value::Int(2)]]
                );
                assert_eq!(
                    calls.count, expected,
                    "no callback repeats for WHEN/REQUIRE or report projection"
                );
                assert_eq!(
                    *trace.borrow(),
                    [("SRC", 0, 0), ("DONOR", 0, 0), ("DONOR", 1, 0)]
                );
            }
            let mut empty = src;
            empty.rows.clear();
            trace.borrow_mut().clear();
            let empty_plan = plan;
            assert!(empty_plan
                .execute(&empty, &[&donor], &mut calls, None)
                .result
                .is_ok());
            assert_eq!(calls.count, 4);
            assert!(trace.borrow().is_empty());
        }
    }
    mod correlated_relation_predicates {
        //! Primary donors borrow the original table even while a secondary drives construction.
        use crate::support::table_schema as schema;
        use std::cell::RefCell;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            expression::{compile, Context},
        };
        use yamaa_core::{
            dataset::{
                Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
                IntermediateKey, IntermediateSource, Keep, MatchRead, MatchValue, OrderTerm,
                RelationPredicate, RowMode, RowTemplate, SecondarySource, SourceSchemas,
                SourceSelection,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            reads: RefCell<Vec<(usize, usize)>>,
            failure: Option<(usize, usize)>,
        }
        impl TableAccess for Table {
            type Error = &'static str;
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
                    return Err(CellError::Access("held primary failure"));
                }
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

        fn fixture(text: &str, failure: Option<(usize, usize)>) -> (Table, Table, DatasetPlan) {
            let current = Table {
                schema: schema(&[("ID", T::Int), ("CUT", T::Int), ("SEQ", T::Int)]),
                rows: vec![
                    vec![Value::Int(1), Value::Int(2), Value::Int(1)],
                    vec![Value::Int(1), Value::Int(4), Value::Int(2)],
                    vec![Value::Int(3), Value::Int(0), Value::Int(3)],
                ],
                reads: RefCell::default(),
                failure,
            };
            let donor = Table {
                schema: schema(&[
                    ("ID", T::Int),
                    ("TIME", T::Int),
                    ("A", T::Str),
                    ("B", T::Str),
                ]),
                rows: vec![
                    vec![
                        Value::Int(1),
                        Value::Int(1),
                        Value::Str("one".into()),
                        Value::Str("x".into()),
                    ],
                    vec![
                        Value::Int(1),
                        Value::Int(3),
                        Value::Str("three".into()),
                        Value::Missing,
                    ],
                    vec![
                        Value::Int(2),
                        Value::Int(9),
                        Value::Str("trap".into()),
                        Value::Str("trap".into()),
                    ],
                ],
                reads: RefCell::default(),
                failure: Some((2, 1)),
            };
            let filter = RelationPredicate::new(
                BoundPredicate::new(
                    compile(text, Context::Predicate, "intermediates[0].filter").unwrap(),
                    vec![
                        Binding {
                            name: "D.TIME".into(),
                            read: Read::Source(1),
                        },
                        Binding {
                            name: "SRC.CUT".into(),
                            read: Read::Column(0),
                        },
                    ],
                )
                .unwrap(),
                vec![CurrentField {
                    identifier: "SRC.CUT".into(),
                    read: CurrentRead::Record {
                        source: 0,
                        column: 1,
                    },
                }],
            )
            .unwrap();
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
                filter: Some(filter),
                selection: Some(SourceSelection {
                    order_by: vec![OrderTerm {
                        column: 1,
                        descending: false,
                        nulls_first: false,
                    }],
                    keep: Keep::Last,
                }),
                no_match: Some(Value::Missing),
                require_match: false,
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,

                range: None,
                columns: None,
            };
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: current.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "D".into(),
                        schema: donor.schema.clone(),
                    }],
                },
                vec![item],
                schema(&[
                    ("ID", T::Int),
                    ("A", T::Str),
                    ("B", T::Str),
                    ("SEQ", T::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            Expression::Intermediate {
                                index: 0,
                                column: 2,
                            },
                        ),
                        assign(
                            2,
                            Expression::Intermediate {
                                index: 0,
                                column: 3,
                            },
                        ),
                        assign(3, Expression::Source(2)),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0, 3],
                vec![],
            )
            .unwrap();
            (current, donor, plan)
        }

        #[test]
        fn equality_matches_are_filtered_per_row_and_all_fields_share_one_choice() {
            let (current, donor, plan) = fixture("D.TIME < SRC.CUT", None);
            let result = plan.execute(
                &current,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [
                    vec![
                        Value::Int(1),
                        Value::Str("one".into()),
                        Value::Str("x".into()),
                        Value::Int(1)
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("three".into()),
                        Value::Missing,
                        Value::Int(2)
                    ],
                    vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(3)],
                ]
            );
            assert_eq!(
                *current.reads.borrow(),
                [
                    (0, 0),
                    (0, 1),
                    (0, 1),
                    (0, 2),
                    (1, 0),
                    (1, 1),
                    (1, 1),
                    (1, 2),
                    (2, 0),
                    (2, 2)
                ]
            );
            assert_eq!(
                *donor.reads.borrow(),
                [
                    (0, 0),
                    (0, 1),
                    (1, 0),
                    (1, 1),
                    (2, 0),
                    (0, 2),
                    (0, 3),
                    (0, 0),
                    (0, 1),
                    (1, 0),
                    (1, 1),
                    (2, 0),
                    (1, 1),
                    (0, 1),
                    (1, 2),
                    (1, 3),
                    (0, 0),
                    (1, 0),
                    (2, 0),
                ]
            );
            assert_eq!(
                result
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler, h.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "read.C1.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        1
                    ),
                    (
                        "read.C1.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        1
                    ),
                    (
                        "read.C2.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        1
                    ),
                    (
                        "read.C2.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        1
                    ),
                ]
            );
        }

        #[test]
        fn false_boolean_children_keep_written_current_failure_order() {
            let (current, donor, plan) = fixture("D.TIME < 0 AND SRC.CUT > 0", Some((0, 1)));
            let result = plan.execute(
                &current,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(
                matches!(*result.result.unwrap_err(), yamaa_core::dataset::ExecutionError::Cell {ref path,source_row:0,error:CellError::Access("held primary failure")} if path=="intermediates[0].filter")
            );
            assert_eq!(*current.reads.borrow(), [(0, 0), (0, 1)]);
            assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1)]);
            assert_eq!(
                result
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler, h.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "read.C1.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C1.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        0
                    ),
                    (
                        "read.C2.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C2.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        0
                    ),
                ]
            );
        }
        #[test]
        fn unmatched_rows_do_not_resolve_any_predicate_operand() {
            let (mut current, donor, plan) = fixture("D.TIME < SRC.CUT", Some((0, 1)));
            for (index, row) in current.rows.iter_mut().enumerate() {
                row[0] = Value::Int(3 + index as i64);
            }
            let result = plan.execute(
                &current,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [
                    vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(1)],
                    vec![Value::Int(4), Value::Missing, Value::Missing, Value::Int(2)],
                    vec![Value::Int(5), Value::Missing, Value::Missing, Value::Int(3)],
                ]
            );
            assert_eq!(
                *current.reads.borrow(),
                [(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)]
            );
            assert_eq!(
                *donor.reads.borrow(),
                [
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (0, 0),
                    (1, 0),
                    (2, 0)
                ]
            );
            assert_eq!(
                result
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler, h.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "read.C1.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C1.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        3
                    ),
                    (
                        "read.C2.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C2.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        3
                    ),
                ]
            );
        }
        #[test]
        fn a_current_access_failure_retains_its_owner_and_exact_successful_handler_prefix() {
            let (current, donor, plan) = fixture("D.TIME < SRC.CUT", Some((1, 1)));
            let result = plan.execute(
                &current,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(
                matches!(*result.result.unwrap_err(),yamaa_core::dataset::ExecutionError::Cell {ref path,source_row:1,error:CellError::Access("held primary failure")} if path=="intermediates[0].filter")
            );
            assert_eq!(
                *current.reads.borrow(),
                [(0, 0), (0, 1), (0, 1), (0, 2), (1, 0), (1, 1)]
            );
            assert_eq!(
                *donor.reads.borrow(),
                [
                    (0, 0),
                    (0, 1),
                    (1, 0),
                    (1, 1),
                    (2, 0),
                    (0, 2),
                    (0, 3),
                    (0, 0),
                    (0, 1)
                ]
            );
            assert_eq!(
                result
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler, h.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "read.C1.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C1.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        0
                    ),
                    (
                        "read.C2.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C2.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        0
                    ),
                ]
            );
        }

        fn rebuilt(
            current: &Table,
            donor: &Table,
            old: &DatasetPlan,
            items: Vec<Intermediate>,
            assignments: Vec<Assignment>,
        ) -> DatasetPlan {
            DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: current.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "D".into(),
                        schema: donor.schema.clone(),
                    }],
                },
                items,
                old.output().clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments,
                    filter: None,
                }],
                vec![],
                vec![0, 3],
                vec![],
            )
            .unwrap()
        }
        #[test]
        fn completed_output_current_fields_keep_normal_missing_and_choice_semantics() {
            let (current, donor, old) = fixture("D.TIME < SRC.CUT", None);
            let mut items = old.intermediates().to_vec();
            items[0].filter = Some(
                RelationPredicate::new(
                    items[0].filter.as_ref().unwrap().predicate().clone(),
                    vec![CurrentField {
                        identifier: "SRC.CUT".into(),
                        read: CurrentRead::Output(3),
                    }],
                )
                .unwrap(),
            );
            let mut assignments = old.templates()[0].assignments.clone();
            let seq = assignments.pop().unwrap();
            assignments.insert(1, seq);
            let plan = rebuilt(&current, &donor, &old, items, assignments);
            let result = plan.execute(
                &current,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [
                    vec![Value::Int(1), Value::Missing, Value::Missing, Value::Int(1)],
                    vec![
                        Value::Int(1),
                        Value::Str("one".into()),
                        Value::Str("x".into()),
                        Value::Int(2)
                    ],
                    vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(3)],
                ]
            );
            assert_eq!(
                *current.reads.borrow(),
                [(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)]
            );
            assert_eq!(
                result
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler, h.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "read.C1.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C1.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        2
                    ),
                    (
                        "read.C2.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C2.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        2
                    ),
                ]
            );
        }
        #[test]
        fn current_kept_selections_share_the_attempt_cache_without_recovery_observations() {
            let (current, donor, old) = fixture("D.TIME < SRC.CUT", None);
            let mut first = old.intermediates()[0].clone();
            first.filter = None;
            let mut second = first.clone();
            second.identifier = "Q".into();
            second.path = "intermediates[1]".into();
            second.filter = Some(
                RelationPredicate::new(
                    old.intermediates()[0]
                        .filter
                        .as_ref()
                        .unwrap()
                        .predicate()
                        .clone(),
                    vec![CurrentField {
                        identifier: "SRC.CUT".into(),
                        read: CurrentRead::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    }],
                )
                .unwrap(),
            );
            let assignments = vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Intermediate {
                        index: 1,
                        column: 2,
                    },
                ),
                assign(
                    2,
                    Expression::Intermediate {
                        index: 1,
                        column: 3,
                    },
                ),
                assign(3, Expression::Source(2)),
            ];
            let plan = rebuilt(&current, &donor, &old, vec![first, second], assignments);
            let result = plan.execute(
                &current,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [
                    vec![
                        Value::Int(1),
                        Value::Str("one".into()),
                        Value::Str("x".into()),
                        Value::Int(1)
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("one".into()),
                        Value::Str("x".into()),
                        Value::Int(2)
                    ],
                    vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(3)],
                ]
            );
            assert_eq!(
                *current.reads.borrow(),
                [(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)]
            );
            assert_eq!(
                result
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler, h.count))
                    .collect::<Vec<_>>(),
                [
                    (
                        "read.C1.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C1.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        1
                    ),
                    (
                        "read.C2.multiple_matches",
                        yamaa_core::numeric_lifecycle::HandlerKind::MultipleMatches,
                        0
                    ),
                    (
                        "read.C2.no_match",
                        yamaa_core::numeric_lifecycle::HandlerKind::NoMatch,
                        1
                    ),
                ]
            );
        }
    }
    mod dataset_admission {
        use std::{cell::Cell, convert::Infallible};
        use yamaa_core::dataset::{
            Assignment, DatasetPlan, ExecutionError, Expression, Resource, RowMode, RowTemplate,
            SecondarySource,
        };
        use yamaa_core::{function_invocation::NoFunctions, limits::Limits};
        use yamaa_core::{
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        struct Source {
            schema: TableSchema,
            rows: usize,
            reads: Cell<usize>,
        }

        impl Source {
            fn new(rows: usize, columns: usize) -> Self {
                Self {
                    schema: TableSchema::new(
                        (0..columns)
                            .map(|column| Column {
                                name: format!("C{column}"),
                                kind: ColumnType::Int,
                            })
                            .collect(),
                    )
                    .unwrap(),
                    rows,
                    reads: Cell::new(0),
                }
            }
        }

        impl TableAccess for Source {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.rows
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                assert!(self.rows <= Limits::TABLE_ROWS);
                assert!(self
                    .rows
                    .checked_mul(self.schema.columns().len())
                    .is_some_and(|cells| cells <= Limits::TABLE_CELLS));
                self.reads.set(self.reads.get() + 1);
                if row >= self.rows || column >= self.schema.columns().len() {
                    return Err(CellError::OutOfBounds { row, column });
                }
                Ok(ValueRef::Int(i64::try_from(row).unwrap()))
            }
        }

        fn plan(source: &Source, secondary: usize) -> DatasetPlan {
            DatasetPlan::new_with_sources(
                source.schema.clone(),
                (0..secondary)
                    .map(|index| SecondarySource {
                        name: format!("S{index}"),
                        schema: source.schema.clone(),
                    })
                    .collect(),
                source.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: (0..source.schema.columns().len())
                        .map(|column| Assignment {
                            column,
                            expression: Expression::Source(column),
                            path: format!("columns.C{column}"),
                        })
                        .collect(),
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap()
        }

        #[test]
        fn source_count_is_admitted_before_reads_and_repeated_attempts_are_independent() {
            for secondary_count in [63, 64] {
                let primary = Source::new(2, 1);
                let secondary = Source::new(2, 1);
                let plan = plan(&primary, secondary_count);
                let inputs =
                    vec![&secondary as &dyn TableAccess<Error = Infallible>; secondary_count];
                for _ in 0..2 {
                    let attempted = plan.execute(&primary, &inputs, &mut NoFunctions::new(), None);
                    if secondary_count == 63 {
                        assert_eq!(
                            attempted.result.unwrap().dataset.rows(),
                            &[vec![Value::Int(0)], vec![Value::Int(1)]]
                        );
                    } else {
                        assert_eq!(
                            *attempted.result.unwrap_err(),
                            ExecutionError::Limit {
                                resource: Resource::SourceCount,
                                limit: 64,
                                required: Some(65),
                            }
                        );
                        assert_eq!(primary.reads.get(), 0);
                    }
                    assert_eq!(secondary.reads.get(), 0);
                }
            }
        }

        #[test]
        fn row_and_cell_geometry_refusals_precede_all_cell_reads() {
            for (rows, columns, resource, limit, required) in [
                (10_000_001, 1, Resource::SourceRows, 10_000_000, 10_000_001),
                (
                    3_575_419,
                    179,
                    Resource::SourceCells,
                    640_000_000,
                    640_000_001,
                ),
                (usize::MAX, 2, Resource::SourceRows, 10_000_000, usize::MAX),
            ] {
                let primary = Source::new(rows, columns);
                let secondary = Source::new(1, columns);
                let attempted = plan(&primary, 1).execute(
                    &primary,
                    &[&secondary],
                    &mut NoFunctions::new(),
                    None,
                );
                assert_eq!(
                    *attempted.result.unwrap_err(),
                    ExecutionError::SourceLimit {
                        resource,
                        limit,
                        required: Some(required),
                        source_index: 0,
                    }
                );
                assert_eq!(primary.reads.get(), 0);
                assert_eq!(secondary.reads.get(), 0);
            }
        }

        #[test]
        fn secondary_width_refusal_retains_only_the_refused_source_coordinate() {
            for width in [1_024, 1_025] {
                let primary = Source::new(1, 1);
                let narrow = Source::new(1, 1);
                let wide = Source::new(1, width);
                let plan = DatasetPlan::new_with_sources(
                    primary.schema.clone(),
                    [&narrow, &wide]
                        .into_iter()
                        .enumerate()
                        .map(|(index, source)| SecondarySource {
                            name: format!("S{index}"),
                            schema: source.schema.clone(),
                        })
                        .collect(),
                    primary.schema.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![Assignment {
                            column: 0,
                            expression: Expression::Source(0),
                            path: "columns.C0".into(),
                        }],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap();
                let attempted =
                    plan.execute(&primary, &[&narrow, &wide], &mut NoFunctions::new(), None);
                if width == 1_024 {
                    assert_eq!(
                        attempted.result.unwrap().dataset.rows(),
                        &[vec![Value::Int(0)]]
                    );
                    assert_eq!(primary.reads.get(), 1);
                } else {
                    assert_eq!(
                        *attempted.result.unwrap_err(),
                        ExecutionError::SourceLimit {
                            resource: Resource::SourceColumns,
                            limit: 1_024,
                            required: Some(1_025),
                            source_index: 2,
                        }
                    );
                    assert_eq!(primary.reads.get(), 0);
                }
                assert_eq!(narrow.reads.get(), 0);
                assert_eq!(wide.reads.get(), 0);
            }
        }

        #[test]
        fn column_width_admits_1024_and_refuses_1025_before_cell_reads() {
            for width in [1_024, 1_025] {
                let source = Source::new(1, width);
                let attempted =
                    plan(&source, 0).execute(&source, &[], &mut NoFunctions::new(), None);
                if width == 1_024 {
                    assert_eq!(
                        attempted.result.unwrap().dataset.rows(),
                        &[vec![Value::Int(0); 1_024]]
                    );
                    assert_eq!(source.reads.get(), 1_024);
                } else {
                    assert_eq!(
                        *attempted.result.unwrap_err(),
                        ExecutionError::SourceLimit {
                            resource: Resource::SourceColumns,
                            limit: 1_024,
                            required: Some(1_025),
                            source_index: 0,
                        }
                    );
                    assert_eq!(source.reads.get(), 0);
                }
            }
        }

        #[test]
        fn wider_output_refusal_names_output_columns_without_attributing_it_to_input() {
            let source = Source::new(1, 1);
            let output = Source::new(1, 1_025);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                output.schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: (0..1_025)
                        .map(|column| Assignment {
                            column,
                            expression: if column == 0 {
                                Expression::Source(0)
                            } else {
                                Expression::Literal(Value::Int(1))
                            },
                            path: format!("columns.C{column}"),
                        })
                        .collect(),
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempted = plan.execute(&source, &[], &mut NoFunctions::new(), None);
            assert_eq!(
                *attempted.result.unwrap_err(),
                ExecutionError::ScopedLimit {
                    resource: Resource::OutputColumns,
                    limit: 1_024,
                    required: Some(1_025),
                    spec_paths: vec!["output.columns".into()],
                }
            );
            assert_eq!(source.reads.get(), 0);
        }

        #[test]
        fn fixed_geometry_refusal_closes_admission_once_before_finished() {
            use yamaa_core::{
                application_observer::{Outcome, Refusal},
                dataset::{ExecutionEvent, ExecutionPhase},
            };
            let source = Source::new(10_000_001, 1);
            let mut events = Vec::new();
            let attempt = plan(&source, 0).execute(
                &source,
                &[],
                &mut NoFunctions::new(),
                Some(&mut |event| events.push(event)),
            );
            assert!(attempt.result.is_err());
            assert_eq!(
                events,
                [
                    ExecutionEvent::Phase(ExecutionPhase::Admission),
                    ExecutionEvent::Refusal {
                        phase: ExecutionPhase::Admission,
                        refusal: Refusal {
                            resource: Resource::SourceRows,
                            limit: 10_000_000,
                            required: Some(10_000_001)
                        },
                    },
                    ExecutionEvent::Exit {
                        phase: ExecutionPhase::Admission,
                        outcome: Outcome::Failed
                    },
                    ExecutionEvent::Phase(ExecutionPhase::Finished),
                ]
            );
            assert_eq!(source.reads.get(), 0);
        }
    }
    mod dataset_aggregate {
        //! Independent relation, complete-context and opaque-read ordering controls.
        use std::cell::RefCell;
        use yamaa_core::dataset::*;
        use yamaa_core::json::Value as V;
        use yamaa_core::{
            diagnostic::ConditionCode as C,
            expression::{compile, Context},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            reads: RefCell<Vec<(usize, usize)>>,
            fail: Option<(usize, usize)>,
        }
        impl TableAccess for Table {
            type Error = &'static str;
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
                if self.fail == Some((row, column)) {
                    return Err(CellError::Access("late original"));
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
                        kind: ColumnType::Int,
                    })
                    .collect(),
            )
            .unwrap()
        }
        fn table(rows: Vec<Vec<Value>>) -> Table {
            Table {
                schema: schema(&["ID", "V"]),
                rows,
                reads: Default::default(),
                fail: None,
            }
        }
        fn assignment(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("columns[{column}].derivation"),
            }
        }
        fn aggregate(
            text: &str,
            relation: AggregateRelation,
            bindings: &[(&str, usize)],
            phase: AggregatePhase,
            context: Option<AggregateRowContext>,
        ) -> Expression {
            Expression::Aggregate(Box::new(
                BoundAggregate::new(
                    compile(
                        text,
                        Context::Aggregate,
                        "columns.RESULT.derivation.aggregate.expr",
                    )
                    .unwrap(),
                    relation,
                    bindings
                        .iter()
                        .map(|(name, column)| AggregateBinding {
                            name: (*name).into(),
                            column: *column,
                        })
                        .collect(),
                    None,
                    phase,
                    context,
                    "columns.RESULT.derivation.aggregate",
                )
                .unwrap(),
            ))
        }

        #[test]
        fn qualified_predicates_visit_only_matching_donor_records() {
            use yamaa_core::bound_expression::{Binding, BoundPredicate, Read};
            let primary = table(vec![vec![Value::Int(1), Value::Int(0)]]);
            let mut donor = table(vec![
                vec![Value::Int(1), Value::Int(5)],
                vec![Value::Int(2), Value::Int(8)],
            ]);
            donor.fail = Some((1, 1));
            let predicate = BoundPredicate::new(
                yamaa_core::expression::compile(
                    "S.V > 0",
                    yamaa_core::expression::Context::Predicate,
                    "aggregate.filter",
                )
                .unwrap(),
                vec![Binding {
                    name: "S.V".into(),
                    read: Read::Source(1),
                }],
            )
            .unwrap();
            let bound = BoundAggregate::new(
                compile("SUM(S.V)", Context::Aggregate, "aggregate").unwrap(),
                AggregateRelation::Matched {
                    source: AggregateSource::Secondary(0),
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "aggregate.key.ID".into(),
                        value: MatchValue::Variable(MatchRead::Output {
                            column: 0,
                            identifier: "ID".into(),
                        }),
                    }],
                    group_by: vec![],
                },
                vec![AggregateBinding {
                    name: "S.V".into(),
                    column: 1,
                }],
                Some(predicate),
                AggregatePhase::Derivation,
                None,
                "aggregate",
            )
            .unwrap();
            let plan = DatasetPlan::new_with_sources(
                primary.schema.clone(),
                vec![SecondarySource {
                    name: "S".into(),
                    schema: donor.schema.clone(),
                }],
                schema(&["ID", "RESULT"]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assignment(0, Expression::Source(0))],
                    filter: None,
                }],
                vec![assignment(1, Expression::Aggregate(Box::new(bound)))],
                vec![0],
                vec![],
            )
            .unwrap();
            assert_eq!(
                plan.execute(
                    &primary,
                    &[&donor],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[vec![Value::Int(1), Value::Int(5)]]
            );
            assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1), (1, 0), (0, 1)]);
        }

        #[test]
        fn correlated_sum_and_mean_keep_donor_order_through_cancellation_and_missing_values() {
            for (values, sum, mean) in [
                ([1e16, 1.0, -1e16], 0.0_f64, 0.0_f64),
                ([1e16, -1e16, 1.0], 1.0_f64, 1.0_f64 / 3.0),
            ] {
                let primary = table(vec![vec![Value::Int(1), Value::Int(0)]]);
                let donor = Table {
                    schema: TableSchema::new(vec![
                        Column {
                            name: "ID".into(),
                            kind: ColumnType::Int,
                        },
                        Column {
                            name: "V".into(),
                            kind: ColumnType::Float,
                        },
                    ])
                    .unwrap(),
                    rows: vec![
                        vec![Value::Int(1), Value::float(values[0])],
                        vec![Value::Int(2), Value::float(99.0)],
                        vec![Value::Int(1), Value::float(values[1])],
                        vec![Value::Int(1), Value::Missing],
                        vec![Value::Int(1), Value::float(values[2])],
                    ],
                    reads: Default::default(),
                    // This unrelated donor's value must never enter either reduction.
                    fail: Some((1, 1)),
                };
                let relation = AggregateRelation::Matched {
                    source: AggregateSource::Secondary(0),
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "aggregate.key.ID".into(),
                        value: MatchValue::Variable(MatchRead::Output {
                            column: 0,
                            identifier: "ID".into(),
                        }),
                    }],
                    group_by: vec![],
                };
                let output = TableSchema::new(vec![
                    Column {
                        name: "ID".into(),
                        kind: ColumnType::Int,
                    },
                    Column {
                        name: "SUM".into(),
                        kind: ColumnType::Float,
                    },
                    Column {
                        name: "MEAN".into(),
                        kind: ColumnType::Float,
                    },
                ])
                .unwrap();
                let plan = DatasetPlan::new_with_sources(
                    primary.schema.clone(),
                    vec![SecondarySource {
                        name: "S".into(),
                        schema: donor.schema.clone(),
                    }],
                    output,
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![assignment(0, Expression::Source(0))],
                        filter: None,
                    }],
                    ["SUM(S.V)", "MEAN(S.V)"]
                        .iter()
                        .enumerate()
                        .map(|(index, expression)| {
                            assignment(
                                index + 1,
                                aggregate(
                                    expression,
                                    relation.clone(),
                                    &[("S.V", 1)],
                                    AggregatePhase::Derivation,
                                    None,
                                ),
                            )
                        })
                        .collect(),
                    vec![0],
                    vec![],
                )
                .unwrap();
                let result = plan
                    .execute(
                        &primary,
                        &[&donor],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap();
                assert_eq!(result.dataset.rows().len(), 1);
                assert_eq!(result.dataset.rows()[0][0], Value::Int(1));
                for (column, expected) in [(1, sum), (2, mean)] {
                    let Value::Float(actual) = result.dataset.rows()[0][column] else {
                        panic!("floating reduction result");
                    };
                    assert_eq!(actual.get().to_bits(), expected.to_bits());
                }
                let value_reads = donor
                    .reads
                    .borrow()
                    .iter()
                    .copied()
                    .filter(|&(_, column)| column == 1)
                    .collect::<Vec<_>>();
                assert_eq!(value_reads, [(0, 1), (2, 1), (3, 1), (4, 1)].repeat(2));
            }
        }

        #[test]
        fn grouped_numeric_reducers_and_record_star_keep_order_and_empty_distinctions() {
            let source = table(vec![
                vec![Value::Int(1), Value::Int(3)],
                vec![Value::Int(1), Value::Int(5)],
                vec![Value::Int(2), Value::Missing],
            ]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&["ID", "RESULT", "N"]),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assignment(0, Expression::Source(0)),
                        assignment(
                            1,
                            aggregate(
                                "SUM(D.V * 2) + D.ID",
                                AggregateRelation::CurrentGroup,
                                &[("D.V", 1), ("D.ID", 0)],
                                AggregatePhase::RowConstruction,
                                None,
                            ),
                        ),
                        assignment(
                            2,
                            aggregate(
                                "COUNT(D.*)",
                                AggregateRelation::CurrentGroup,
                                &[],
                                AggregatePhase::RowConstruction,
                                None,
                            ),
                        ),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
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
                [
                    vec![Value::Int(1), Value::Int(17), Value::Int(2)],
                    vec![Value::Int(2), Value::Missing, Value::Int(1)]
                ]
            );
        }

        #[test]
        fn a_qualified_primary_relation_is_reduced_before_joining_to_each_record() {
            let source = table(vec![
                vec![Value::Int(1), Value::Int(3)],
                vec![Value::Int(1), Value::Int(5)],
            ]);
            let relation = AggregateRelation::Matched {
                source: AggregateSource::Primary,
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "columns.RESULT.derivation.aggregate.key.ID".into(),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 0,
                        identifier: "ID".into(),
                    }),
                }],
                group_by: vec![],
            };
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&["ID", "V", "RESULT"]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assignment(0, Expression::Source(0)),
                        assignment(1, Expression::Source(1)),
                    ],
                    filter: None,
                }],
                vec![assignment(
                    2,
                    aggregate(
                        "SUM(D.V)",
                        relation,
                        &[("D.V", 1)],
                        AggregatePhase::Derivation,
                        None,
                    ),
                )],
                vec![0, 1],
                vec![],
            )
            .unwrap();
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                [
                    vec![Value::Int(1), Value::Int(3), Value::Int(8)],
                    vec![Value::Int(1), Value::Int(5), Value::Int(8)]
                ]
            );
        }

        #[test]
        fn output_group_reduction_reads_the_complete_constructed_relation() {
            let source = table(vec![
                vec![Value::Int(1), Value::Int(3)],
                vec![Value::Int(1), Value::Int(5)],
                vec![Value::Missing, Value::Missing],
            ]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&["ID", "V", "RESULT"]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assignment(0, Expression::Source(0)),
                        assignment(1, Expression::Source(1)),
                    ],
                    filter: None,
                }],
                vec![assignment(
                    2,
                    aggregate(
                        "COUNT(V)",
                        AggregateRelation::Output { group_by: vec![0] },
                        &[("V", 1)],
                        AggregatePhase::Derivation,
                        None,
                    ),
                )],
                vec![1],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            // The missing output key causes its independent retained key failure only
            // after the reducer has seen the complete output relation.
            assert!(matches!(
                *attempt.result.unwrap_err(),
                ExecutionError::KeyFailures(_)
            ));
            assert_eq!(
                *source.reads.borrow(),
                [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
            );
            let source = table(vec![
                vec![Value::Int(1), Value::Int(3)],
                vec![Value::Int(1), Value::Int(5)],
                vec![Value::Missing, Value::Int(7)],
            ]);
            assert_eq!(
                plan.execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                [
                    vec![Value::Int(1), Value::Int(3), Value::Int(2)],
                    vec![Value::Int(1), Value::Int(5), Value::Int(2)],
                    vec![Value::Missing, Value::Int(7), Value::Int(1)]
                ]
            );
        }

        #[test]
        fn only_is_eager_and_retains_complete_group_context_behind_outer_missing() {
            let source = table(vec![
                vec![Value::Int(1), Value::Missing],
                vec![Value::Int(1), Value::Missing],
            ]);
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&["ID", "RESULT"]),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assignment(0, Expression::Source(0)),
                        assignment(
                            1,
                            aggregate(
                                "NULL + ONLY(D.V)",
                                AggregateRelation::CurrentGroup,
                                &[("D.V", 1)],
                                AggregatePhase::RowConstruction,
                                Some(AggregateRowContext {
                                    row: "summary".into(),
                                    groups: vec![("D.ID".into(), 0)],
                                }),
                            ),
                        ),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &source,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(attempt.handler_counts.is_empty());
            let ExecutionError::SemanticDiagnostic {
                diagnostic,
                identity,
            } = *attempt.result.unwrap_err()
            else {
                panic!("typed ONLY failure")
            };
            assert_eq!(diagnostic.code, C::AggregateRowMultipleRecords);
            assert_eq!(diagnostic.definition().requirement, Some("REQ-0501"));
            assert_eq!(
                diagnostic.spec_paths,
                ["columns.RESULT.derivation.aggregate"]
            );
            assert_eq!(identity, None);
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            assert_eq!(
                diagnostic.context,
                [
                    ("expr".into(), V::String("NULL + ONLY(D.V)".into())),
                    ("reducer".into(), V::String("ONLY".into())),
                    (
                        "record_count".into(),
                        V::Number(("2").parse().expect("canonical integer"))
                    ),
                    ("row".into(), V::String("summary".into())),
                    (
                        "group".into(),
                        V::Object([("D.ID".into(), V::from(1))].into_iter().collect())
                    )
                ]
                .into()
            );
            assert_eq!(
                *source.reads.borrow(),
                [(0, 0), (1, 0), (0, 0), (0, 1), (1, 1), (0, 0)]
            );
        }

        #[test]
        fn every_argument_is_collected_before_a_potential_earlier_fold_overflow() {
            let mut source = table(vec![
                vec![Value::Int(1), Value::Int(i64::MAX)],
                vec![Value::Int(1), Value::Int(1)],
                vec![Value::Int(1), Value::Int(0)],
            ]);
            source.fail = Some((2, 1));
            let plan = DatasetPlan::new(
                source.schema.clone(),
                schema(&["ID", "RESULT"]),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assignment(0, Expression::Source(0)),
                        assignment(
                            1,
                            aggregate(
                                "SUM(D.V)",
                                AggregateRelation::CurrentGroup,
                                &[("D.V", 1)],
                                AggregatePhase::RowConstruction,
                                None,
                            ),
                        ),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
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
                ExecutionError::Cell {
                    source_row: 2,
                    error: CellError::Access("late original"),
                    ..
                }
            ));
            assert_eq!(
                *source.reads.borrow(),
                [(0, 0), (1, 0), (2, 0), (0, 0), (0, 1), (1, 1), (2, 1)]
            );
        }

        #[test]
        fn qualified_group_by_outside_the_join_keys_is_refused_req_0470() {
            use yamaa_core::dataset::PlanError;
            // Two donor groups share the join key ID=1 but differ in PARAM, the
            // group_by column. Matching on keys only would silently merge them.
            let primary = table(vec![vec![Value::Int(1), Value::Int(0)]]);
            let donor = Table {
                schema: schema(&["ID", "PARAM", "V"]),
                rows: vec![
                    vec![Value::Int(1), Value::Int(10), Value::Int(2)],
                    vec![Value::Int(1), Value::Int(20), Value::Int(3)],
                ],
                reads: Default::default(),
                fail: None,
            };
            let plan_for = |group_by: Vec<usize>| {
                let bound = BoundAggregate::new(
                    compile("SUM(S.V)", Context::Aggregate, "aggregate").unwrap(),
                    AggregateRelation::Matched {
                        source: AggregateSource::Secondary(0),
                        keys: vec![IntermediateKey {
                            source_column: 0,
                            path: "aggregate.key.ID".into(),
                            value: MatchValue::Variable(MatchRead::Output {
                                column: 0,
                                identifier: "ID".into(),
                            }),
                        }],
                        group_by,
                    },
                    vec![AggregateBinding {
                        name: "S.V".into(),
                        column: 2,
                    }],
                    None,
                    AggregatePhase::Derivation,
                    None,
                    "aggregate",
                )
                .unwrap();
                DatasetPlan::new_with_sources(
                    primary.schema.clone(),
                    vec![SecondarySource {
                        name: "S".into(),
                        schema: donor.schema.clone(),
                    }],
                    schema(&["ID", "RESULT"]),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![assignment(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![assignment(1, Expression::Aggregate(Box::new(bound)))],
                    vec![0],
                    vec![],
                )
            };
            // PARAM (donor column 1) is not a join/output key: refused at
            // validation (plan admission), before any execution.
            assert_eq!(plan_for(vec![1]), Err(PlanError::InvalidSource));
            // Positive control: grouping by the join key itself is compliant, and
            // the admitted aggregate executes exactly as before.
            let plan = plan_for(vec![0]).unwrap();
            assert_eq!(
                plan.execute(
                    &primary,
                    &[&donor],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[vec![Value::Int(1), Value::Int(5)]]
            );
        }

        #[test]
        fn a_range_value_reads_only_the_record_that_built_the_row() {
            use yamaa_core::{dataset::PlanError, value::ValueType};
            let primary = Table {
                schema: schema(&["ID", "CUT"]),
                rows: vec![
                    vec![Value::Int(1), Value::Int(1)],
                    vec![Value::Int(2), Value::Int(2)],
                ],
                reads: Default::default(),
                fail: None,
            };
            let donor = table(vec![
                vec![Value::Int(1), Value::Int(1)],
                vec![Value::Int(1), Value::Int(2)],
                vec![Value::Int(2), Value::Int(1)],
                vec![Value::Int(2), Value::Int(2)],
            ]);
            // C keeps its records in another order, with a limit in the column
            // where the primary keeps its cutoff.
            let other = Table {
                schema: schema(&["ID", "LIMIT"]),
                rows: vec![
                    vec![Value::Int(2), Value::Int(1)],
                    vec![Value::Int(1), Value::Int(2)],
                ],
                reads: Default::default(),
                fail: None,
            };
            let plan_for = |source: usize| {
                let range = RelationRange::new(
                    "aggregate.between".into(),
                    CurrentField {
                        read: CurrentRead::Record { source, column: 1 },
                        identifier: "VALUE".into(),
                    },
                    Some(ValueType::Int),
                    Some(RelationRangeBound {
                        column: 1,
                        identifier: "V".into(),
                        kind: Some(ValueType::Int),
                    }),
                    None,
                )
                .unwrap();
                let bound = BoundAggregate::new(
                    compile("SUM(S.V)", Context::Aggregate, "aggregate").unwrap(),
                    AggregateRelation::Matched {
                        source: AggregateSource::Secondary(0),
                        keys: vec![IntermediateKey {
                            source_column: 0,
                            path: "aggregate.key.ID".into(),
                            value: MatchValue::Variable(MatchRead::Output {
                                column: 0,
                                identifier: "ID".into(),
                            }),
                        }],
                        group_by: vec![],
                    },
                    vec![AggregateBinding {
                        name: "S.V".into(),
                        column: 1,
                    }],
                    None,
                    AggregatePhase::Derivation,
                    None,
                    "aggregate",
                )
                .unwrap()
                .with_record_operations(Some(range), vec![], None);
                DatasetPlan::new_with_sources(
                    primary.schema.clone(),
                    vec![
                        SecondarySource {
                            name: "S".into(),
                            schema: donor.schema.clone(),
                        },
                        SecondarySource {
                            name: "C".into(),
                            schema: other.schema.clone(),
                        },
                    ],
                    schema(&["ID", "RESULT"]),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![assignment(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![assignment(1, Expression::Aggregate(Box::new(bound)))],
                    vec![0],
                    vec![],
                )
            };
            // C has no record at a primary row's position.
            assert_eq!(plan_for(2), Err(PlanError::InvalidSource));
            // The primary record that built each row narrows that row's donors.
            let plan = plan_for(0).unwrap();
            assert_eq!(
                plan.execute(
                    &primary,
                    &[&donor, &other],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .unwrap()
                .dataset
                .rows(),
                &[
                    vec![Value::Int(1), Value::Int(1)],
                    vec![Value::Int(2), Value::Int(3)]
                ]
            );
        }
    }
    mod dataset_diagnostic {
        mod borrowed {
            use crate::support::table_schema as schema;
            use std::{cell::RefCell, convert::Infallible};
            use yamaa_core::dataset::{
                Assignment, DatasetPlan, ExecutionError, Expression, RowIdentity, RowMode,
                RowTemplate,
            };
            use yamaa_core::json::Value as V;
            use yamaa_core::{
                reduction::NumericReducer,
                table::{CellError, TableAccess, TableSchema, ValueRef},
                temporal::{Date, Precision},
                value::{ColumnType, Value, ValueType},
            };

            #[test]
            fn borrowed_failure_projection_retains_every_identity_key_partition_and_host_payload() {
                // Projection compiles for a payload with neither Debug nor Clone.
                struct Payload(Box<u8>);
                let identity = || {
                    Some(RowIdentity {
                        position: 7,
                        values: vec![Value::Int(i64::MAX), Value::Missing],
                    })
                };
                let observed = || {
                    vec![
                        ("flag".into(), Value::Bool(false)),
                        ("missing".into(), Value::Missing),
                    ]
                };
                let date = Value::Date(Date::new(2024, 2, 29, Precision::Month).unwrap());
                let faults: Vec<ExecutionError<Payload>> = vec![
                    ExecutionError::MultipleValues {
                        path: "columns.A.source".into(),
                        identifier: "SRC.A".into(),
                        value_count: 3,
                        identity: identity(),
                    },
                    ExecutionError::UnmatchedKey {
                        path: "columns.A.source".into(),
                        dataset: "DM".into(),
                        intermediate: "I".into(),
                        matched_key: observed(),
                        identity: identity(),
                    },
                    ExecutionError::UnmatchedKey {
                        path: "columns.A.derivation.source".into(),
                        dataset: "DM".into(),
                        intermediate: "intermediate(DM)".into(),
                        matched_key: observed(),
                        identity: identity(),
                    },
                    ExecutionError::MultipleMatches {
                        path: "columns.A.source".into(),
                        dataset: "DM".into(),
                        intermediate: "I".into(),
                        match_count: 2,
                        matched_key: observed(),
                        identity: identity(),
                    },
                    ExecutionError::BaselineAmbiguity {
                        path: "columns.A.baseline".into(),
                        column: "A".into(),
                        date,
                        match_count: 2,
                        partition: observed(),
                    },
                    ExecutionError::ReductionType {
                        path: "columns.A.aggregate".into(),
                        expression: "SUM(SRC.A)".into(),
                        reducer: NumericReducer::Sum,
                        source: "SRC.A".into(),
                        actual: ValueType::Str,
                    },
                ];
                for (error, requirement) in faults.iter().zip([
                    "REQ-0075",
                    "LOOKUP-24",
                    "LOOKUP-24",
                    "REQ-0127",
                    "REQ-0322",
                    "REQ-0510",
                ]) {
                    let finding = error.diagnostic().unwrap().unwrap();
                    assert_eq!(finding.definition().requirement, Some(requirement));
                    assert_eq!(error.diagnostic(), Ok(Some(finding)));
                    match error {
                        ExecutionError::MultipleValues {
                            identity: Some(value),
                            ..
                        }
                        | ExecutionError::UnmatchedKey {
                            identity: Some(value),
                            ..
                        }
                        | ExecutionError::MultipleMatches {
                            identity: Some(value),
                            ..
                        } => {
                            assert_eq!(value.position, 7);
                            assert_eq!(value.values, [Value::Int(i64::MAX), Value::Missing]);
                        }
                        ExecutionError::BaselineAmbiguity { partition, .. } => {
                            assert_eq!(*partition, observed())
                        }
                        ExecutionError::ReductionType { source, .. } => assert_eq!(source, "SRC.A"),
                        _ => panic!("retained cause changed"),
                    }
                    if let ExecutionError::MultipleMatches { matched_key, .. }
                    | ExecutionError::UnmatchedKey { matched_key, .. } = error
                    {
                        assert_eq!(*matched_key, observed());
                    }
                }
                let payload = Payload(Box::new(9));
                let address = &*payload.0 as *const u8;
                let error = ExecutionError::Cell {
                    path: "source".into(),
                    source_row: 9,
                    error: CellError::Access(payload),
                };
                assert!(error.diagnostic().unwrap().is_none());
                let ExecutionError::Cell {
                    error: CellError::Access(Payload(value)),
                    ..
                } = error
                else {
                    panic!("original host error changed")
                };
                assert_eq!(&*value as *const u8, address);
            }

            struct Table {
                schema: TableSchema,
                rows: Vec<Vec<Value>>,
                reads: RefCell<Vec<(usize, usize)>>,
                fail: Option<(usize, usize)>,
            }
            impl TableAccess for Table {
                type Error = &'static str;
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
                    if self.fail == Some((row, column)) {
                        return Err(CellError::Access("later access failure"));
                    }
                    self.rows
                        .get(row)
                        .and_then(|values| values.get(column))
                        .map(ValueRef::from)
                        .ok_or(CellError::OutOfBounds { row, column })
                }
            }
            #[test]
            fn projection_never_replays_reads_and_later_source_failure_keeps_its_precedence() {
                let mut source = Table {
                    schema: schema(&[("ID", ColumnType::Int), ("A", ColumnType::Str)]),
                    rows: vec![
                        vec![Value::Int(1), Value::Str("07".into())],
                        vec![Value::Int(1), Value::Str("7".into())],
                        vec![Value::Int(1), Value::Str("8".into())],
                    ],
                    reads: RefCell::default(),
                    fail: None,
                };
                let plan = DatasetPlan::new(
                    source.schema.clone(),
                    schema(&[("ID", ColumnType::Int), ("A", ColumnType::Int)]),
                    vec![RowTemplate {
                        mode: RowMode::Keys,
                        assignments: vec![Assignment {
                            column: 0,
                            path: "columns.ID.source".into(),
                            expression: Expression::Source(0),
                        }],
                        filter: None,
                    }],
                    vec![Assignment {
                        column: 1,
                        path: "columns.A.source".into(),
                        expression: Expression::Collect {
                            column: 1,
                            identifier: "SRC.A".into(),
                            filter: None,
                            selection: None,
                        },
                    }],
                    vec![0],
                    vec![],
                )
                .unwrap();
                let error = plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap_err();
                let reads = source.reads.borrow().clone();
                let finding = error.diagnostic().unwrap().unwrap();
                assert_eq!(finding.definition().requirement, Some("REQ-0075"));
                assert_eq!(finding.context["value_count"], V::from(3));
                assert_eq!(*source.reads.borrow(), reads);
                assert!(
                    matches!(&*error,ExecutionError::MultipleValues {identity:Some(value),..} if value.values == [Value::Int(1)])
                );
                source.fail = Some((2, 1));
                let error = plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                    .unwrap_err();
                assert!(error.diagnostic().unwrap().is_none());
                assert!(matches!(
                    &*error,
                    ExecutionError::Cell {
                        source_row: 2,
                        error: CellError::Access("later access failure"),
                        ..
                    }
                ));
                // A type-level use also confirms projection imposes no table/error bound.
                let _: Result<
                    Option<yamaa_core::diagnostic::Diagnostic>,
                    yamaa_core::diagnostic::ProjectionError,
                > = ExecutionError::<Infallible>::SchemaMismatch.diagnostic();
            }
        }
    }
}
