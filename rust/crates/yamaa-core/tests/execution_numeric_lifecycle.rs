//! Execution contracts: numeric lifecycle and row intermediate context.
mod execution {
    mod numeric_lifecycle {
        use std::{collections::BTreeMap, convert::Infallible};
        use yamaa_core::conversion::ConversionError;
        use yamaa_core::expression::{
            compile, Condition, Context, ErrorKind, Resolver as NumericResolver,
        };
        use yamaa_core::numeric::ArithmeticErrorKind;
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
                compile(expression, Context::Compute, EXPRESSION).unwrap(),
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
                            let span = error.source_span.expect("formula span");
                            assert!(fields[1].get(span.start..span.end).is_some());
                            assert_eq!(error.expression, fields[1]);
                            path = error.spec_path;
                            let ErrorKind::Condition(condition) = error.kind;
                            context = match &condition {
                                Condition::NumericUnknownField { identifier } => {
                                    format!("identifier:{identifier}")
                                }
                                Condition::NumericInput { identifier, actual } => {
                                    assert_eq!(*actual, ValueType::Str);
                                    format!("source:{identifier}:str")
                                }
                                Condition::Arithmetic(ArithmeticErrorKind::IntegerOverflow {
                                    value,
                                }) => format!("integer:{value}"),
                                _ => "-".into(),
                            };
                            format!(
                                "error:{}:{}:{}",
                                condition.phase(),
                                condition.condition(),
                                condition.rule()
                            )
                        }
                        NumericLifecycleError::Conversion { spec_path, error } => {
                            path = spec_path;
                            context = conversion_value(&error, target);
                            format!(
                                "error:{}:{}:{}",
                                error.phase(),
                                error.condition(),
                                error.rule()
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
                                replacement.rule()
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
                compile("A", Context::Compute, EXPRESSION).unwrap(),
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
            let span = error.source_span.unwrap();
            assert_eq!(&"NULL + BROKEN + LATER"[span.start..span.end], "BROKEN");
            let ErrorKind::Resolution {
                identifier,
                error: Opaque(payload),
            } = error.kind
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
                compile(
                    "A + 1",
                    Context::Compute,
                    "columns.B.derivation.value.compute",
                )
                .unwrap(),
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
    mod row_intermediate_context {
        //! Complete record attempts have stable choices and separate diagnostic positions.
        use crate::support::table_schema as schema;
        use std::{cell::RefCell, rc::Rc};

        use yamaa_core::{
            schema::{Document, DocumentNode as N},
            specification::SourceDeclaration,
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

        use crate::support::document::Tree;
        use crate::support::specification::prepare;

        fn port_document(
            reordered: bool,
            drivers: [Option<&str>; 2],
            grouped: bool,
            field: &str,
        ) -> Document {
            let source = |name| Tree::map(vec![("value", Tree::text(name))]);
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
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
                ("domain", Tree::text("TEST")),
                ("input", Tree::map(inputs)),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "intermediates",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("P")),
                        ("dataset", Tree::text("DONOR")),
                        ("key", Tree::map(vec![("K", Tree::text("KEY"))])),
                        ("absent", Tree::Scalar(N::Null)),
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
            tree.document()
        }

        #[test]
        fn all_inputs_inspect_then_capture_decode_in_authored_order_and_move_opaque_failure_once() {
            use std::sync::Arc;
            use yamaa_core::decoder;
            use yamaa_core::resolver::{self, Captured, Located, Name, Resolver};
            use yamaa_core::specification_run::{self as run, PortError, RunError};
            type IoTrace = Rc<RefCell<Vec<String>>>;
            struct Port {
                trace: IoTrace,
                reads: usize,
                fail: Option<Failure>,
            }
            impl Resolver for Port {
                type Error = Failure;
                fn inspect(&mut self, name: Name<'_>) -> Result<Located, Failure> {
                    let Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.trace
                        .borrow_mut()
                        .push(format!("inspect:{}", source.name));
                    Ok(Located {
                        identity: source.path.clone(),
                        display_path: source.path.clone(),
                    })
                }
                fn capture(&mut self, name: Name<'_>) -> Result<Captured, Failure> {
                    let Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.trace
                        .borrow_mut()
                        .push(format!("capture:{}", source.name));
                    if source.name == "SRC" {
                        if let Some(error) = self.fail.take() {
                            return Err(error);
                        }
                    }
                    self.reads += 1;
                    Ok(Captured::resource(Arc::from([1u8]), true))
                }
                fn failure(&self, _: Name<'_>, _: &Failure) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Failure> {
                    panic!("an input is never rebased")
                }
            }
            struct Decoder {
                trace: IoTrace,
                fail: Option<Failure>,
            }
            impl decoder::Decoder for Decoder {
                crate::support::ports::no_documents!();
                fn continue_after(&self, _error: &Self::Error) -> bool {
                    false
                }
                type Error = Failure;
                type Table = Table;
                fn table(
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
            let prepared = prepare(&port_document(
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
