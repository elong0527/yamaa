//! Project contracts.
#[allow(dead_code)]
mod support;
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
            temporal::{Date, DatePrecision},
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
            let (condition, requirement) = match &f.kind {
                FailureKind::UnknownArguments(_)
                | FailureKind::MissingRequired { .. }
                | FailureKind::ArgumentType { .. } => ("invalid_function_argument", "REQ-0700"),
                FailureKind::CallFailed(_) => ("function_call_failed", "REQ-0701"),
                _ => ("invalid_function_result", "REQ-0702"),
            };
            assert_eq!((f.condition(), f.requirement()), (condition, requirement));
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
        /// Shared hand-written truth also runs through the real Python BoundFunction.
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

        /// Reuse repeats effects, including after failures; returned text owns its memory.
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

        /// Host-independent values retain precision until an actual host encoder consumes them.
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
                    assert_eq!(date.collected_precision(), DatePrecision::Year);
                    Ok(Value::Date(date))
                }
            }
            let date = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
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
            assert_eq!(result.collected_precision(), DatePrecision::Year);
            assert_eq!(calls.get(), 1);
        }

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
    mod producer_admission {
        use crate::support::producer as support;
        use serde_json::Value as V;
        use support::{candidate, consumer, document, producer, Tree};
        use yamaa_core::schema::DocumentNode as N;
        use yamaa_core::{
            diagnostic::Diagnostic,
            producer_admission::{prepare, Error, Input},
            producer_contract,
            specification::{PrepareError, PreparedSpecification},
            value::ColumnType,
        };
        fn invalid(
            result: Result<yamaa_core::producer_admission::Prepared, Error>,
        ) -> Vec<Diagnostic> {
            match result {
                Err(Error::Invalid(findings)) => findings,
                _ => panic!("producer admission findings"),
            }
        }
        fn text(s: &str) -> V {
            V::String(s.into())
        }
        #[test]
        fn sealed_compiler_preserves_distinct_producer_identity_fields_and_authored_origins() {
            let source = producer();
            let consumer = consumer(None);
            let metadata =
                prepare("/consumer.yaml", &consumer, &[candidate(&source)], None).unwrap();
            let inputs = metadata.inputs().collect::<Vec<_>>();
            let [Input::Producer(declaration)] = inputs.as_slice() else {
                panic!("producer kind")
            };
            assert_eq!(declaration.dataset(), "SRC");
            assert_eq!(declaration.schema_path(), "nested/producer.yaml");
            assert_eq!(
                declaration.schema_origin().declaring_source(),
                "/parent.yaml"
            );
            assert_eq!(
                declaration.schema_origin().written(),
                "../nested/producer.yaml"
            );
            assert_eq!(
                declaration.input_origin().declaring_source(),
                "/consumer.yaml"
            );
            assert_eq!(declaration.input_origin().written(), "./produced.csv");
            assert_eq!(declaration.producer_identity(), "/producer.yaml");
            assert_eq!(declaration.artifact_identity(), "/produced.csv");
            assert_eq!(
                declaration
                    .contract()
                    .fields()
                    .iter()
                    .map(|f| (f.name.as_str(), f.kind, f.label.as_str()))
                    .collect::<Vec<_>>(),
                [
                    ("ID", ColumnType::Int, "Identifier"),
                    ("VALUE", ColumnType::Float, "Reported value")
                ]
            );
            assert_eq!(metadata.projection(), ["ID"]);
            assert!(metadata.called_functions().is_empty());
            assert!(matches!(
                PreparedSpecification::prepare(&consumer),
                Err(PrepareError::Unsupported(_))
            ));
        }
        #[test]
        fn input_declaration_order_is_independent_of_supplied_metadata_order() {
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
            let mut a = candidate(&source);
            let mut b = candidate(&source);
            a.dataset = "A";
            a.schema_path = "a.yaml";
            b.dataset = "B";
            b.schema_path = "b.yaml";
            let metadata = prepare("/consumer.yaml", &consumer, &[a, b], None).unwrap();
            assert_eq!(
                metadata
                    .inputs()
                    .map(|i| match i {
                        Input::Producer(p) => (p.dataset(), true),
                        Input::External(s) => (s.name.as_str(), false),
                    })
                    .collect::<Vec<_>>(),
                [("B", true), ("EXT", false), ("A", true)]
            );
            assert_eq!(
                metadata
                    .producers()
                    .iter()
                    .map(|p| p.dataset())
                    .collect::<Vec<_>>(),
                ["B", "A"]
            );
        }
        #[test]
        fn inline_types_are_redundant_even_when_equal_empty_or_null() {
            let source = producer();
            for types in [
                Tree::map(vec![("ID", Tree::text("int"))]),
                Tree::map(vec![]),
                Tree::Scalar(N::Null),
            ] {
                let findings = invalid(prepare(
                    "/consumer.yaml",
                    &consumer(Some(types)),
                    &[candidate(&source)],
                    None,
                ));
                assert_eq!(findings.len(), 1);
                assert_eq!(
                    (
                        findings[0].definition().condition,
                        findings[0].definition().requirement
                    ),
                    ("redundant_field_type", Some("REQ-0523"))
                );
                assert_eq!(findings[0].context["dataset"], text("SRC"));
                assert!(findings[0].spec_paths[0].starts_with("input.SRC.types"));
            }
        }
        #[test]
        fn missing_duplicate_and_undeclared_metadata_are_never_external_file_reads() {
            let source = producer();
            let consumer = consumer(None);
            let findings = invalid(prepare("/consumer.yaml", &consumer, &[], None));
            assert_eq!(findings[0].context["reason"], text("missing_metadata"));
            let findings = invalid(prepare(
                "/consumer.yaml",
                &consumer,
                &[candidate(&source), candidate(&source)],
                None,
            ));
            assert_eq!(findings[0].context["reason"], text("duplicate_metadata"));
            let mut extra = candidate(&source);
            extra.dataset = "ABSENT";
            let findings = invalid(prepare(
                "/consumer.yaml",
                &consumer,
                &[candidate(&source), extra],
                None,
            ));
            assert_eq!(findings[0].context["reason"], text("undeclared_metadata"));
        }
        #[test]
        fn contradictory_identities_and_self_dependency_fail_at_metadata_admission() {
            let source = producer();
            let consumer = consumer(None);
            for variation in 0..4 {
                let mut c = candidate(&source);
                match variation {
                    0 => c.schema_path = "other.yaml",
                    1 => c.schema_identity = "/other.yaml",
                    2 => c.input_origin.written = "",
                    _ => {
                        c.schema_identity = "/consumer.yaml";
                        c.producer_identity = "/consumer.yaml";
                    }
                }
                let findings = invalid(prepare("/consumer.yaml", &consumer, &[c], None));
                assert_eq!(
                    findings[0].context["reason"],
                    text(if variation == 3 {
                        "producer_workflow_cycle"
                    } else {
                        "contradictory_metadata"
                    })
                );
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].spec_paths, ["input.SRC.schema"]);
                let definition = findings[0].definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    ("validation", "invalid_producer_contract", Some("REQ-0534")),
                );
                assert_eq!(findings[0].context.len(), 2);
                assert_eq!(findings[0].context["dataset"], text("SRC"));
            }
        }
        #[test]
        fn canonical_artifact_mismatch_keeps_written_geometry_and_normative_mapping() {
            let source = producer();
            let consumer = consumer(None);
            let mut c = candidate(&source);
            c.output_identity = "/other.csv";
            let findings = invalid(prepare("/consumer.yaml", &consumer, &[c], None));
            let f = &findings[0];
            assert_eq!(
                (
                    f.definition().phase,
                    f.definition().condition,
                    f.definition().requirement
                ),
                (
                    "validation",
                    "producer_output_path_mismatch",
                    Some("REQ-0534")
                )
            );
            assert_eq!(f.spec_paths, ["input.SRC.path", "input.SRC.schema"]);
            assert_eq!(f.context["source_path"], text("./produced.csv"));
            assert_eq!(f.context["output_path"], text("produced.csv"));
        }
        #[test]
        fn producer_contract_findings_retain_selected_field_order() {
            let source = document(
                Tree::map(vec![(
                    "RAW",
                    Tree::map(vec![("path", Tree::text("raw.csv"))]),
                )]),
                &[("ID", "int", None)],
                &["ID", "ID", "OTHER"],
                false,
            );
            let findings = invalid(prepare(
                "/consumer.yaml",
                &consumer(None),
                &[candidate(&source)],
                None,
            ));
            assert_eq!(
                findings
                    .iter()
                    .map(|f| f.spec_paths[0].as_str())
                    .collect::<Vec<_>>(),
                [
                    "input.SRC.schema.columns.ID.label",
                    "input.SRC.schema.output.columns[1]",
                    "input.SRC.schema.output.columns[2]"
                ]
            );
            assert!(findings
                .iter()
                .all(|f| f.definition().requirement == Some("REQ-0534")));
            assert_eq!(
                findings[2].context["declarations"],
                V::Number(("0").parse().expect("canonical integer"))
            );
        }

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
                matches!(prepare("/consumer.yaml",&consumer,&[candidate(&source)],None),Err(Error::Compilation(PrepareError::Unsupported(features))) if features.iter().any(|f| f.path=="parents" && f.operation == "parents"))
            );
        }
        #[test]
        fn actual_metadata_comparison_is_ordered_typed_bounded_and_portable() {
            let contract = producer_contract::prepare(&producer()).unwrap();
            assert!(contract
                .validate_header("SRC", &["ID", "VALUE"])
                .unwrap()
                .is_none());
            for actual in [
                vec!["VALUE", "ID"],
                vec!["ID"],
                vec!["ID", "VALUE", "OTHER"],
                vec!["ID", "ID"],
            ] {
                let finding = contract.validate_header("SRC", &actual).unwrap().unwrap();
                assert_eq!(
                    (
                        finding.definition().condition,
                        finding.definition().requirement
                    ),
                    ("producer_contract_mismatch", Some("REQ-0535"))
                );
                assert_eq!(finding.spec_paths, ["input.SRC.schema", "input.SRC.path"]);
            }
            let finding = contract
                .validate_typed_fields(
                    "SRC",
                    &[("ID", ColumnType::Int), ("VALUE", ColumnType::Str)],
                )
                .unwrap()
                .unwrap();
            assert_eq!(
                finding.context["expected_types"],
                V::Array(vec![text("int"), text("float")])
            );
            assert_eq!(
                finding.context["actual_types"],
                V::Array(vec![text("int"), text("str")])
            );
            assert_eq!(finding.context["reordered"], V::Bool(false));
            assert!(contract
                .validate_header("SRC", &["ID", "VALUE"])
                .unwrap()
                .is_none());
        }
        #[test]
        fn one_producer_identity_cannot_describe_two_different_output_contracts() {
            let input = Tree::map(vec![
                (
                    "A",
                    Tree::map(vec![
                        ("path", Tree::text("produced.csv")),
                        ("schema", Tree::text("a.yaml")),
                    ]),
                ),
                (
                    "B",
                    Tree::map(vec![
                        ("path", Tree::text("produced.csv")),
                        ("schema", Tree::text("b.yaml")),
                    ]),
                ),
            ]);
            let consumer = document(input, &[("ID", "int", Some("ID"))], &["ID"], false);
            let source = producer();
            let other = document(
                Tree::map(vec![(
                    "RAW",
                    Tree::map(vec![("path", Tree::text("raw.csv"))]),
                )]),
                &[
                    ("ID", "int", Some("Identifier")),
                    ("VALUE", "str", Some("Reported value")),
                ],
                &["ID", "VALUE"],
                false,
            );
            let mut a = candidate(&source);
            a.dataset = "A";
            a.schema_path = "a.yaml";
            let mut b = candidate(&other);
            b.dataset = "B";
            b.schema_path = "b.yaml";
            let findings = invalid(prepare("/consumer.yaml", &consumer, &[a, b], None));
            assert_eq!(findings[0].spec_paths, ["input.B.schema"]);
            assert_eq!(
                findings[0].context["reason"],
                text("contradictory_metadata")
            );
        }
    }
}
