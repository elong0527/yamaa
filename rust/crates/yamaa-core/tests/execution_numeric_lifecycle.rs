//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod numeric_lifecycle {
        use std::{collections::BTreeMap, convert::Infallible};
        use yamaa_core::conversion::ConversionError;
        use yamaa_core::evaluation::{EvaluationErrorKind, NumericCondition, NumericResolver};
        use yamaa_core::numeric::ArithmeticErrorKind;
        use yamaa_core::numeric_compiler::compile_numeric;
        use yamaa_core::numeric_lifecycle::{
            HandlerCounter, HandlerKind, LiteralHandler, NumericDerivation, NumericLifecycleError,
        };
        use yamaa_core::value::{ColumnType, Selection, Value, ValueType};

        const COLUMN: &str = "columns.A";
        const EXPRESSION: &str = "columns.A.derivation.value.compute";
        const HANDLER: &str = "columns.A.derivation.unconvertible";

        /// Decode explicit scalar fixture inputs without using the declared output type.
        fn value(token: &str) -> Value {
            if token == "missing" {
                return Value::Missing;
            }
            let (kind, text) = token.split_once(':').unwrap();
            match kind {
                "int" => Value::Int(text.parse().unwrap()),
                "float" => Value::float(text.parse().unwrap()),
                "bool" => Value::Bool(text.parse().unwrap()),
                "str" => Value::Str(text.into()),
                _ => panic!("unknown fixture value"),
            }
        }

        /// Encode types and exact binary64 bits, keeping zero signs observable.
        fn encoded(value: &Value) -> String {
            match value {
                Value::Missing => "missing".into(),
                Value::Int(number) => format!("int:{number}"),
                Value::Float(number) => format!("float:{:016x}", number.get().to_bits()),
                Value::Bool(boolean) => format!("bool:{boolean}"),
                Value::Str(text) => format!("str:{text}"),
                Value::Date(date) => format!(
                    "date:{date}:{}",
                    format!("{:?}", date.collected_precision()).to_lowercase()
                ),
                Value::DateTime(date) => format!(
                    "datetime:{date}:{}",
                    format!("{:?}", date.collected_precision()).to_lowercase()
                ),
            }
        }

        /// Decode the closed destination vocabulary rather than infer it from a result.
        fn column_type(text: &str) -> ColumnType {
            match text {
                "int" => ColumnType::Int,
                "float" => ColumnType::Float,
                "str" => ColumnType::Str,
                "date" => ColumnType::Date,
                "datetime" => ColumnType::DateTime,
                _ => panic!("invalid fixture target"),
            }
        }

        /// Keep overflow decimal diagnostic data distinct from ordinary runtime integers.
        fn conversion_value(error: &ConversionError, target: ColumnType) -> String {
            assert_eq!(error.target, target);
            assert_eq!(error.applicable_handler(), "unconvertible");
            match error.source_type() {
                None => {
                    assert!(error.value.is_null());
                    "missing".into()
                }
                Some(ValueType::Float) => {
                    format!("float:{:016x}", error.value.as_f64().unwrap().to_bits())
                }
                Some(ValueType::Int) => {
                    let number = error.value.as_number().unwrap();
                    format!(
                        "{}:{number}",
                        if number.as_i64().is_some() {
                            "int"
                        } else {
                            "integer"
                        }
                    )
                }
                Some(ValueType::Bool) => format!("bool:{}", error.value.as_bool().unwrap()),
                Some(ValueType::Str) => format!("str:{}", error.value.as_str().unwrap()),
                Some(ValueType::Date | ValueType::DateTime) => {
                    panic!("unexpected temporal lifecycle fixture input")
                }
            }
        }

        /// Fake input port that observes every resolution and keeps absence distinct.
        struct Resolver {
            values: BTreeMap<String, Value>,
            trace: Vec<String>,
        }
        impl NumericResolver for Resolver {
            type Error = Infallible;
            /// Record written evaluation order before returning the selected source value.
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.trace.push(name.into());
                Ok(self
                    .values
                    .get(name)
                    .cloned()
                    .map_or(Selection::Absent, Selection::Present))
            }
        }

        /// Build the same application declaration used by the independent Python runner.
        fn derivation(
            expression: &str,
            target: ColumnType,
            handler: Option<Value>,
        ) -> NumericDerivation {
            NumericDerivation::new(
                compile_numeric(expression, EXPRESSION).unwrap(),
                target,
                COLUMN,
                handler.map(|value| LiteralHandler {
                    spec_path: HANDLER.into(),
                    value,
                }),
            )
        }

        /// Replay results, fatal contexts, paths, traces and handler counts from independent truth.
        #[test]
        fn shared_numeric_lifecycle_vectors() {
            for row in include_str!("fixtures/numeric_lifecycle.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 11);
                let target = column_type(fields[3]);
                let plan = derivation(
                    fields[1],
                    target,
                    (fields[4] != "-").then(|| value(fields[4])),
                );
                let mut counter = HandlerCounter::default();
                plan.register_handlers(&mut counter);
                let mut resolver = Resolver {
                    values: if fields[2] == "-" {
                        BTreeMap::new()
                    } else {
                        fields[2]
                            .split(';')
                            .map(|binding| {
                                let (name, token) = binding.split_once('=').unwrap();
                                (name.into(), value(token))
                            })
                            .collect()
                    },
                    trace: Vec::new(),
                };
                let mut path = "-".to_string();
                let mut context = "-".to_string();
                let mut original = "-".to_string();
                let actual = match plan.evaluate(&mut resolver, &mut counter) {
                    Ok(result) => encoded(&result),
                    Err(failure) => match *failure {
                        NumericLifecycleError::Evaluation(error) => {
                            assert!(fields[1]
                                .get(error.source_span.start..error.source_span.end)
                                .is_some());
                            assert_eq!(error.evaluation.location.expression, fields[1]);
                            path = error.evaluation.location.spec_path;
                            let EvaluationErrorKind::Numeric(condition) = error.evaluation.kind
                            else {
                                panic!("infallible resolver");
                            };
                            context = match &condition {
                                NumericCondition::UnknownField { identifier } => {
                                    format!("identifier:{identifier}")
                                }
                                NumericCondition::IncompatibleInput { identifier, actual } => {
                                    assert_eq!(*actual, ValueType::Str);
                                    format!("source:{identifier}:str")
                                }
                                NumericCondition::Arithmetic(
                                    ArithmeticErrorKind::IntegerOverflow { value },
                                ) => format!("integer:{value}"),
                                NumericCondition::Arithmetic(
                                    ArithmeticErrorKind::InvalidRoundingDigits,
                                ) => "digits:float".into(),
                                _ => "-".into(),
                            };
                            format!(
                                "error:{}:{}:{}",
                                condition.phase(),
                                condition.condition(),
                                condition.requirement()
                            )
                        }
                        NumericLifecycleError::Conversion { spec_path, error } => {
                            path = spec_path;
                            context = conversion_value(&error, target);
                            format!(
                                "error:{}:{}:{}",
                                error.phase(),
                                error.condition(),
                                error.requirement()
                            )
                        }
                        NumericLifecycleError::HandlerConversion {
                            spec_path,
                            column_path,
                            original: failure,
                            replacement,
                        } => {
                            assert_eq!(column_path, COLUMN);
                            path = spec_path;
                            original = conversion_value(&failure, target);
                            context = conversion_value(&replacement, target);
                            format!(
                                "error:{}:{}:{}",
                                replacement.phase(),
                                replacement.condition(),
                                replacement.requirement()
                            )
                        }
                        NumericLifecycleError::Accounting { .. } => {
                            panic!("tiny fixture cannot overflow counters")
                        }
                    },
                };
                assert_eq!(actual, fields[5], "{}", fields[0]);
                assert_eq!(path, fields[6], "{}", fields[0]);
                assert_eq!(
                    resolver.trace.join(","),
                    fields[7].trim_matches('-'),
                    "{}",
                    fields[0]
                );
                assert_eq!(context, fields[9], "{}", fields[0]);
                assert_eq!(original, fields[10], "{}", fields[0]);
                if fields[8] == "-" {
                    assert!(counter.snapshot().is_empty());
                } else {
                    let [entry] = counter.snapshot() else {
                        panic!("one declared handler")
                    };
                    assert_eq!(entry.spec_path, HANDLER);
                    assert_eq!(entry.handler.name(), "unconvertible");
                    assert_eq!(entry.count.to_string(), fields[8], "{}", fields[0]);
                }
            }
        }

        /// Count replacements exactly once on every evaluation; retain declaration order and zeros.
        #[test]
        fn accounting_retains_zero_declarations_and_prior_firings_on_failure() {
            let good = derivation("A", ColumnType::Int, Some(Value::Int(7)));
            let bad = NumericDerivation::new(
                compile_numeric("A", EXPRESSION).unwrap(),
                ColumnType::Int,
                "columns.B",
                Some(LiteralHandler {
                    spec_path: "columns.B.derivation.unconvertible".into(),
                    value: Value::Str("bad".into()),
                }),
            );
            let mut counter = HandlerCounter::default();
            bad.register_handlers(&mut counter);
            good.register_handlers(&mut counter);
            let mut resolver = Resolver {
                values: [("A".into(), Value::float(1.25))].into(),
                trace: Vec::new(),
            };
            for _ in 0..2 {
                assert_eq!(
                    good.evaluate(&mut resolver, &mut counter),
                    Ok(Value::Int(7))
                );
            }
            assert_eq!(
                counter
                    .snapshot()
                    .iter()
                    .map(|c| c.count)
                    .collect::<Vec<_>>(),
                [0, 2]
            );
            assert!(matches!(
                *bad.evaluate(&mut resolver, &mut counter).unwrap_err(),
                NumericLifecycleError::HandlerConversion { .. }
            ));
            good.register_handlers(&mut counter);
            assert_eq!(
                counter
                    .snapshot()
                    .iter()
                    .map(|c| (c.spec_path.as_str(), c.count))
                    .collect::<Vec<_>>(),
                [("columns.B.derivation.unconvertible", 1), (HANDLER, 2)]
            );
            assert_eq!(resolver.trace, ["A", "A", "A"]);
            assert_eq!(counter.snapshot()[0].handler, HandlerKind::Unconvertible);
        }

        /// A caller-owned failure has no Clone requirement and cannot be caught by conversion handling.
        #[test]
        fn opaque_resolver_failure_bypasses_handler_and_keeps_exact_provenance() {
            #[derive(Debug)]
            struct Opaque(u32);
            struct Failed(Vec<String>);
            impl NumericResolver for Failed {
                type Error = Opaque;
                /// Fail on the reached identifier, proving no subsequent resolver is invoked.
                fn resolve(&mut self, name: &str) -> Result<Selection, Opaque> {
                    self.0.push(name.into());
                    Err(Opaque(42))
                }
            }
            let plan = derivation(
                "NULL + BROKEN + LATER",
                ColumnType::Int,
                Some(Value::Int(7)),
            );
            let mut resolver = Failed(Vec::new());
            let mut counter = HandlerCounter::default();
            let failure = plan.evaluate(&mut resolver, &mut counter).unwrap_err();
            let NumericLifecycleError::Evaluation(error) = *failure else {
                panic!("evaluation failure")
            };
            assert_eq!(
                &"NULL + BROKEN + LATER"[error.source_span.start..error.source_span.end],
                "BROKEN"
            );
            let EvaluationErrorKind::Resolution {
                identifier,
                error: Opaque(payload),
            } = error.evaluation.kind
            else {
                panic!("opaque resolution failure")
            };
            assert_eq!((identifier.as_str(), payload), ("BROKEN", 42));
            assert_eq!(resolver.0, ["BROKEN"]);
            assert_eq!(counter.snapshot()[0].count, 0);
        }

        /// A caller publishes only the completed replacement to a dependent numeric plan.
        #[test]
        fn dependent_reads_converted_handler_value() {
            let first = derivation("RAW", ColumnType::Int, Some(Value::Str("7".into())));
            let next = NumericDerivation::new(
                compile_numeric("A + 1", "columns.B.derivation.value.compute").unwrap(),
                ColumnType::Int,
                "columns.B",
                None,
            );
            let mut counter = HandlerCounter::default();
            let mut resolver = Resolver {
                values: [("RAW".into(), Value::float(1.25))].into(),
                trace: Vec::new(),
            };
            let completed = first.evaluate(&mut resolver, &mut counter).unwrap();
            assert_eq!(completed, Value::Int(7));
            resolver.values.insert("A".into(), completed);
            assert_eq!(
                next.evaluate(&mut resolver, &mut counter),
                Ok(Value::Int(8))
            );
            assert_eq!(resolver.trace, ["RAW", "A"]);
            assert_eq!(counter.snapshot()[0].count, 1);
        }
    }
    mod ordinary_key_index {
        //! Independent donor order, missingness and native-effect controls for large keys.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::numeric_lifecycle::HandlerKind;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            dataset::{
                Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
                IntermediateKey, IntermediateSource, Keep, MatchRead, MatchValue, OrderTerm,
                RelationPredicate, RowMode, RowTemplate, SecondarySource, SourceSchemas,
                SourceSelection,
            },
            predicate_compiler,
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType as T, Value},
        };
        #[derive(Debug)]
        struct Original(Box<u8>, Rc<()>);
        type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
        struct Table {
            name: &'static str,
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            trace: Trace,
            failure: RefCell<Option<(usize, usize, Original)>>,
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
                self.trace.borrow_mut().push((self.name, row, column));
                if self
                    .failure
                    .borrow()
                    .as_ref()
                    .is_some_and(|(r, c, _)| (*r, *c) == (row, column))
                {
                    return Err(CellError::Access(
                        self.failure.borrow_mut().take().unwrap().2,
                    ));
                }
                Ok((&self.rows[row][column]).into())
            }
        }
        #[derive(Clone, Copy)]
        enum Mode {
            Ordinary,
            Expression,
            Composite,
            Correlated,
        }
        fn fixture(keep: Keep, mode: Mode, selection: bool) -> (Table, Table, DatasetPlan, Trace) {
            let trace = Trace::default();
            let mut rows = (0..64)
                .map(|i| {
                    vec![
                        Value::Int(i),
                        if i == 0 || i == 10 {
                            Value::Missing
                        } else {
                            Value::Str(format!("K{i:03}"))
                        },
                        Value::Int(1),
                    ]
                })
                .collect::<Vec<_>>();
            rows.push(vec![
                Value::Int(64),
                Value::Str("unmatched".into()),
                Value::Int(1),
            ]);
            let primary = Table {
                name: "BASE",
                schema: schema(&[("ID", T::Int), ("MATCH", T::Str), ("ACTIVE", T::Int)]),
                rows,
                trace: trace.clone(),
                failure: RefCell::default(),
            };
            let mut rows = Vec::new();
            for i in 0..64 {
                for donor in 0..4 {
                    rows.push(vec![
                        Value::Str(format!("K{i:03}")),
                        Value::Int(1),
                        Value::Int(i64::from(donor != 0)),
                        Value::Int(i * 4 + donor),
                    ]);
                }
            }
            let donors = Table {
                name: "DONOR",
                schema: schema(&[
                    ("K", T::Str),
                    ("ORDER", T::Int),
                    ("ACTIVE", T::Int),
                    ("VALUE", T::Int),
                ]),
                rows,
                trace: trace.clone(),
                failure: RefCell::default(),
            };
            let current = MatchRead::Record {
                column: 1,
                identifier: "BASE.MATCH".into(),
            };
            let mut keys = vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].key.K".into(),
                value: if matches!(mode, Mode::Expression) {
                    MatchValue::Source(current)
                } else {
                    MatchValue::Variable(current)
                },
            }];
            if matches!(mode, Mode::Composite) {
                keys.push(IntermediateKey {
                    source_column: 2,
                    path: "intermediates[0].key.ACTIVE".into(),
                    value: MatchValue::Variable(MatchRead::Record {
                        column: 2,
                        identifier: "BASE.ACTIVE".into(),
                    }),
                });
            }
            let correlated = matches!(mode, Mode::Correlated);
            let predicate = predicate_compiler::compile(
                if correlated {
                    "DONOR.ACTIVE = BASE.ACTIVE"
                } else {
                    "DONOR.ACTIVE = 1"
                },
                "intermediates[0].filter",
            )
            .unwrap();
            let mut bindings = vec![Binding {
                name: "DONOR.ACTIVE".into(),
                read: Read::Source(2),
            }];
            let current = if correlated {
                bindings.push(Binding {
                    name: "BASE.ACTIVE".into(),
                    read: Read::Column(0),
                });
                vec![CurrentField {
                    identifier: "BASE.ACTIVE".into(),
                    read: CurrentRead::Record {
                        source: 0,
                        column: 2,
                    },
                }]
            } else {
                vec![]
            };
            let item = Intermediate {
                identifier: "SELECTED".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Input(0),
                keys,
                filter: Some(
                    RelationPredicate::new(
                        BoundPredicate::new(predicate, bindings).unwrap(),
                        current,
                    )
                    .unwrap(),
                ),
                selection: selection.then_some(SourceSelection {
                    order_by: vec![OrderTerm {
                        column: 1,
                        descending: false,
                        nulls_first: false,
                    }],
                    keep,
                }),
                no_match: Some(Value::Missing),
                unique: vec![],
                derivations: None,
                pending_schema: None,
                range: None,
                columns: None,
            };
            let output = schema(&[("ID", T::Int), ("VALUE", T::Int)]);
            let assignments = vec![
                Assignment {
                    column: 0,
                    path: "columns.ID".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: "columns.VALUE".into(),
                    expression: Expression::Intermediate {
                        index: 0,
                        column: 3,
                    },
                },
            ];
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: primary.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "DONOR".into(),
                        schema: donors.schema.clone(),
                    }],
                },
                vec![item],
                output,
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments,
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            (primary, donors, plan, trace)
        }
        fn expected(keep: Keep) -> Vec<Vec<Value>> {
            (0..65)
                .map(|i| {
                    vec![
                        Value::Int(i),
                        if i == 0 || i == 10 || i == 64 {
                            Value::Missing
                        } else {
                            Value::Int(i * 4 + if keep == Keep::First { 1 } else { 3 })
                        },
                    ]
                })
                .collect()
        }
        #[test]
        fn indexed_truth_keeps_filter_order_missingness_and_stable_first_last() {
            for keep in [Keep::First, Keep::Last] {
                for _ in 0..2 {
                    let (primary, donors, plan, trace) = fixture(keep, Mode::Ordinary, true);
                    // Every first donor is filtered out before its key can be inspected.
                    *donors.failure.borrow_mut() =
                        Some((0, 0, Original(Box::new(17), Rc::new(()))));
                    let attempted = plan.execute(
                        &primary,
                        &[&donors],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    );
                    assert_eq!(attempted.result.unwrap().dataset.rows(), expected(keep));
                    let events = trace.borrow();
                    let first_current = events
                        .iter()
                        .position(|event| *event == ("BASE", 0, 1))
                        .unwrap();
                    assert_eq!(
                        events[..first_current]
                            .iter()
                            .filter(|(name, _, col)| *name == "DONOR" && *col == 2)
                            .count(),
                        256
                    );
                    let key_reads = events
                        .iter()
                        .filter(|(name, _, col)| *name == "DONOR" && *col == 0)
                        .copied()
                        .collect::<Vec<_>>();
                    assert_eq!(key_reads.len(), 192);
                    {
                        assert_eq!(
                            key_reads,
                            (0..256)
                                .filter(|r| r % 4 != 0)
                                .map(|r| ("DONOR", r, 0))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
        #[test]
        fn expression_composite_and_correlated_selectors_keep_the_existing_scan() {
            for mode in [Mode::Expression, Mode::Composite, Mode::Correlated] {
                for _ in 0..2 {
                    let (primary, donors, plan, trace) = fixture(Keep::First, mode, true);
                    assert_eq!(
                        plan.execute(
                            &primary,
                            &[&donors],
                            &mut yamaa_core::function_invocation::NoFunctions::new(),
                            None
                        )
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                        expected(Keep::First)
                    );
                    let key_reads = trace
                        .borrow()
                        .iter()
                        .filter(|(name, _, col)| *name == "DONOR" && *col == 0)
                        .count();
                    assert_eq!(
                        key_reads,
                        if matches!(mode, Mode::Correlated) {
                            256 * 63
                        } else {
                            192 * 63
                        }
                    );
                }
            }
        }
        #[test]
        fn first_donor_failure_retains_original_identity_and_the_exact_scan_prefix() {
            let mut traces = Vec::new();
            for _ in 0..2 {
                let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
                let payload = Original(Box::new(29), Rc::new(()));
                let pointer = payload.0.as_ref() as *const u8;
                let marker = Rc::clone(&payload.1);
                *donors.failure.borrow_mut() = Some((130, 0, payload));
                let attempted = plan.execute(
                    &primary,
                    &[&donors],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let ExecutionError::SecondaryCell {
                    source: 0,
                    source_row: 130,
                    error: CellError::Access(error),
                    ..
                } = *attempted.result.unwrap_err()
                else {
                    panic!("original donor failure")
                };
                assert_eq!(error.0.as_ref() as *const u8, pointer);
                assert!(Rc::ptr_eq(&error.1, &marker));
                traces.push(trace.borrow().clone());
            }
            assert_eq!(traces[0], traces[1]);
        }
        #[test]
        fn multiple_match_refusal_reads_no_order_or_payload_and_builds_no_result() {
            for _ in 0..2 {
                let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, false);
                let attempted = plan.execute(
                    &primary,
                    &[&donors],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                assert!(matches!(
                    *attempted.result.unwrap_err(),
                    ExecutionError::MultipleMatches { match_count: 3, .. }
                ));
                assert!(!trace
                    .borrow()
                    .iter()
                    .any(|(name, _, col)| *name == "DONOR" && matches!(col, 1 | 3)));
            }
        }

        #[test]
        fn missing_donor_keys_and_selected_missing_payload_keep_distinct_handler_outcomes() {
            for keep in [Keep::First, Keep::Last] {
                let mut observations = Vec::new();
                for _ in 0..2 {
                    let (primary, mut donors, plan, _) = fixture(keep, Mode::Ordinary, true);
                    for row in [5, 6, 7] {
                        donors.rows[row][0] = Value::Missing;
                    }
                    // A present selected record with a missing field is still a match.
                    for row in [9, 11] {
                        donors.rows[row][3] = Value::Missing;
                    }
                    let attempted = plan.execute(
                        &primary,
                        &[&donors],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    );
                    let mut truth = expected(keep);
                    truth[1][1] = Value::Missing;
                    truth[2][1] = Value::Missing;
                    assert_eq!(attempted.result.unwrap().dataset.rows(), truth);
                    assert_eq!(
                        attempted
                            .handler_counts
                            .iter()
                            .find(|entry| entry.handler == HandlerKind::NoMatch)
                            .unwrap()
                            .count,
                        4
                    );
                    assert_eq!(
                        attempted
                            .handler_counts
                            .iter()
                            .find(|entry| entry.handler == HandlerKind::MultipleMatches)
                            .unwrap()
                            .count,
                        61
                    );
                    observations.push(attempted.handler_counts);
                }
                assert_eq!(observations[0], observations[1]);
            }
        }

        #[test]
        fn entire_source_filter_finishes_before_current_key_and_donor_key_effects() {
            let mut traces = Vec::new();
            for _ in 0..2 {
                let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
                let original = Original(Box::new(41), Rc::new(()));
                let pointer = original.0.as_ref() as *const u8;
                let marker = Rc::clone(&original.1);
                *donors.failure.borrow_mut() = Some((130, 2, original));
                *primary.failure.borrow_mut() = Some((0, 1, Original(Box::new(43), Rc::new(()))));
                let attempted = plan.execute(
                    &primary,
                    &[&donors],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let ExecutionError::Predicate {
                    source_row: 130,
                    error,
                } = *attempted.result.unwrap_err()
                else {
                    panic!("source-only filter keeps its original failure")
                };
                let yamaa_core::predicate::ErrorKind::Resolution {
                    error: CellError::Access(original),
                    ..
                } = error.kind
                else {
                    panic!("original predicate operand failure")
                };
                assert_eq!(original.0.as_ref() as *const u8, pointer);
                assert!(Rc::ptr_eq(&original.1, &marker));
                assert!(primary.failure.borrow().is_some());
                assert!(!trace.borrow().iter().any(|(name, _, column)| {
                    (*name == "BASE" && *column == 1) || (*name == "DONOR" && *column != 2)
                }));
                traces.push(trace.borrow().clone());
            }
            assert_eq!(traces[0], traces[1]);
        }

        #[test]
        fn failing_current_key_reads_no_donor_key_order_or_payload() {
            let mut traces = Vec::new();
            for _ in 0..2 {
                let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
                let original = Original(Box::new(47), Rc::new(()));
                let pointer = original.0.as_ref() as *const u8;
                let marker = Rc::clone(&original.1);
                *primary.failure.borrow_mut() = Some((1, 1, original));
                *donors.failure.borrow_mut() = Some((130, 0, Original(Box::new(53), Rc::new(()))));
                let attempted = plan.execute(
                    &primary,
                    &[&donors],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                );
                let ExecutionError::Cell {
                    source_row: 1,
                    error: CellError::Access(original),
                    ..
                } = *attempted.result.unwrap_err()
                else {
                    panic!("current key keeps its original failure")
                };
                assert_eq!(original.0.as_ref() as *const u8, pointer);
                assert!(Rc::ptr_eq(&original.1, &marker));
                assert!(donors.failure.borrow().is_some());
                assert!(!trace
                    .borrow()
                    .iter()
                    .any(|(name, _, column)| *name == "DONOR" && *column != 2));
                traces.push(trace.borrow().clone());
            }
            assert_eq!(traces[0], traces[1]);
        }
    }
    mod predicate_diagnostics {
        //! Predicate projection inspects one completed native failure without port effects.
        use serde_json::Value as JsonValue;
        use std::cell::RefCell;
        use yamaa_core::dataset::{
            Assignment, DatasetPlan, ExecutionError, Expression, RowMode, RowTemplate,
        };
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            diagnostic::PredicateDiagnosticError,
            predicate::{Comparison, ErrorKind, Node, Plan, Scalar},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        // Neither Clone nor Debug is available to diagnostic code.
        struct Opaque(Box<u8>);
        struct Source {
            schema: TableSchema,
            error: RefCell<Option<Opaque>>,
            reads: RefCell<Vec<(usize, usize)>>,
        }
        impl TableAccess for Source {
            type Error = Opaque;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                2
            }
            fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Opaque>> {
                self.reads.borrow_mut().push((row, column));
                if let Some(error) = self.error.borrow_mut().take() {
                    Err(CellError::Access(error))
                } else {
                    Ok(ValueRef::Int(7))
                }
            }
        }
        fn source(error: Option<Opaque>) -> Source {
            Source {
                schema: TableSchema::new(vec![Column {
                    name: "A".into(),
                    kind: ColumnType::Int,
                }])
                .unwrap(),
                error: RefCell::new(error),
                reads: RefCell::default(),
            }
        }
        fn plan(schema: &TableSchema, right: Value) -> DatasetPlan {
            let filter = BoundPredicate::new(
                Plan::new(
                    vec![Node::Compare {
                        operator: Comparison::Equal,
                        left: Scalar::Identifier("A".into()),
                        right: Scalar::Literal(right),
                    }],
                    0,
                    "rows[0].filter".into(),
                    "A = original\0🙂".into(),
                )
                .unwrap(),
                vec![Binding {
                    name: "A".into(),
                    read: Read::Source(0),
                }],
            )
            .unwrap();
            DatasetPlan::new(
                schema.clone(),
                schema.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![],
                    filter: Some(filter),
                }],
                vec![Assignment {
                    column: 0,
                    expression: Expression::Source(0),
                    path: "columns.A.derivation".into(),
                }],
                vec![0],
                vec![],
            )
            .unwrap()
        }
        #[test]
        fn opaque_cell_identity_survives_repeated_projection_without_retry_or_later_reads() {
            let payload = Opaque(Box::new(91));
            let address = &*payload.0 as *const u8;
            let table = source(Some(payload));
            let plan = plan(&table.schema, Value::Int(7));
            assert!(table.reads.borrow().is_empty());
            let Err(failure) = plan
                .execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
            else {
                panic!("source failure must abort")
            };
            let ExecutionError::Predicate { source_row, error } = &*failure else {
                panic!("predicate source failure")
            };
            assert_eq!(*source_row, 0);
            assert_eq!(error.spec_path, "rows[0].filter");
            assert_eq!(error.expression, "A = original\0🙂");
            for _ in 0..3 {
                let Err(PredicateDiagnosticError::Resolution(CellError::Access(payload))) =
                    error.diagnostic()
                else {
                    panic!("original resolver failure is retained during repeated projection")
                };
                assert_eq!(&*payload.0 as *const u8, address);
            }
            let ErrorKind::Resolution {
                identifier,
                error: CellError::Access(payload),
            } = &error.kind
            else {
                panic!("original cell error retained")
            };
            assert_eq!(identifier, "A");
            assert_eq!(&*payload.0 as *const u8, address);
            assert_eq!(*table.reads.borrow(), [(0, 0)]);
        }
        #[test]
        fn semantic_filter_failure_precedes_assignments_and_the_next_source_record() {
            let table = source(None);
            let plan = plan(&table.schema, Value::Str("incompatible".into()));
            let Err(failure) = plan
                .execute(
                    &table,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
            else {
                panic!("incompatible comparison must abort")
            };
            let ExecutionError::Predicate { source_row, error } = &*failure else {
                panic!("predicate semantic failure")
            };
            assert_eq!(*source_row, 0);
            let diagnostic = error
                .diagnostic()
                .unwrap_or_else(|_| panic!("semantic finding"));
            assert_eq!(diagnostic.definition().phase, "validation");
            assert_eq!(diagnostic.definition().condition, "incompatible_input_type");
            assert_eq!(diagnostic.definition().requirement, Some("REQ-0190"));
            assert_eq!(diagnostic.spec_paths, ["rows[0].filter"]);
            assert_eq!(
                diagnostic.context,
                [
                    ("left_type".into(), JsonValue::String("int".into())),
                    ("right_type".into(), JsonValue::String("str".into())),
                ]
                .into()
            );
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, Some(vec![]));
            assert_eq!(
                error
                    .diagnostic()
                    .unwrap_or_else(|_| panic!("retained finding")),
                diagnostic
            );
            assert_eq!(*table.reads.borrow(), [(0, 0)]);
        }
    }
    mod primary_intermediates {
        //! Primary donors borrow the original table even while a secondary drives construction.
        use crate::support::table_schema as schema;
        use std::cell::RefCell;
        use yamaa_core::{
            dataset::{
                Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey,
                IntermediateSource, MatchRead, MatchValue, RowMode, RowTemplate, SecondarySource,
                SourceSchemas,
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

        fn fixture(failure: Option<(usize, usize)>) -> (Table, Table, DatasetPlan) {
            let primary = Table {
                schema: schema(&[("ID", T::Int), ("V", T::Str)]),
                rows: vec![
                    vec![Value::Int(1), Value::Str("x".into())],
                    vec![Value::Int(2), Value::Str("y".into())],
                ],
                reads: RefCell::default(),
                failure,
            };
            let other = Table {
                schema: schema(&[("ID", T::Int)]),
                rows: vec![vec![Value::Int(2)], vec![Value::Int(1)]],
                reads: RefCell::default(),
                failure: None,
            };
            let item = Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Primary {
                    schema: primary.schema.clone(),
                    name: "SRC".into(),
                },
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
                no_match: None,
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,

                range: None,
                columns: None,
            };
            let read = || Expression::Intermediate {
                index: 0,
                column: 1,
            };
            let plan = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: primary.schema.clone(),
                    secondary: vec![SecondarySource {
                        name: "ALT".into(),
                        schema: other.schema.clone(),
                    }],
                },
                vec![item],
                schema(&[("ID", T::Int), ("A", T::Str), ("B", T::Str)]),
                vec![RowTemplate {
                    mode: RowMode::Driver {
                        source: 1,
                        mode: Box::new(RowMode::Records),
                    },
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(1, read()),
                        assign(2, read()),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            (primary, other, plan)
        }
        #[test]
        fn one_selection_per_current_row_reads_the_original_relation_without_copies() {
            let (primary, other, plan) = fixture(None);
            let result = plan.execute(
                &primary,
                &[&other],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(result.handler_counts.is_empty());
            assert_eq!(
                result.result.unwrap().dataset.rows(),
                [
                    vec![
                        Value::Int(2),
                        Value::Str("y".into()),
                        Value::Str("y".into())
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("x".into()),
                        Value::Str("x".into())
                    ]
                ]
            );
            assert_eq!(
                *primary.reads.borrow(),
                [
                    (0, 0),
                    (1, 0),
                    (1, 1),
                    (1, 1),
                    (0, 0),
                    (1, 0),
                    (0, 1),
                    (0, 1)
                ]
            );
            assert_eq!(*other.reads.borrow(), [(0, 0), (1, 0)]);
        }
        #[test]
        fn a_primary_donor_failure_keeps_its_original_owner_and_exact_read_prefix() {
            let (primary, other, plan) = fixture(Some((1, 0)));
            let result = plan.execute(
                &primary,
                &[&other],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let error = result.result.unwrap_err();
            assert!(
                matches!(*error, yamaa_core::dataset::ExecutionError::Cell { ref path, source_row: 1, error: CellError::Access("held primary failure") } if path == "intermediates[0]")
            );
            assert_eq!(*primary.reads.borrow(), [(0, 0), (1, 0)]);
            assert_eq!(*other.reads.borrow(), [(0, 0)]);
            assert!(result.handler_counts.is_empty());
        }
    }
    mod row_derivations {
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            dataset::{
                Assignment, CaseBranch, DatasetPlan, Expression, FirstAvailable, OrderTerm,
                RowMode, RowTemplate, SecondarySource, SelectionRead, SelectionSource, Window,
                WindowKind,
            },
            table::{CellError, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
            reads: RefCell<Vec<(usize, usize)>>,
            failure: Option<(usize, usize, Rc<str>)>,
        }
        impl TableAccess for Table {
            type Error = Rc<str>;
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
                if let Some((r, c, error)) = &self.failure {
                    if (*r, *c) == (row, column) {
                        return Err(CellError::Access(error.clone()));
                    }
                }
                Ok((&self.rows[row][column]).into())
            }
        }
        fn assign(column: usize, expression: Expression, path: &str) -> Assignment {
            Assignment {
                column,
                expression,
                path: path.into(),
            }
        }
        #[test]
        fn grouped_filter_removes_candidates_before_numbering_windows() {
            let source = schema(&[("ID", ColumnType::Int)]);
            let table = Table {
                schema: source.clone(),
                rows: [1, 1, 2, 3, 3]
                    .into_iter()
                    .map(|id| vec![Value::Int(id)])
                    .collect(),
                reads: RefCell::default(),
                failure: None,
            };
            let filter = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("N > 1", "row.filter").unwrap(),
                vec![Binding {
                    name: "N".into(),
                    read: Read::Column(1),
                }],
            )
            .unwrap();
            let plan = DatasetPlan::new(
                source,
                schema(&[
                    ("ID", ColumnType::Int),
                    ("N", ColumnType::Int),
                    ("SEQ", ColumnType::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assign(0, Expression::Source(0), "row.ID"),
                        assign(
                            1,
                            Expression::Count {
                                column: None,
                                text: "COUNT(*)".into(),
                            },
                            "row.N",
                        ),
                        assign(
                            2,
                            Expression::Window(Window {
                                kind: WindowKind::RowNumber,
                                group_by: vec![],
                                order_by: vec![OrderTerm {
                                    column: 0,
                                    descending: false,
                                    nulls_first: false,
                                }],
                                filter: None,
                            }),
                            "row.SEQ",
                        ),
                    ],
                    filter: Some(filter),
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
                .rows(),
                [
                    vec![Value::Int(1), Value::Int(2), Value::Int(1)],
                    vec![Value::Int(3), Value::Int(2), Value::Int(2)],
                ]
            );
        }

        #[test]
        fn grouped_filter_uses_one_eligibility_pass_before_windows() {
            struct OnceTable {
                schema: TableSchema,
                reads: RefCell<Vec<(usize, usize)>>,
                payload: Rc<str>,
            }
            impl TableAccess for OnceTable {
                type Error = Rc<str>;
                fn schema(&self) -> &TableSchema {
                    &self.schema
                }
                fn row_count(&self) -> usize {
                    3
                }
                fn cell(
                    &self,
                    row: usize,
                    column: usize,
                ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                    let mut reads = self.reads.borrow_mut();
                    let repeated = reads.contains(&(row, column));
                    reads.push((row, column));
                    match column {
                        0 => Ok(ValueRef::Int(row as i64 + 1)),
                        1 if repeated => Err(CellError::Access(Rc::clone(&self.payload))),
                        1 => Ok(ValueRef::Int(if row == 1 { 0 } else { 1 })),
                        _ => panic!("unexpected source column"),
                    }
                }
            }
            let source = schema(&[("ID", ColumnType::Int), ("KEEP", ColumnType::Int)]);
            let table = OnceTable {
                schema: source.clone(),
                reads: RefCell::default(),
                payload: Rc::from("filter source was revisited"),
            };
            let filter = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("KEEP > 0", "row.filter").unwrap(),
                vec![Binding {
                    name: "KEEP".into(),
                    read: Read::Column(1),
                }],
            )
            .unwrap();
            let plan = DatasetPlan::new(
                source,
                schema(&[
                    ("ID", ColumnType::Int),
                    ("KEEP", ColumnType::Int),
                    ("SEQ", ColumnType::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    assignments: vec![
                        assign(0, Expression::Source(0), "row.ID"),
                        assign(
                            1,
                            Expression::Reduce {
                                identifier: None,
                                column: 1,
                                reducer: yamaa_core::reduction::NumericReducer::Sum,
                                text: "SUM(SRC.KEEP)".into(),
                            },
                            "row.KEEP",
                        ),
                        assign(
                            2,
                            Expression::Window(Window {
                                kind: WindowKind::RowNumber,
                                group_by: vec![],
                                order_by: vec![OrderTerm {
                                    column: 0,
                                    descending: false,
                                    nulls_first: false,
                                }],
                                filter: None,
                            }),
                            "row.SEQ",
                        ),
                    ],
                    filter: Some(filter),
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
            assert!(attempt.handler_counts.is_empty() && attempt.retained_verifications.is_empty());
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [
                    vec![Value::Int(1), Value::Int(1), Value::Int(1)],
                    vec![Value::Int(3), Value::Int(1), Value::Int(2)]
                ],
            );
            assert_eq!(
                *table.reads.borrow(),
                [
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (0, 0),
                    (0, 1),
                    (1, 0),
                    (1, 1),
                    (2, 0),
                    (2, 1)
                ],
            );
        }

        #[test]
        fn windows_number_each_template_after_filter_without_revisiting_sources() {
            let source = schema(&[("ID", ColumnType::Int)]);
            let table = Table {
                schema: source.clone(),
                rows: vec![
                    vec![Value::Int(3)],
                    vec![Value::Int(1)],
                    vec![Value::Int(2)],
                ],
                reads: RefCell::default(),
                failure: None,
            };
            let output = schema(&[
                ("ID", ColumnType::Int),
                ("SECTION", ColumnType::Str),
                ("SEQ", ColumnType::Int),
                ("NEXT", ColumnType::Int),
            ]);
            let window = |kind| {
                Expression::Window(Window {
                    kind,
                    group_by: vec![],
                    order_by: vec![OrderTerm {
                        column: 0,
                        descending: false,
                        nulls_first: false,
                    }],
                    filter: None,
                })
            };
            let filter = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("ID > 1", "row.filter").unwrap(),
                vec![Binding {
                    name: "ID".into(),
                    read: Read::Column(0),
                }],
            )
            .unwrap();
            let template = |section: &str, filter| RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0), "row.ID"),
                    assign(
                        1,
                        Expression::Literal(Value::Str(section.into())),
                        "row.SECTION",
                    ),
                    assign(2, window(WindowKind::RowNumber), "row.SEQ"),
                    assign(
                        3,
                        Expression::Window(Window {
                            kind: WindowKind::RowValue {
                                column: 2,
                                offset: 1,
                            },
                            group_by: vec![],
                            order_by: vec![OrderTerm {
                                column: 2,
                                descending: false,
                                nulls_first: false,
                            }],
                            filter: None,
                        }),
                        "row.NEXT",
                    ),
                ],
                filter,
            };
            let plan = DatasetPlan::new(
                source,
                output,
                vec![template("all", None), template("screened", Some(filter))],
                vec![],
                vec![0, 1],
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
            assert_eq!(
                result.dataset.rows(),
                [
                    vec![
                        Value::Int(3),
                        Value::Str("all".into()),
                        Value::Int(3),
                        Value::Missing
                    ],
                    vec![
                        Value::Int(1),
                        Value::Str("all".into()),
                        Value::Int(1),
                        Value::Int(2)
                    ],
                    vec![
                        Value::Int(2),
                        Value::Str("all".into()),
                        Value::Int(2),
                        Value::Int(3)
                    ],
                    vec![
                        Value::Int(3),
                        Value::Str("screened".into()),
                        Value::Int(2),
                        Value::Missing
                    ],
                    vec![
                        Value::Int(2),
                        Value::Str("screened".into()),
                        Value::Int(1),
                        Value::Int(2)
                    ]
                ]
            );
            assert_eq!(
                *table.reads.borrow(),
                [(0, 0), (1, 0), (2, 0), (0, 0), (1, 0), (2, 0)]
            );
        }
        #[test]
        fn nested_case_selection_keeps_authored_reads_and_original_failure() {
            let source = schema(&[
                ("ID", ColumnType::Int),
                ("FIRST", ColumnType::Str),
                ("LAST", ColumnType::Str),
            ]);
            let payload: Rc<str> = Rc::from("original source failure");
            let table = Table {
                schema: source.clone(),
                rows: vec![
                    vec![
                        Value::Int(1),
                        Value::Str("first".into()),
                        Value::Str("unread".into()),
                    ],
                    vec![Value::Int(2), Value::Missing, Value::Str("failed".into())],
                ],
                reads: RefCell::default(),
                failure: Some((1, 2, payload.clone())),
            };
            let output = schema(&[("ID", ColumnType::Int), ("RESULT", ColumnType::Str)]);
            let path = "rows[0].derivations.RESULT.case[0].then.first_available";
            let predicate = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("ID > 0", "row.case.when").unwrap(),
                vec![Binding {
                    name: "ID".into(),
                    read: Read::Column(0),
                }],
            )
            .unwrap();
            let selection = Expression::FirstAvailable(Box::new(FirstAvailable::new(
                [1, 2]
                    .iter()
                    .enumerate()
                    .map(|(index, column)| SelectionSource {
                        path: format!("{path}.sources[{index}]"),
                        read: SelectionRead::Source(*column),
                    })
                    .collect(),
                Value::Missing,
            )));
            let plan = DatasetPlan::new(
                source,
                output,
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0), "row.ID"),
                        assign(
                            1,
                            Expression::Case(vec![CaseBranch {
                                when: Some(predicate),
                                assignment: Box::new(assign(1, selection, path)),
                            }]),
                            "row.RESULT",
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
                &table,
                &[],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let error = attempt.result.unwrap_err();
            let ExecutionError::Cell {
                path: actual,
                source_row,
                error: CellError::Access(held),
            } = *error
            else {
                panic!("source error changed")
            };
            assert_eq!(actual, format!("{path}.sources[1]"));
            assert_eq!(source_row, 1);
            assert!(Rc::ptr_eq(&held, &payload));
            assert_eq!(
                *table.reads.borrow(),
                [(0, 0), (0, 1), (1, 0), (1, 1), (1, 2)]
            );
            assert!(attempt.retained_verifications.is_empty());
        }

        #[test]
        fn unselected_nested_function_requires_activation_before_every_table_method() {
            use yamaa_core::{
                dataset::BoundFunction,
                function_signature::{FunctionIdentity, InvocationPlan},
            };
            let source = schema(&[("ID", ColumnType::Int)]);
            let table = Table {
                schema: source.clone(),
                rows: vec![vec![Value::Int(1)]],
                reads: RefCell::default(),
                failure: None,
            };
            let predicate = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("ID < 0", "case.when").unwrap(),
                vec![Binding {
                    name: "ID".into(),
                    read: Read::Column(0),
                }],
            )
            .unwrap();
            let signature = InvocationPlan::new(
                FunctionIdentity {
                    name: "id".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "project.id".into(),
                },
                vec![],
                ColumnType::Int,
                false,
            )
            .unwrap();
            let function = Expression::Function(BoundFunction::new(0, signature, vec![]).unwrap());
            let plan = DatasetPlan::new(
                source.clone(),
                schema(&[("ID", ColumnType::Int), ("RESULT", ColumnType::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0), "row.ID"),
                        assign(
                            1,
                            Expression::Case(vec![CaseBranch {
                                when: Some(predicate),
                                assignment: Box::new(assign(
                                    1,
                                    function,
                                    "row.RESULT.case[0].then.function",
                                )),
                            }]),
                            "row.RESULT",
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
                        &table,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::FunctionBinding { slot: 0 }
            ));
            assert!(table.reads.borrow().is_empty());
        }

        #[test]
        fn distinct_driver_failure_retains_its_source_coordinate_payload_and_entered_prefix() {
            let primary = Table {
                schema: schema(&[("ID", ColumnType::Int)]),
                rows: vec![vec![Value::Int(1)], vec![Value::Int(2)]],
                reads: RefCell::default(),
                failure: None,
            };
            let payload: Rc<str> = "secondary retained failure".into();
            let secondary = Table {
                schema: schema(&[
                    ("PAD", ColumnType::Str),
                    ("ID", ColumnType::Int),
                    ("VALUE", ColumnType::Float),
                ]),
                rows: vec![
                    vec![Value::Str("x".into()), Value::Int(10), Value::float(3.0)],
                    vec![Value::Str("y".into()), Value::Int(20), Value::float(4.0)],
                ],
                reads: RefCell::default(),
                failure: Some((1, 2, payload.clone())),
            };
            let assignments = |id, value| {
                vec![
                    assign(0, Expression::Source(id), "rows.ID.source"),
                    assign(1, value, "rows.VALUE.source"),
                ]
            };
            let plan = DatasetPlan::new_with_sources(
                primary.schema.clone(),
                vec![SecondarySource {
                    name: "SECOND".into(),
                    schema: secondary.schema.clone(),
                }],
                schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Float)]),
                vec![
                    RowTemplate {
                        mode: RowMode::Records,
                        assignments: assignments(0, Expression::Literal(Value::float(1.0))),
                        filter: None,
                    },
                    RowTemplate {
                        mode: RowMode::Driver {
                            source: 1,
                            mode: Box::new(RowMode::Records),
                        },
                        assignments: assignments(1, Expression::Source(2)),
                        filter: None,
                    },
                ],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let attempt = plan.execute(
                &primary,
                &[&secondary],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let error = attempt.result.unwrap_err();
            let ExecutionError::Cell {
                path,
                source_row,
                error: CellError::Access(actual),
            } = *error
            else {
                panic!("original cell cause")
            };
            assert_eq!(path, "rows.VALUE.source");
            assert_eq!(source_row, 1);
            assert!(Rc::ptr_eq(&actual, &payload));
            assert_eq!(*primary.reads.borrow(), [(0, 0), (1, 0)]);
            assert_eq!(*secondary.reads.borrow(), [(0, 1), (0, 2), (1, 1), (1, 2)]);
            assert!(attempt.retained_verifications.is_empty());
        }

        /// Container reads run after the template window without an extra source pass.
        #[test]
        fn wrapped_first_available_reads_the_completed_template_window() {
            let source = schema(&[("ID", ColumnType::Int)]);
            let table = Table {
                schema: source.clone(),
                rows: vec![
                    vec![Value::Int(3)],
                    vec![Value::Int(1)],
                    vec![Value::Int(2)],
                ],
                reads: RefCell::default(),
                failure: None,
            };
            let plan = DatasetPlan::new(
                source,
                schema(&[
                    ("ID", ColumnType::Int),
                    ("SEQ", ColumnType::Int),
                    ("SELECTED", ColumnType::Int),
                ]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0), "row.ID"),
                        assign(
                            1,
                            Expression::Window(Window {
                                kind: WindowKind::RowNumber,
                                group_by: vec![],
                                order_by: vec![OrderTerm {
                                    column: 0,
                                    descending: false,
                                    nulls_first: false,
                                }],
                                filter: None,
                            }),
                            "row.SEQ",
                        ),
                        assign(
                            2,
                            Expression::FirstAvailable(Box::new(FirstAvailable::new(
                                vec![SelectionSource {
                                    path: "row.SELECTED.sources[0]".into(),
                                    read: SelectionRead::Expression(Box::new(assign(
                                        2,
                                        Expression::SourceBinding {
                                            source: Box::new(assign(
                                                2,
                                                Expression::Column(1),
                                                "row.SELECTED.source",
                                            )),
                                            absent: None,
                                            variable_absent: false,
                                            selection_declared: false,
                                            variable: "SEQ".into(),
                                            path: "row.SELECTED.source".into(),
                                        },
                                        "row.SELECTED.source",
                                    ))),
                                }],
                                Value::Int(99),
                            ))),
                            "row.SELECTED",
                        ),
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
            assert_eq!(
                result.dataset.rows(),
                [
                    vec![Value::Int(3), Value::Int(3), Value::Int(3)],
                    vec![Value::Int(1), Value::Int(1), Value::Int(1)],
                    vec![Value::Int(2), Value::Int(2), Value::Int(2)]
                ]
            );
            assert_eq!(*table.reads.borrow(), [(0, 0), (1, 0), (2, 0)]);
        }

        /// An unselected source-selection callback is checked before every table method.
        #[test]
        fn wrapped_selection_cannot_hide_an_unactivated_function() {
            use yamaa_core::{
                dataset::BoundFunction,
                function_signature::{FunctionIdentity, InvocationPlan},
            };
            struct Untouched;
            impl TableAccess for Untouched {
                type Error = Rc<str>;
                fn schema(&self) -> &TableSchema {
                    panic!("schema entered before function admission")
                }
                fn row_count(&self) -> usize {
                    panic!("cardinality entered before function admission")
                }
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                    panic!("cell entered before function admission")
                }
            }
            let signature = InvocationPlan::new(
                FunctionIdentity {
                    name: "id".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "project.id".into(),
                },
                vec![],
                ColumnType::Int,
                false,
            )
            .unwrap();
            let selection = Expression::FirstAvailable(Box::new(FirstAvailable::new(
                vec![
                    SelectionSource {
                        path: "row.RESULT.sources[0]".into(),
                        read: SelectionRead::Expression(Box::new(assign(
                            1,
                            Expression::Literal(Value::Int(7)),
                            "row.RESULT.literal",
                        ))),
                    },
                    SelectionSource {
                        path: "row.RESULT.sources[1]".into(),
                        read: SelectionRead::Expression(Box::new(assign(
                            1,
                            Expression::SourceBinding {
                                source: Box::new(assign(
                                    1,
                                    Expression::Case(vec![CaseBranch {
                                        when: Some(
                                            BoundPredicate::new(
                                                yamaa_core::predicate_compiler::compile(
                                                    "1 = 1",
                                                    "row.RESULT.case.when",
                                                )
                                                .unwrap(),
                                                vec![],
                                            )
                                            .unwrap(),
                                        ),
                                        assignment: Box::new(assign(
                                            1,
                                            Expression::Function(
                                                BoundFunction::new(0, signature, vec![]).unwrap(),
                                            ),
                                            "row.RESULT.function",
                                        )),
                                    }]),
                                    "row.RESULT.case",
                                )),
                                absent: None,
                                variable_absent: false,
                                selection_declared: false,
                                variable: "FUNCTION".into(),
                                path: "row.RESULT.source".into(),
                            },
                            "row.RESULT.source",
                        ))),
                    },
                ],
                Value::Missing,
            )));
            let plan = DatasetPlan::new(
                schema(&[("ID", ColumnType::Int)]),
                schema(&[("ID", ColumnType::Int), ("RESULT", ColumnType::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0), "row.ID"),
                        assign(1, selection, "row.RESULT"),
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
                        &Untouched,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None
                    )
                    .result
                    .unwrap_err(),
                ExecutionError::FunctionBinding { slot: 0 }
            ));
        }
    }
    mod row_intermediate_context {
        //! Complete record attempts have stable choices and separate diagnostic positions.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};
        use yamaa_core::dataset::ExecutionError;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            dataset::{
                Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, MatchRead,
                MatchValue, RowMode, RowTemplate, SecondarySource, SourceSchemas,
            },
            predicate::{Comparison, Node, Scalar},
            schema::{Document, DocumentNode as N, SpecificationDocument},
            specification::{PreparedSpecification, SourceDeclaration},
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
            fail: RefCell<Option<(usize, usize, Failure)>>,
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
                    .is_some_and(|(r, c, _)| *r == row && *c == column)
                {
                    return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().2));
                }
                Ok(ValueRef::from(&self.rows[row][column]))
            }
        }
        fn tables() -> (Table, Table, Trace) {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let text = |s: &str| Value::Str(s.into());
            (
                Table {
                    name: "SRC",
                    schema: schema(&[("ID", T::Str), ("K", T::Str), ("KEEP", T::Str)]),
                    rows: vec![
                        vec![text("a"), text("x"), text("0")],
                        vec![text("b"), text("y"), text("1")],
                        vec![text("c"), text("z"), text("0")],
                    ],
                    fail: RefCell::default(),
                    trace: trace.clone(),
                },
                Table {
                    name: "DONOR",
                    schema: schema(&[("K", T::Str), ("V", T::Str)]),
                    rows: vec![vec![text("x"), text("X")], vec![text("y"), text("Y")]],
                    fail: RefCell::default(),
                    trace: trace.clone(),
                },
                trace,
            )
        }
        fn filter(value: &str) -> BoundPredicate {
            BoundPredicate::new(
                yamaa_core::predicate::Plan::new(
                    vec![Node::Compare {
                        operator: Comparison::Equal,
                        left: Scalar::Identifier("SRC.KEEP".into()),
                        right: Scalar::Literal(Value::Str(value.into())),
                    }],
                    0,
                    "rows.filter".into(),
                    format!("SRC.KEEP = '{value}'"),
                )
                .unwrap(),
                vec![Binding {
                    name: "SRC.KEEP".into(),
                    read: Read::Source(2),
                }],
            )
            .unwrap()
        }
        fn plan(
            src: &Table,
            donor: &Table,
            same_record: bool,
            target: T,
            absence: bool,
        ) -> DatasetPlan {
            let assign = |column, expression, path: String| Assignment {
                column,
                expression,
                path,
            };
            let templates = ["L", "H"]
                .iter()
                .enumerate()
                .map(|(index, section)| {
                    let path = |name| format!("rows[{index}].derivations.{name}.source");
                    RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0), path("ID")),
                            assign(
                                1,
                                Expression::Literal(Value::Str((*section).into())),
                                path("SECTION"),
                            ),
                            assign(
                                2,
                                if same_record {
                                    Expression::Literal(Value::Str(
                                        if index == 0 { "x" } else { "y" }.into(),
                                    ))
                                } else {
                                    Expression::Source(1)
                                },
                                path("KEY"),
                            ),
                            assign(
                                3,
                                Expression::Intermediate {
                                    index: 0,
                                    column: 1,
                                },
                                path("ROWV"),
                            ),
                        ],
                        filter: (!same_record).then(|| filter(if index == 0 { "0" } else { "1" })),
                    }
                })
                .collect();
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
                    source: yamaa_core::dataset::IntermediateSource::Input(0),
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "intermediates[0].key".into(),
                        value: MatchValue::Variable(MatchRead::Output {
                            column: 2,
                            identifier: "KEY".into(),
                        }),
                    }],
                    filter: None,
                    selection: None,
                    no_match: absence.then_some(Value::Missing),
                    unique: Vec::new(),
                    derivations: None,
                    pending_schema: None,

                    range: None,
                    columns: None,
                }],
                schema(&[
                    ("ID", T::Str),
                    ("SECTION", T::Str),
                    ("KEY", T::Str),
                    ("ROWV", target),
                    ("COLV", T::Str),
                ]),
                templates,
                vec![assign(
                    4,
                    Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                    "columns.COLV.derivation.source".into(),
                )],
                vec![0, 1],
                vec![],
            )
            .unwrap()
        }
        #[test]
        fn discarded_attempts_never_reuse_choices_and_retained_columns_reuse_their_token() {
            let (src, donor, trace) = tables();
            let attempt = plan(&src, &donor, false, T::Str, true).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let t = |s: &str| Value::Str(s.into());
            assert_eq!(
                attempt.result.unwrap().dataset.rows(),
                [
                    vec![t("a"), t("L"), t("x"), t("X"), t("X")],
                    vec![t("c"), t("L"), t("z"), Value::Missing, Value::Missing],
                    vec![t("b"), t("H"), t("y"), t("Y"), t("Y")]
                ]
            );
            assert_eq!(
                attempt
                    .handler_counts
                    .iter()
                    .map(|h| (h.spec_path.as_str(), h.handler.name(), h.count))
                    .collect::<Vec<_>>(),
                [
                    ("rows[0].derivations.ROWV.source.no_match", "no_match", 1),
                    ("rows[1].derivations.ROWV.source.no_match", "no_match", 1),
                    ("columns.COLV.derivation.source.no_match", "no_match", 1)
                ]
            );
            assert_eq!(
                trace
                    .borrow()
                    .iter()
                    .filter(|&&(name, _, col)| name == "DONOR" && col == 0)
                    .count(),
                12,
                "six choices, no extra column selection"
            );
            assert_eq!(
                trace
                    .borrow()
                    .iter()
                    .filter(|&&(name, _, col)| name == "DONOR" && col == 1)
                    .count(),
                6,
                "cached choice still reads each value occurrence"
            );
        }
        #[test]
        fn the_same_physical_record_under_two_templates_has_distinct_completed_match_values() {
            let (src, donor, trace) = tables();
            let attempt = plan(&src, &donor, true, T::Str, true).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let rows = attempt.result.unwrap().dataset.rows().to_vec();
            assert_eq!(rows.len(), 6);
            for row in &rows[..3] {
                assert_eq!(row[3..], [Value::Str("X".into()), Value::Str("X".into())]);
            }
            for row in &rows[3..] {
                assert_eq!(row[3..], [Value::Str("Y".into()), Value::Str("Y".into())]);
            }
            assert!(attempt.handler_counts.is_empty());
            assert_eq!(
                trace
                    .borrow()
                    .iter()
                    .filter(|&&(name, _, col)| name == "DONOR" && col == 0)
                    .count(),
                12
            );
        }
        #[test]
        fn opaque_selected_cell_identity_survives_and_prevents_filter_and_later_reads() {
            let (src, donor, trace) = tables();
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            let marker = Rc::new(());
            *donor.fail.borrow_mut() = Some((0, 1, Failure(payload, marker.clone())));
            let attempt = plan(&src, &donor, false, T::Str, true).execute(
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
                panic!("original opaque donor failure")
            };
            assert_eq!(
                (path.as_str(), source, source_row),
                ("intermediates[0]", 0, 0)
            );
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&retained, &marker));
            assert_eq!(
                *trace.borrow(),
                [
                    ("SRC", 0, 0),
                    ("SRC", 0, 1),
                    ("DONOR", 0, 0),
                    ("DONOR", 1, 0),
                    ("DONOR", 0, 1)
                ]
            );
            assert!(attempt.handler_counts.is_empty());
        }
        #[test]
        fn failure_position_is_retained_row_position_not_private_attempt_token() {
            let (src, donor, _) = tables();
            let attempt = plan(&src, &donor, false, T::Str, false).execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            let ExecutionError::UnmatchedKey {
                matched_key,
                identity: Some(identity),
                ..
            } = *attempt.result.unwrap_err()
            else {
                panic!("unmatched boundary")
            };
            assert_eq!(
                identity.position, 1,
                "third attempt follows only one accepted candidate"
            );
            assert_eq!(
                identity.values,
                [Value::Str("c".into()), Value::Str("L".into())]
            );
            assert_eq!(matched_key, [("K".into(), Value::Str("z".into()))]);
        }

        #[test]
        fn empty_retained_output_has_no_selection_effects() {
            let (mut src, donor, trace) = tables();
            let plan = plan(&src, &donor, false, T::Str, true);
            assert!(plan
                .execute(
                    &src,
                    &[&donor],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None
                )
                .result
                .is_ok());
            src.rows.clear();
            trace.borrow_mut().clear();
            let empty = plan.execute(
                &src,
                &[&donor],
                &mut yamaa_core::function_invocation::NoFunctions::new(),
                None,
            );
            assert!(empty.result.unwrap().dataset.rows().is_empty());
            assert!(trace.borrow().is_empty());
            assert!(empty.handler_counts.is_empty());
        }

        use crate::support::document::Tree;

        fn port_document(
            reordered: bool,
            drivers: [Option<&str>; 2],
            grouped: bool,
            field: &str,
        ) -> SpecificationDocument {
            let source = |name| {
                Tree::map(vec![(
                    "value",
                    Tree::map(vec![(
                        "source",
                        Tree::map(vec![("variable", Tree::text(name))]),
                    )]),
                )])
            };
            let mut inputs = vec![
                ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                ("DONOR", Tree::map(vec![("path", Tree::text("donor.csv"))])),
            ];
            if reordered {
                inputs.reverse();
            }
            let mut rows = Vec::new();
            for (id, driver) in ["first", "second"].iter().zip(drivers) {
                let mut fields = vec![
                    ("id", Tree::text(id)),
                    ("derivations", Tree::map(vec![("ROWV", source(field))])),
                ];
                if let Some(driver) = driver {
                    fields.push(("dataset", Tree::text(driver)));
                }
                if grouped {
                    fields.push(("group_by", Tree::List(vec![Tree::text("SRC.ID")])));
                }
                rows.push(Tree::map(fields));
            }
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                ("input", Tree::map(inputs)),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "intermediates",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("P")),
                        ("dataset", Tree::text("DONOR")),
                        ("key", Tree::map(vec![("K", Tree::text("KEY"))])),
                        ("no_match", Tree::Scalar(N::Null)),
                    ])]),
                ),
                (
                    "columns",
                    Tree::List(vec![
                        Tree::map(vec![
                            ("name", Tree::text("ID")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.ID")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("KEY")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.K")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("ROWV")),
                            ("type", Tree::text("str")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("COLV")),
                            ("type", Tree::text("str")),
                            ("derivation", source("P.V")),
                        ]),
                    ]),
                ),
                ("rows", Tree::List(rows)),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("out.csv")),
                        (
                            "columns",
                            Tree::List(vec![
                                Tree::text("ID"),
                                Tree::text("KEY"),
                                Tree::text("ROWV"),
                                Tree::text("COLV"),
                            ]),
                        ),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            SpecificationDocument::admit(Document::new(nodes, root).unwrap())
                .unwrap()
                .unwrap()
        }

        #[test]
        fn all_inputs_inspect_then_capture_decode_in_authored_order_and_move_opaque_failure_once() {
            use std::sync::Arc;
            use yamaa_core::specification_run::{
                self as run, PortError, RunError, SourceDecoder, SourcePort,
            };
            type IoTrace = Rc<RefCell<Vec<String>>>;
            struct Port {
                trace: IoTrace,
                reads: usize,
                fail: Option<Failure>,
            }
            impl SourcePort for Port {
                fn resource_failure(
                    &self,
                    _error: &Self::Error,
                ) -> Option<yamaa_core::resource::ResourceFailure> {
                    None
                }
                type Error = Failure;
                fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Failure> {
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
                ) -> Result<Arc<[u8]>, Failure> {
                    *origin = Some(run::SourceOrigin::Resource);
                    self.trace
                        .borrow_mut()
                        .push(format!("capture:{}", source.name));
                    if source.name == "SRC" {
                        if let Some(error) = self.fail.take() {
                            return Err(error);
                        }
                    }
                    self.reads += 1;
                    Ok(Arc::from([1u8]))
                }
            }
            struct Decoder {
                trace: IoTrace,
                fail: Option<Failure>,
            }
            impl SourceDecoder for Decoder {
                fn continue_after(&self, _error: &Self::Error) -> bool {
                    false
                }
                type Error = Failure;
                type Table = Table;
                fn decode(
                    &mut self,
                    source: &SourceDeclaration,
                    bytes: &[u8],
                    _interpretation: yamaa_core::specification::SourceInterpretation,
                    _contract: Option<&yamaa_core::producer_contract::Contract>,
                ) -> Result<Table, Failure> {
                    self.trace
                        .borrow_mut()
                        .push(format!("decode:{}", source.name));
                    assert_eq!(bytes, [1]);
                    if source.name == "SRC" {
                        if let Some(error) = self.fail.take() {
                            return Err(error);
                        }
                    }
                    let (src, donor, _) = tables();
                    Ok(if source.name == "SRC" { src } else { donor })
                }
            }
            let prepared = PreparedSpecification::prepare(&port_document(
                true,
                [Some("SRC"), Some("SRC")],
                false,
                "P.V",
            ))
            .unwrap();
            for capture_failure in [false, true] {
                let io = Rc::new(RefCell::new(Vec::new()));
                let payload = Box::new(7);
                let address = &*payload as *const u8;
                let marker = Rc::new(());
                let mut failure = Some(Failure(payload, marker.clone()));
                let mut port = Port {
                    trace: io.clone(),
                    reads: 0,
                    fail: if capture_failure {
                        failure.take()
                    } else {
                        None
                    },
                };
                let mut decoder = Decoder {
                    trace: io.clone(),
                    fail: failure,
                };
                let mut attempt = run::CapturedAttempt::new(prepared.source());
                run::execute_into(
                    &prepared,
                    &mut port,
                    &mut decoder,
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    &mut attempt,
                    yamaa_core::application_observer::Scope::new(
                        &yamaa_core::application_observer::Noop,
                    ),
                );
                let error = if capture_failure {
                    let PortError::Capture(error) = attempt.result.unwrap_err() else {
                        panic!("original capture payload")
                    };
                    error
                } else {
                    let PortError::Run(RunError::Sources(mut errors)) = attempt.result.unwrap_err()
                    else {
                        panic!("original decoder payload")
                    };
                    assert_eq!(errors.len(), 1);
                    let (source, error) = errors.pop().unwrap();
                    assert_eq!(source, 1);
                    error
                };
                assert_eq!(&*error.0 as *const u8, address);
                assert!(Rc::ptr_eq(&error.1, &marker));
                let mut expected = vec![
                    "inspect:DONOR",
                    "inspect:SRC",
                    "capture:DONOR",
                    "decode:DONOR",
                    "capture:SRC",
                ];
                if !capture_failure {
                    expected.push("decode:SRC");
                }
                assert_eq!(*io.borrow(), expected, "no later request or retry");
                assert!(attempt.sources[0].snapshot.is_some());
                assert!(attempt.sources[0].table.is_some());
                assert_eq!(attempt.sources[0].read.source.name, "DONOR");
                assert_eq!(port.reads, if capture_failure { 1 } else { 2 });
                assert!(attempt.sources[1].table.is_none());
            }
        }
    }
}
