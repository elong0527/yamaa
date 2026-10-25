//! Project contracts: function invocation and producer links. The seams
//! here are scripted function ports and the values preparation returns;
//! producer-link refusals are cases of `fixtures/diagnostics/producers.json`.
mod project {
    mod function_invocation {
        use std::collections::BTreeMap;
        use yamaa_core::function_invocation::*;
        use yamaa_core::function_signature::{
            FunctionIdentity, InvocationPlan, Parameter, Presence, ProjectFunctionIdentity,
            ProjectInvocationPlan,
        };
        use yamaa_core::{
            table::ValueRef,
            temporal::{Date, Precision},
            value::{ColumnType, Value, ValueType},
        };

        /// Independently authored input spelling; expected bits are never generated here.
        fn value(token: &str) -> Value {
            if token == "missing" {
                return Value::Missing;
            }
            let (kind, text) = token.split_once(':').unwrap();
            match kind {
                "int" => Value::Int(text.parse().unwrap()),
                "float" => Value::float(text.parse().unwrap()),
                "str" => Value::Str(text.into()),
                "bool" => Value::Bool(text.parse().unwrap()),
                "date" => Value::Date(text.parse().unwrap()),
                "datetime" => Value::DateTime(text.parse().unwrap()),
                _ => panic!("invalid fixture type"),
            }
        }
        /// Preserve exact type and float bits when observing invocation/return values.
        fn encode(v: ValueRef<'_>) -> String {
            match v {
                ValueRef::Missing => "missing".into(),
                ValueRef::Int(n) => format!("int:{n}"),
                ValueRef::Float(n) => format!("float:{:016x}", n.get().to_bits()),
                ValueRef::Str(s) => format!("str:{s}"),
                ValueRef::Bool(b) => format!("bool:{b}"),
                ValueRef::Date(d) => format!("date:{d}"),
                ValueRef::DateTime(d) => format!("datetime:{d}"),
            }
        }
        /// Decode only the closed logical type vocabulary.
        fn kind(text: &str) -> ValueType {
            match text {
                "int" => ValueType::Int,
                "float" => ValueType::Float,
                "str" => ValueType::Str,
                "bool" => ValueType::Bool,
                "date" => ValueType::Date,
                "datetime" => ValueType::DateTime,
                _ => panic!("invalid fixture type"),
            }
        }
        /// Return types deliberately exclude Boolean values.
        fn result_kind(text: &str) -> ColumnType {
            match text {
                "int" => ColumnType::Int,
                "float" => ColumnType::Float,
                "str" => ColumnType::Str,
                "date" => ColumnType::Date,
                "datetime" => ColumnType::DateTime,
                _ => panic!("invalid result type"),
            }
        }
        /// Fixed resolved identity must accompany every failure without host rediscovery.
        fn identity() -> FunctionIdentity {
            FunctionIdentity {
                name: "example".into(),
                contract_version: "1".into(),
                implementation_version: "2".into(),
                call: "artifact.example".into(),
            }
        }
        /// Build declarations in authored order, retaining the host mapping and defaults.
        fn parameters(text: &str) -> Vec<Parameter> {
            if text == "-" {
                return vec![];
            }
            text.split(';')
                .map(|p| {
                    let f: Vec<_> = p.split('/').collect();
                    Parameter {
                        name: f[0].into(),
                        host_name: f[4].into(),
                        kind: kind(f[1]),
                        accepts_missing: f[2].parse().unwrap(),
                        presence: if f[3] == "required" {
                            Presence::Required
                        } else {
                            Presence::Optional(value(f[3]))
                        },
                    }
                })
                .collect()
        }
        /// A non-Clone host error demonstrates that payloads are moved, not reconstructed.
        #[derive(Debug, PartialEq)]
        struct Payload(String);
        struct Port<'a> {
            action: &'a str,
            trace: Vec<String>,
            calls: usize,
        }
        impl FunctionPort for Port<'_> {
            type Error = Payload;
            /// Record one effect before returning a normalized scalar or host failure.
            fn call(
                &mut self,
                _slot: usize,
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
                self.calls += 1;
                self.trace.extend(
                    args.iter()
                        .map(|a| format!("{}={}", a.name, encode(a.value))),
                );
                if args.is_empty() {
                    self.trace.push("called".into());
                }
                if self.action == "raise" {
                    return Err(HostError::Raised(Payload("ValueError:boom".into())));
                }
                if let Some(reason) = self.action.strip_prefix("invalid:") {
                    return Err(HostError::InvalidResult(Payload(reason.into())));
                }
                if let Some(name) = self.action.strip_prefix("echo:") {
                    return Ok(match args.iter().find(|a| a.name == name).unwrap().value {
                        ValueRef::Missing => Value::Missing,
                        ValueRef::Str(s) => Value::Str(s.into()),
                        ValueRef::Int(n) => Value::Int(n),
                        ValueRef::Float(n) => Value::Float(n),
                        ValueRef::Bool(b) => Value::Bool(b),
                        ValueRef::Date(d) => Value::Date(d),
                        ValueRef::DateTime(d) => Value::DateTime(d),
                    });
                }
                Ok(value(self.action))
            }
        }
        /// Render portable failure facts and assert normative metadata before comparison.
        fn failure(f: InvocationFailure<Payload>) -> String {
            assert_eq!(f.identity, identity());
            assert_eq!(f.phase(), "derivation");
            let (condition, rule) = match &f.kind {
                FailureKind::UnknownArguments(_)
                | FailureKind::MissingRequired { .. }
                | FailureKind::ArgumentType { .. } => ("invalid_function_argument", "FUNC-5"),
                FailureKind::CallFailed(_) => ("function_call_failed", "FUNC-8"),
                _ => ("invalid_function_result", "FUNC-9"),
            };
            assert_eq!((f.condition(), f.rule()), (condition, rule));
            match f.kind {
                FailureKind::UnknownArguments(names) => {
                    format!("error:unknown:{}", names.join(","))
                }
                FailureKind::MissingRequired { parameter } => format!("error:missing:{parameter}"),
                FailureKind::ArgumentType {
                    parameter,
                    expected,
                    actual,
                } => format!("error:argument:{parameter}:{expected:?}:{actual:?}").to_lowercase(),
                FailureKind::CallFailed(Payload(s)) => format!("error:raised:{s}"),
                FailureKind::InvalidHostResult(Payload(s)) => format!("error:invalid:{s}"),
                FailureKind::BooleanResult => "error:boolean".into(),
                FailureKind::UndeclaredMissing => "error:undeclared-missing".into(),
                FailureKind::ResultType { expected, actual } => {
                    format!("error:result:{expected:?}:{actual:?}").to_lowercase()
                }
            }
        }
        /// Already data: `fixtures/function_invocation.tsv`, hand-written truth
        /// that also runs through the real Python BoundFunction.
        #[test]
        fn shared_function_invocation_vectors() {
            let mut count = 0;
            for row in include_str!("fixtures/function_invocation.tsv")
                .lines()
                .skip(1)
            {
                count += 1;
                let f: Vec<_> = row.split('\t').collect();
                assert_eq!(f.len(), 8);
                let plan = InvocationPlan::new(
                    identity(),
                    parameters(f[1]),
                    result_kind(f[4]),
                    f[5].parse().unwrap(),
                )
                .unwrap();
                let supplied = if f[2] == "-" {
                    BTreeMap::new()
                } else {
                    f[2].split(';')
                        .map(|s| {
                            let (k, v) = s.split_once('=').unwrap();
                            (k.into(), value(v))
                        })
                        .collect()
                };
                let mut port = Port {
                    action: f[3],
                    trace: vec![],
                    calls: 0,
                };
                let actual = match invoke(&plan, &supplied, &mut port) {
                    Ok(v) => encode((&v).into()),
                    Err(e) => failure(e),
                };
                assert_eq!(actual, f[6], "{}", f[0]);
                assert_eq!(port.calls, usize::from(f[7] != "-"), "{}", f[0]);
                assert_eq!(
                    if port.trace.is_empty() {
                        "-".into()
                    } else {
                        port.trace.join(";")
                    },
                    f[7],
                    "{}",
                    f[0]
                );
            }
            assert_eq!(count, 42);
        }

        /// Seam: a scripted `FunctionPort`'s calls and trace (core ports). Reuse
        /// repeats effects, including after failures; returned text owns its memory.
        #[test]
        fn synchronous_effects_are_never_retried_memoized_or_rolled_back() {
            let plan = InvocationPlan::new(
                identity(),
                parameters("x/str/false/required/host_x"),
                ColumnType::Str,
                false,
            )
            .unwrap();
            let mut port = Port {
                action: "echo:host_x",
                trace: vec![],
                calls: 0,
            };
            let owned = {
                let input = BTreeMap::from([("x".into(), Value::Str("owned".into()))]);
                invoke(&plan, &input, &mut port).unwrap()
            };
            assert_eq!(owned, Value::Str("owned".into()));
            let input = BTreeMap::from([("x".into(), Value::Str("again".into()))]);
            for action in ["raise", "invalid:list", "int:1", "echo:host_x"] {
                port.action = action;
                let result = invoke(&plan, &input, &mut port);
                assert_eq!(result.is_ok(), action == "echo:host_x");
            }
            assert_eq!(port.calls, 5);
            assert_eq!(port.trace.len(), 5);
            assert_eq!(plan.identity(), &identity());
        }

        /// Seam: the precision a scripted, non-`Send` `FunctionPort` observes
        /// (core ports). Host-independent values retain precision until an
        /// actual host encoder consumes them.
        #[test]
        fn typed_port_retains_temporal_precision_and_needs_no_send_bound() {
            struct Local(std::rc::Rc<std::cell::Cell<usize>>);
            impl FunctionPort for Local {
                type Error = std::convert::Infallible;
                /// Observe the borrowed precision and return the same owned civil value.
                fn call(
                    &mut self,
                    _slot: usize,
                    args: &[Argument<'_>],
                ) -> Result<Value, HostError<Self::Error>> {
                    self.0.set(self.0.get() + 1);
                    let ValueRef::Date(date) = args[0].value else {
                        panic!("date")
                    };
                    assert_eq!(date.collected_precision(), Precision::Year);
                    Ok(Value::Date(date))
                }
            }
            let date = Date::new(2025, 1, 1, Precision::Year).unwrap();
            let plan = InvocationPlan::new(
                identity(),
                parameters("x/date/false/required/host_x"),
                ColumnType::Date,
                false,
            )
            .unwrap();
            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
            let mut port = Local(calls.clone());
            let result = invoke(
                &plan,
                &BTreeMap::from([("x".into(), Value::Date(date))]),
                &mut port,
            )
            .unwrap();
            let Value::Date(result) = result else {
                panic!("date")
            };
            assert_eq!(result.collected_precision(), Precision::Year);
            assert_eq!(calls.get(), 1);
        }

        /// Seam: a versionless plan's defaults, values and failure kinds through a
        /// scripted `FunctionPort`, with its calls and trace (core ports).
        #[test]
        fn versionless_invocation_defaults_missing_types_and_repeated_effects_share_one_service() {
            let id = ProjectFunctionIdentity {
                name: "identity".into(),
                call: "program.identity".into(),
            };
            let plan = ProjectInvocationPlan::new(
                id.clone(),
                vec![Parameter {
                    name: "x".into(),
                    host_name: "x".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Optional(Value::Int(i64::MAX)),
                }],
                ColumnType::Int,
                false,
            )
            .unwrap();
            let mut port = Port {
                action: "echo:x",
                trace: vec![],
                calls: 0,
            };
            for expected_calls in 1..=2 {
                assert_eq!(
                    invoke_project(&plan, &BTreeMap::new(), &mut port),
                    Ok(Value::Int(i64::MAX))
                );
                assert_eq!(port.calls, expected_calls);
            }
            assert_eq!(
                port.trace,
                vec!["x=int:9223372036854775807", "x=int:9223372036854775807"]
            );
            assert_eq!(
                invoke_project(
                    &plan,
                    &BTreeMap::from([("x".into(), Value::Missing)]),
                    &mut port
                ),
                Ok(Value::Missing)
            );
            assert_eq!(port.calls, 2);
            let error = invoke_project(
                &plan,
                &BTreeMap::from([("x".into(), Value::float(1.0))]),
                &mut port,
            )
            .unwrap_err();
            assert_eq!(error.identity, id);
            assert_eq!(
                error.kind,
                FailureKind::ArgumentType {
                    parameter: "x".into(),
                    expected: ValueType::Int,
                    actual: ValueType::Float
                }
            );
            assert_eq!(port.calls, 2);
        }

        /// Seam: the opaque host payload and the result failure kinds a scripted
        /// `FunctionPort` produces, with its call counts (core ports).
        #[test]
        fn versionless_failures_keep_opaque_host_payload_and_exact_result_rules() {
            let id = ProjectFunctionIdentity {
                name: "constant".into(),
                call: "program.constant".into(),
            };
            let plan =
                ProjectInvocationPlan::new(id.clone(), vec![], ColumnType::Int, false).unwrap();
            let mut port = Port {
                action: "raise",
                trace: vec![],
                calls: 0,
            };
            let failure = invoke_project(&plan, &BTreeMap::new(), &mut port).unwrap_err();
            assert_eq!(failure.identity, id);
            assert_eq!(
                failure.kind,
                FailureKind::CallFailed(Payload("ValueError:boom".into()))
            );
            assert_eq!(port.calls, 1);
            for (action, kind) in [
                ("bool:true", FailureKind::BooleanResult),
                ("missing", FailureKind::UndeclaredMissing),
                (
                    "float:1.0",
                    FailureKind::ResultType {
                        expected: ColumnType::Int,
                        actual: ValueType::Float,
                    },
                ),
            ] {
                let mut port = Port {
                    action,
                    trace: vec![],
                    calls: 0,
                };
                let failure = invoke_project(&plan, &BTreeMap::new(), &mut port).unwrap_err();
                assert_eq!(failure.kind, kind);
                assert_eq!(port.calls, 1);
            }
        }
    }
    mod producer_links {
        //! A 1.0 consumer prepared with the output contracts its `schema` links
        //! name. Its refusals are cases of `fixtures/diagnostics/producers.json`.
        use crate::support::producer as support;
        use crate::support::specification::prepare_linked;
        use support::{consumer, document, link, links, producer, Tree};
        use yamaa_core::{specification::PrepareError, value::ColumnType};
        /// Seam: the rendered message of a producer cycle, which an issue row
        /// does not hold. A producer whose copies lead back to a specification
        /// being resolved names each specification on the cycle in its message
        /// (SPEC-9); the refusal's record is a case of
        /// `fixtures/diagnostics/producers.json`.
        #[test]
        fn a_copy_cycle_names_each_specification_on_it_in_its_message() {
            let cycle = yamaa_core::copy_expansion::producer_cycle(
                "SRC",
                &["/adae.yaml", "/adsl.yaml", "/adae.yaml"],
            );
            assert_eq!(
                cycle.message(),
                "the producer links form a cycle: /adae.yaml, /adsl.yaml, /adae.yaml"
            );
        }
        /// Seam: the compiled sources and source contract a linked preparation
        /// returns (core outcome).
        #[test]
        fn linked_contract_types_the_stored_fields_without_compiling_the_producer() {
            // The producer's own source and derivations never enter the
            // consumer's compilation: only its selected output contract does.
            let source = producer();
            let consumer = consumer(None);
            let compiled = prepare_linked(&consumer, None, links(&[link(&source)])).unwrap();
            assert_eq!(compiled.sources().len(), 1);
            assert_eq!(compiled.sources()[0].name, "SRC");
            assert_eq!(compiled.sources()[0].path, "produced.csv");
            assert_eq!(
                compiled.sources()[0].types,
                [
                    ("ID".to_owned(), ColumnType::Int),
                    ("VALUE".to_owned(), ColumnType::Float)
                ]
            );
            let contract = compiled.source_contract(0).unwrap();
            assert_eq!(
                contract
                    .fields()
                    .iter()
                    .map(|f| (f.name.as_str(), f.kind, f.label.as_str()))
                    .collect::<Vec<_>>(),
                [
                    ("ID", ColumnType::Int, "Identifier"),
                    ("VALUE", ColumnType::Float, "Reported value")
                ]
            );
            assert!(compiled.source_contract(1).is_none());
            assert_eq!(compiled.projection(), ["ID"]);
            assert!(compiled.called_functions().is_empty());
        }
        /// Seam: the order and contracts of the compiled sources (core outcome).
        #[test]
        fn input_declaration_order_is_independent_of_supplied_link_order() {
            let input = Tree::map(vec![
                (
                    "B",
                    Tree::map(vec![
                        ("path", Tree::text("b.csv")),
                        ("schema", Tree::text("b.yaml")),
                    ]),
                ),
                ("EXT", Tree::map(vec![("path", Tree::text("external.csv"))])),
                (
                    "A",
                    Tree::map(vec![
                        ("path", Tree::text("a.csv")),
                        ("schema", Tree::text("a.yaml")),
                    ]),
                ),
            ]);
            let consumer = document(input, &[("ID", "int", Some("ID"))], &["ID"], false);
            let source = producer();
            let mut a = link(&source);
            let mut b = link(&source);
            a.dataset = "A";
            b.dataset = "B";
            let compiled = prepare_linked(&consumer, None, links(&[a, b])).unwrap();
            assert_eq!(
                compiled
                    .sources()
                    .iter()
                    .enumerate()
                    .map(|(index, source)| (
                        source.name.as_str(),
                        compiled.source_contract(index).is_some()
                    ))
                    .collect::<Vec<_>>(),
                [("B", true), ("EXT", false), ("A", true)]
            );
            assert!(compiled.sources()[1].types.is_empty());
        }

        /// Seam: `PrepareError::Unsupported`, which reports no issue.
        #[test]
        fn unrelated_unsupported_consumer_vocabulary_is_not_stripped() {
            let source = producer();
            let consumer = document(
                Tree::map(vec![(
                    "SRC",
                    Tree::map(vec![
                        ("path", Tree::text("produced.csv")),
                        ("schema", Tree::text("nested/producer.yaml")),
                    ]),
                )]),
                &[("ID", "int", Some("ID"))],
                &["ID"],
                true,
            );
            assert!(
                matches!(prepare_linked(&consumer, None, links(&[link(&source)])), Err(PrepareError::Unsupported(features)) if features.iter().any(|f| f.path=="parent" && f.operation == "parent"))
            );
        }
    }
}
