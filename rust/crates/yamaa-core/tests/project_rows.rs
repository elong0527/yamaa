//! Project contracts.
#[allow(dead_code)]
mod support;
mod project {
    mod project_compiled_rows {
        //! Execute compiler-selected row/default calls without a host-authored plan.
        use crate::support::project_call_compiler as support;
        use std::cell::RefCell;
        use support::{call, function, specification_with_rows, Tree};
        use yamaa_core::function_invocation::{Argument, FunctionPort, HostError};
        use yamaa_core::{
            function_signature::{InvocationPlan, ProjectInvocationPlan},
            schema::DocumentNode as N,
            specification::PreparedSpecification,
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        fn literal(value: i64) -> Tree {
            Tree::map(vec![(
                "literal",
                Tree::Scalar(N::Integer(value.to_string())),
            )])
        }
        fn template(id: &str, key: Tree, second: bool) -> Tree {
            let mut derivations =
                vec![("ID", key), ("D", call("id", vec![("x", Tree::text("A"))]))];
            if second {
                derivations.push(("B", literal(9)));
            }
            Tree::map(vec![
                ("id", Tree::text(id)),
                (
                    "derivations",
                    Tree::map(
                        derivations
                            .into_iter()
                            .map(|(name, value)| (name, Tree::map(vec![("value", value)])))
                            .collect(),
                    ),
                ),
            ])
        }
        struct Table {
            schema: TableSchema,
            observations: RefCell<Vec<&'static str>>,
        }
        impl TableAccess for Table {
            type Error = &'static str;
            fn schema(&self) -> &TableSchema {
                self.observations.borrow_mut().push("schema");
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.observations.borrow_mut().push("rows");
                1
            }
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.observations.borrow_mut().push("cell");
                Ok(ValueRef::Int(1))
            }
        }
        struct Bindings {
            signature: ProjectInvocationPlan,
            inputs: Vec<i64>,
        }
        impl FunctionPort for Bindings {
            type Error = &'static str;
            fn signature(&self, _: usize) -> Option<&InvocationPlan> {
                None
            }
            fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
                (slot == 0).then_some(&self.signature)
            }
            fn call(
                &mut self,
                slot: usize,
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Self::Error>> {
                assert_eq!(slot, 0);
                assert_eq!(args.len(), 1);
                assert_eq!(args[0].name, "x");
                let ValueRef::Int(value) = args[0].value else {
                    panic!("exact scalar integer")
                };
                self.inputs.push(value);
                Ok(Value::Int(value))
            }
        }

        #[test]
        fn compiled_row_defaults_execute_in_each_template_order_and_repeat_without_caches() {
            let spec = specification_with_rows(
                vec![
                    ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                    ("B", "int", call("id", vec![("x", Tree::text("C"))])),
                    ("C", "int", literal(7)),
                    ("D", "int", literal(0)),
                ],
                vec![
                    template(
                        "first",
                        Tree::map(vec![(
                            "source",
                            Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                        )]),
                        false,
                    ),
                    template("second", literal(2), true),
                ],
            );
            let prepared = PreparedSpecification::prepare_with_project(
                &spec,
                &[function("unused"), function("id")],
            )
            .unwrap();
            let table = Table {
                schema: TableSchema::new(vec![Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                }])
                .unwrap(),
                observations: RefCell::default(),
            };
            let plan = prepared.bind(&table.schema).unwrap();
            assert_eq!(prepared.called_functions(), [1]);
            let mut bindings = Bindings {
                signature: prepared.project_calls().unwrap().plans()[0].clone(),
                inputs: Vec::new(),
            };
            for _ in 0..2 {
                let result = plan
                    .execute(&table, &[], &mut bindings, None)
                    .result
                    .unwrap();
                assert_eq!(result.dataset.row_count(), 2);
                for (row, values) in [[1, 7, 7, 7, 7], [2, 9, 9, 7, 9]].iter().enumerate() {
                    for (column, &value) in values.iter().enumerate() {
                        assert_eq!(
                            result.dataset.cell(row, column).unwrap(),
                            ValueRef::Int(value)
                        );
                    }
                }
            }
            assert_eq!(bindings.inputs, [7, 7, 7, 9, 9, 7, 7, 7, 9, 9]);
            assert!(
                table
                    .observations
                    .borrow()
                    .iter()
                    .filter(|&&op| op == "cell")
                    .count()
                    >= 2
            );
        }
    }
    mod project_dataset {
        //! Authored versionless dataset plans and observable table/callback boundaries.
        use std::cell::RefCell;
        use yamaa_core::{
            bound_expression::Read,
            function_signature::{
                InvocationPlan, Parameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
            },
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value, ValueType},
        };
        use yamaa_core::{
            dataset::{
                Assignment, BoundProjectFunction, DatasetPlan, ExecutionError, Expression,
                FunctionArgument, FunctionInput, PlanError, RowMode, RowTemplate,
            },
            function_invocation::{Argument, FailureKind, FunctionPort, HostError},
            project_activation::{ActivatedFunction, ActivationPort, Bindings},
        };
        fn schema() -> TableSchema {
            TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "X".into(),
                    kind: ColumnType::Int,
                },
            ])
            .unwrap()
        }
        struct Table {
            rows: Vec<Vec<Value>>,
            schema: TableSchema,
            observations: RefCell<Vec<String>>,
        }
        impl TableAccess for Table {
            type Error = &'static str;
            fn schema(&self) -> &TableSchema {
                self.observations.borrow_mut().push("schema".into());
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.observations.borrow_mut().push("rows".into());
                self.rows.len()
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.observations
                    .borrow_mut()
                    .push(format!("cell:{row}:{column}"));
                Ok(ValueRef::from(&self.rows[row][column]))
            }
        }
        fn table(values: Vec<Value>) -> Table {
            Table {
                rows: values
                    .into_iter()
                    .enumerate()
                    .map(|(i, x)| vec![Value::Int(i as i64 + 1), x])
                    .collect(),
                schema: schema(),
                observations: RefCell::default(),
            }
        }
        fn signature() -> ProjectInvocationPlan {
            ProjectInvocationPlan::new(
                ProjectFunctionIdentity {
                    name: "identity".into(),
                    call: "installed.identity".into(),
                },
                vec![
                    Parameter {
                        name: "x".into(),
                        host_name: "x".into(),
                        kind: ValueType::Int,
                        presence: Presence::Required,
                        accepts_missing: false,
                    },
                    Parameter {
                        name: "scale".into(),
                        host_name: "scale".into(),
                        kind: ValueType::Int,
                        presence: Presence::Optional(Value::Int(100)),
                        accepts_missing: false,
                    },
                ],
                ColumnType::Int,
                false,
            )
            .unwrap()
        }
        fn argument(input: FunctionInput) -> FunctionArgument {
            FunctionArgument {
                name: "x".into(),
                input,
            }
        }
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("columns.C{column}.derivation.function"),
            }
        }
        fn plan(
            signature: ProjectInvocationPlan,
            args: Vec<FunctionArgument>,
            mode: RowMode,
        ) -> Result<DatasetPlan, PlanError> {
            DatasetPlan::new(
                schema(),
                schema(),
                vec![RowTemplate {
                    mode,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            Expression::ProjectFunction(BoundProjectFunction::new_project(
                                0, signature, args,
                            )?),
                        ),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
        }

        struct Callbacks {
            signature: Option<ProjectInvocationPlan>,
            calls: Vec<Vec<(String, Value)>>,
            result: Result<Value, &'static str>,
        }
        impl FunctionPort for Callbacks {
            type Error = &'static str;
            fn signature(&self, _: usize) -> Option<&InvocationPlan> {
                None
            }
            fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
                if slot == 0 {
                    self.signature.as_ref()
                } else {
                    None
                }
            }
            fn call(
                &mut self,
                slot: usize,
                arguments: &[Argument<'_>],
            ) -> Result<Value, HostError<Self::Error>> {
                assert_eq!(slot, 0);
                self.calls.push(
                    arguments
                        .iter()
                        .map(|a| {
                            (
                                a.name.into(),
                                match a.value {
                                    ValueRef::Int(x) => Value::Int(x),
                                    ValueRef::Float(x) => Value::float(x.get()),
                                    ValueRef::Missing => Value::Missing,
                                    _ => panic!("exact integer args"),
                                },
                            )
                        })
                        .collect(),
                );
                self.result.clone().map_err(HostError::Raised)
            }
        }
        fn callbacks() -> Callbacks {
            Callbacks {
                signature: Some(signature()),
                calls: vec![],
                result: Ok(Value::Int(i64::MAX)),
            }
        }

        #[test]
        fn closed_versionless_slots_defaults_full_i64_and_repeated_invocations() {
            let source = table(vec![Value::Int(i64::MIN), Value::Int(7)]);
            let plan = plan(
                signature(),
                vec![argument(FunctionInput::Read(Read::Source(1)))],
                RowMode::Records,
            )
            .unwrap();
            let mut callbacks = callbacks();
            for _ in 0..2 {
                let result = plan
                    .execute(&source, &[], &mut callbacks, None)
                    .result
                    .unwrap();
                assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Int(i64::MAX));
                assert_eq!(result.dataset.cell(1, 1).unwrap(), ValueRef::Int(i64::MAX));
            }
            assert_eq!(
                callbacks.calls,
                vec![
                    vec![
                        ("x".into(), Value::Int(i64::MIN)),
                        ("scale".into(), Value::Int(100))
                    ],
                    vec![
                        ("x".into(), Value::Int(7)),
                        ("scale".into(), Value::Int(100))
                    ],
                    vec![
                        ("x".into(), Value::Int(i64::MIN)),
                        ("scale".into(), Value::Int(100))
                    ],
                    vec![
                        ("x".into(), Value::Int(7)),
                        ("scale".into(), Value::Int(100))
                    ],
                ]
            );
        }

        #[test]
        fn every_signature_mismatch_precedes_even_schema_cardinality_and_empty_table_observations()
        {
            let plan = plan(
                signature(),
                vec![argument(FunctionInput::Read(Read::Source(1)))],
                RowMode::Records,
            )
            .unwrap();
            for values in [vec![], vec![Value::Int(7)]] {
                for incompatible in [
                    None,
                    Some(
                        ProjectInvocationPlan::new(
                            ProjectFunctionIdentity {
                                name: "other".into(),
                                call: "installed.identity".into(),
                            },
                            signature().signature().parameters().to_vec(),
                            ColumnType::Int,
                            false,
                        )
                        .unwrap(),
                    ),
                    Some(
                        ProjectInvocationPlan::new(
                            signature().identity().clone(),
                            signature().signature().parameters().to_vec(),
                            ColumnType::Float,
                            false,
                        )
                        .unwrap(),
                    ),
                ] {
                    let source = table(values.clone());
                    let mut callbacks = callbacks();
                    callbacks.signature = incompatible;
                    let error = plan
                        .execute(&source, &[], &mut callbacks, None)
                        .result
                        .unwrap_err();
                    assert_eq!(*error, ExecutionError::FunctionBinding { slot: 0 });
                    assert!(source.observations.borrow().is_empty());
                    assert!(callbacks.calls.is_empty());
                }
            }
        }

        #[test]
        fn present_missing_short_circuits_without_host_call_or_default_substitution() {
            let source = table(vec![Value::Missing]);
            let plan = plan(
                signature(),
                vec![argument(FunctionInput::Read(Read::Source(1)))],
                RowMode::Records,
            )
            .unwrap();
            let mut callbacks = callbacks();
            let result = plan
                .execute(&source, &[], &mut callbacks, None)
                .result
                .unwrap();
            assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Missing);
            assert!(callbacks.calls.is_empty());
        }

        #[test]
        fn original_host_failure_keeps_versionless_identity_and_row_without_retry_or_output() {
            let source = table(vec![Value::Int(7), Value::Int(8)]);
            let plan = plan(
                signature(),
                vec![argument(FunctionInput::Read(Read::Source(1)))],
                RowMode::Records,
            )
            .unwrap();
            let mut callbacks = callbacks();
            callbacks.result = Err("original owned host payload");
            let error = plan
                .execute(&source, &[], &mut callbacks, None)
                .result
                .unwrap_err();
            let ExecutionError::ProjectFunction {
                path,
                error,
                identity,
            } = *error
            else {
                panic!("project invocation facts")
            };
            assert_eq!(path, "columns.C1.derivation.function");
            assert_eq!(error.identity, signature().identity().clone());
            assert_eq!(
                error.kind,
                FailureKind::CallFailed("original owned host payload")
            );
            assert_eq!(identity.unwrap().values, vec![Value::Int(1)]);
            assert_eq!(callbacks.calls.len(), 1);
            assert!(!source
                .observations
                .borrow()
                .iter()
                .any(|s| s.starts_with("cell:1:")));
        }

        #[test]
        fn versionless_call_admission_preserves_closed_names_and_group_completed_output_scope() {
            for args in [
                vec![],
                vec![
                    argument(FunctionInput::Literal(Value::Int(7))),
                    argument(FunctionInput::Literal(Value::Int(8))),
                ],
                vec![FunctionArgument {
                    name: "unknown".into(),
                    input: FunctionInput::Literal(Value::Int(7)),
                }],
            ] {
                assert_eq!(
                    plan(signature(), args, RowMode::Records),
                    Err(PlanError::InvalidFunction)
                );
            }
            assert_eq!(
                plan(
                    signature(),
                    vec![argument(FunctionInput::Read(Read::Column(1)))],
                    RowMode::Records
                ),
                Err(PlanError::UnavailableColumn)
            );
            assert_eq!(
                plan(
                    signature(),
                    vec![argument(FunctionInput::Read(Read::Source(1)))],
                    RowMode::Groups(vec![0])
                ),
                Err(PlanError::NonGroupSource)
            );
        }

        #[test]
        fn opposite_zero_default_is_a_binding_mismatch_before_any_table_or_callback_effect() {
            let make = |zero| {
                let mut parameters = signature().signature().parameters().to_vec();
                parameters[1].kind = ValueType::Float;
                parameters[1].presence = Presence::Optional(Value::float(zero));
                ProjectInvocationPlan::new(
                    signature().identity().clone(),
                    parameters,
                    ColumnType::Int,
                    false,
                )
                .unwrap()
            };
            for (compiled, activated) in [(-0.0, 0.0), (0.0, -0.0)] {
                let plan = plan(
                    make(compiled),
                    vec![argument(FunctionInput::Read(Read::Source(1)))],
                    RowMode::Records,
                )
                .unwrap();
                let source = table(vec![Value::Int(7)]);
                let mut callbacks = callbacks();
                callbacks.signature = Some(make(activated));
                let error = plan
                    .execute(&source, &[], &mut callbacks, None)
                    .result
                    .unwrap_err();
                assert_eq!(*error, ExecutionError::FunctionBinding { slot: 0 });
                assert!(source.observations.borrow().is_empty());
                assert!(callbacks.calls.is_empty());
            }
        }

        fn union_signature(name: &str) -> ProjectInvocationPlan {
            ProjectInvocationPlan::new(
                ProjectFunctionIdentity {
                    name: name.into(),
                    call: format!("installed.{name}"),
                },
                signature().signature().parameters().to_vec(),
                ColumnType::Int,
                false,
            )
            .unwrap()
        }
        fn union() -> [ActivatedFunction<&'static str>; 2] {
            ["producer", "consumer"].map(|name| ActivatedFunction {
                plan: union_signature(name),
                handle: name,
                cases: vec![],
            })
        }
        #[derive(Default)]
        struct UnionPort {
            calls: Vec<&'static str>,
            failure: bool,
        }
        impl ActivationPort for UnionPort {
            type Handle = &'static str;
            type Error = &'static str;
            fn verify_lock(
                &mut self,
                _: yamaa_core::project_function::Language,
                _: &yamaa_core::project_environment::LockReference,
                _: &[ProjectFunctionIdentity],
            ) -> Result<(), Self::Error> {
                panic!("projected handles must not reactivate")
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &yamaa_core::function_signature::LogicalSignature,
            ) -> Result<Self::Handle, Self::Error> {
                panic!("projected handles must not rebind")
            }
            fn invoke(
                &mut self,
                handle: &Self::Handle,
                _: &[Argument<'_>],
            ) -> Result<Value, HostError<Self::Error>> {
                self.calls.push(*handle);
                if self.failure {
                    Err(HostError::Raised("original union host failure"))
                } else {
                    Ok(Value::Int(match *handle {
                        "producer" => 71,
                        "consumer" => 19,
                        _ => panic!("independently declared handle"),
                    }))
                }
            }
        }

        #[test]
        fn different_nodes_local_zero_projects_to_their_exact_union_handle() {
            let activated = union();
            let mut port = UnionPort::default();
            // Consumer is local slot zero but graph slot one; producer is graph slot
            // zero. Distinct host results make accidental identity projection observable.
            for (name, global, expected) in [("consumer", 1, 19), ("producer", 0, 71)] {
                let node_plan = plan(
                    union_signature(name),
                    vec![argument(FunctionInput::Read(Read::Source(1)))],
                    RowMode::Records,
                )
                .unwrap();
                let source = table(vec![Value::Int(7)]);
                let projection = [global];
                let plans = [union_signature(name)];
                let mut bindings =
                    Bindings::projected(&activated, &projection, &plans, &mut port).unwrap();
                assert!(bindings.project_signature(1).is_none());
                let result = node_plan
                    .execute(&source, &[], &mut bindings, None)
                    .result
                    .unwrap();
                assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Int(expected));
            }
            assert_eq!(port.calls, ["consumer", "producer"]);
        }

        #[test]
        fn projection_admission_rejects_shape_slot_identity_and_complete_signature_without_host_effects(
        ) {
            let activated = union();
            let consumer = [union_signature("consumer")];
            for projection in [vec![], vec![0], vec![2], vec![1, 0]] {
                let mut port = UnionPort::default();
                assert!(
                    Bindings::projected(&activated, &projection, &consumer, &mut port).is_err()
                );
                assert!(port.calls.is_empty());
            }
            let changed = ProjectInvocationPlan::new(
                consumer[0].identity().clone(),
                consumer[0].signature().parameters().to_vec(),
                ColumnType::Float,
                false,
            )
            .unwrap();
            let mut port = UnionPort::default();
            assert!(matches!(
                Bindings::projected(&activated, &[1], &[changed], &mut port),
                Err(0)
            ));
            assert!(port.calls.is_empty());
        }

        #[test]
        fn projected_handles_still_preflight_the_actual_node_before_any_table_observation() {
            let activated = union();
            let expected = [union_signature("producer")];
            let mut port = UnionPort::default();
            let mut bindings = Bindings::projected(&activated, &[0], &expected, &mut port).unwrap();
            let consumer = plan(
                union_signature("consumer"),
                vec![argument(FunctionInput::Read(Read::Source(1)))],
                RowMode::Records,
            )
            .unwrap();
            let source = table(vec![Value::Int(7)]);
            let error = consumer
                .execute(&source, &[], &mut bindings, None)
                .result
                .unwrap_err();
            assert_eq!(*error, ExecutionError::FunctionBinding { slot: 0 });
            assert!(source.observations.borrow().is_empty());
            assert!(port.calls.is_empty());
        }

        #[test]
        fn projected_invocations_preserve_missing_short_circuit_and_original_failure_without_replay(
        ) {
            let activated = union();
            let expected = [union_signature("consumer")];
            let node_plan = plan(
                union_signature("consumer"),
                vec![argument(FunctionInput::Read(Read::Source(1)))],
                RowMode::Records,
            )
            .unwrap();
            let mut port = UnionPort::default();
            let source = table(vec![Value::Missing]);
            let mut bindings = Bindings::projected(&activated, &[1], &expected, &mut port).unwrap();
            let result = node_plan
                .execute(&source, &[], &mut bindings, None)
                .result
                .unwrap();
            assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Missing);
            assert!(port.calls.is_empty());

            port.failure = true;
            let mut bindings = Bindings::projected(&activated, &[1], &expected, &mut port).unwrap();
            let source = table(vec![Value::Int(7), Value::Int(8)]);
            let error = node_plan
                .execute(&source, &[], &mut bindings, None)
                .result
                .unwrap_err();
            assert!(
                matches!(*error, ExecutionError::ProjectFunction { error, .. }
        if error.kind == FailureKind::CallFailed("original union host failure"))
            );
            assert_eq!(port.calls, ["consumer"]);
        }
    }
    mod project_environment {
        use yamaa_core::{
            project_environment::{
                validate, Draft, Environment, Finding, LockKind, LockReference, Submission,
            },
            project_function::{Case, Definition, Language},
            value::{ColumnType, Value},
        };
        fn draft() -> Draft {
            Draft {
                language: None,
                lock: None,
                functions: None,
                codelists: vec![],
                has_study: false,
                submissions: vec![],
            }
        }
        fn definition() -> Definition {
            Definition {
                name: "constant".into(),
                function: "project.constant".into(),
                description: "Constant integer.".into(),
                params: vec![],
                returns: ColumnType::Int,
                may_return_missing: false,
                comparison_decimals: 4,
                tests: vec![Case {
                    id: "ordinary".into(),
                    covers: vec!["normal".into(), "boundary".into()],
                    args: vec![],
                    result: Value::Int(1),
                }],
            }
        }
        #[test]
        fn empty_environment_and_submission_only_environments_need_no_code_or_lock() {
            assert!(Environment::admit(Language::Python, draft()).is_ok());
            let mut submission = draft();
            submission.has_study = true;
            submission.submissions = vec![Submission::Sdtm, Submission::Adam, Submission::Send];
            assert!(Environment::admit(Language::R, submission).is_ok());
        }
        #[test]
        fn present_functions_even_empty_require_language_and_lock() {
            let mut d = draft();
            d.functions = Some(vec![]);
            assert_eq!(
                validate(Language::Python, &d),
                vec![Finding::MissingLanguage, Finding::MissingLock]
            );
            d.functions = None;
            d.language = Some(Language::Python);
            assert!(validate(Language::Python, &d).is_empty());
        }
        #[test]
        fn declared_language_and_captured_lock_kind_are_independent_findings() {
            let mut d = draft();
            d.language = Some(Language::R);
            d.lock = Some(LockReference {
                written: "custom-name.lock".into(),
                kind: LockKind::Uv,
            });
            assert_eq!(
                validate(Language::Python, &d),
                vec![
                    Finding::LanguageMismatch {
                        declared: Language::R,
                        host: Language::Python
                    },
                    Finding::LockKindMismatch {
                        language: Language::R,
                        actual: LockKind::Uv
                    }
                ]
            );
            d.language = Some(Language::Python);
            assert!(validate(Language::Python, &d).is_empty());
        }
        #[test]
        fn all_function_and_root_faults_remain_owned_without_any_effect_port() {
            let mut d = draft();
            d.functions = Some(vec![definition(), definition()]);
            d.submissions = vec![Submission::Adam];
            assert_eq!(
                validate(Language::Python, &d),
                vec![
                    Finding::MissingLanguage,
                    Finding::MissingLock,
                    Finding::DuplicateFunction { function: 1 },
                    Finding::MissingStudy
                ]
            );
            d.language = Some(Language::Python);
            d.lock = Some(LockReference {
                written: "../uv.lock".into(),
                kind: LockKind::Uv,
            });
            d.functions.as_mut().unwrap().pop();
            d.has_study = true;
            let expected = d.clone();
            let admitted = Environment::admit(Language::Python, d).unwrap();
            assert_eq!(admitted.draft(), &expected);
        }

        #[test]
        fn execution_conversion_moves_exact_definitions_and_preserves_absent_or_empty_functions() {
            let empty = Environment::admit(Language::R, draft())
                .unwrap()
                .into_execution();
            assert_eq!(empty.host(), Language::R);
            assert_eq!(empty.language(), None);
            assert!(!empty.functions_present());
            assert!(empty.functions().is_empty());
            assert!(empty.lock().is_none());
            let mut d = draft();
            d.language = Some(Language::Python);
            d.lock = Some(LockReference {
                written: "uv.lock".into(),
                kind: LockKind::Uv,
            });
            d.functions = Some(vec![]);
            let empty = Environment::admit(Language::Python, d.clone())
                .unwrap()
                .into_execution();
            assert!(empty.functions_present());
            assert!(empty.functions().is_empty());
            d.functions = Some(vec![definition()]);
            d.has_study = true;
            d.submissions = vec![Submission::Adam];
            let expected = d.clone();
            let original_name = d.functions.as_ref().unwrap()[0].name.as_ptr();
            let original_cases = d.functions.as_ref().unwrap()[0].tests.as_ptr();
            let execution = Environment::admit(Language::Python, d)
                .unwrap()
                .into_execution();
            assert_eq!(execution.language(), Some(Language::Python));
            assert_eq!(execution.lock(), expected.lock.as_ref());
            assert_eq!(
                execution.functions()[0].definition(),
                &expected.functions.as_ref().unwrap()[0]
            );
            assert_eq!(
                execution.functions()[0].definition().name.as_ptr(),
                original_name
            );
            assert_eq!(
                execution.functions()[0].definition().tests.as_ptr(),
                original_cases
            );
            assert_eq!(execution.functions()[0].language(), Language::Python);
            assert!(execution.has_study());
            assert_eq!(execution.submissions(), [Submission::Adam]);
            assert!(execution.catalogue().sources().is_empty());
        }

        #[test]
        fn rejected_admission_retains_exact_draft_ownership_and_all_findings() {
            let mut input = draft();
            input.functions = Some(vec![definition(), definition()]);
            input.submissions = vec![Submission::Adam];
            let expected = input.clone();
            let functions = input.functions.as_ref().unwrap().as_ptr();
            let cases = input.functions.as_ref().unwrap()[0].tests.as_ptr();
            let rejected = Environment::admit_retained(Language::Python, input).unwrap_err();
            assert_eq!(*rejected.draft, expected);
            assert_eq!(
                rejected.draft.functions.as_ref().unwrap().as_ptr(),
                functions
            );
            assert_eq!(
                rejected.draft.functions.as_ref().unwrap()[0].tests.as_ptr(),
                cases
            );
            assert_eq!(
                rejected.error,
                vec![
                    Finding::MissingLanguage,
                    Finding::MissingLock,
                    Finding::DuplicateFunction { function: 1 },
                    Finding::MissingStudy
                ]
            );
        }
    }
    mod project_environment_diagnostics {
        use crate::support::project_environment as support;
        use serde_json::Value as JsonValue;
        use yamaa_core::{
            project_environment::{self, Draft, Finding, LockKind, LockReference},
            project_environment_diagnostics::{diagnostics, Error},
            project_function::{Case, Definition, Finding as F, Language, Parameter},
            value::{ColumnType, Value, ValueType},
        };
        #[test]
        fn classified_unavailable_root_maps_to_the_supplied_environment_requirement() {
            use yamaa_core::project_environment_diagnostics::ReadFailure;
            for (failure, cause) in [
                (ReadFailure::Missing, "missing"),
                (ReadFailure::Unreadable, "unreadable"),
            ] {
                let diagnostic = failure.diagnostic("/study/environment.yaml");
                let definition = diagnostic.code.definition();
                assert_eq!(
                    (
                        definition.phase,
                        definition.condition,
                        definition.requirement
                    ),
                    (
                        "validation",
                        "project_environment_missing",
                        Some("REQ-0694")
                    )
                );
                assert_eq!(diagnostic.spec_paths, ["environment"]);
                assert_eq!(diagnostic.context["cause"], JsonValue::String(cause.into()));
                assert_eq!(
                    diagnostic.context["path"],
                    JsonValue::String("/study/environment.yaml".into())
                );
            }
        }
        #[test]
        fn every_root_and_terminology_cause_has_independent_normative_paths_and_mapping() {
            assert_eq!(support::reached().len(), 9);
            let draft = support::draft();
            let projected = diagnostics(
                &draft,
                &project_environment::validate(Language::Python, &draft),
            )
            .unwrap();
            assert_eq!(projected[3].context["value"], JsonValue::Bool(true));
            assert_eq!(projected[1].context.len(), 2);
            assert_eq!(
                projected[1].context["runner"],
                JsonValue::String("python".into())
            );
            assert_eq!(
                projected[7].context["field"],
                JsonValue::String("decode".into())
            );
            assert_eq!(
                projected[8].context["field"],
                JsonValue::String("rank".into())
            );
        }
        fn draft() -> Draft {
            Draft {
                language: Some(Language::Python),
                lock: Some(LockReference {
                    written: "uv.lock".into(),
                    kind: LockKind::Uv,
                }),
                functions: Some(vec![Definition {
                    name: "id".into(),
                    function: "project.id".into(),
                    description: "Identity".into(),
                    params: vec![Parameter {
                        name: "x".into(),
                        kind: ValueType::Int,
                        required: true,
                        default: None,
                        accepts_missing: false,
                    }],
                    returns: ColumnType::Int,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![Case {
                        id: "ordinary".into(),
                        covers: vec!["normal".into()],
                        args: vec![("x".into(), Value::Int(i64::MIN))],
                        result: Value::Int(i64::MAX),
                    }],
                }]),
                codelists: vec![],
                has_study: false,
                submissions: vec![],
            }
        }
        #[test]
        fn every_function_field_route_preserves_authored_indices_and_exact_scalar_kinds() {
            let cases = [
                (F::InvalidName, "functions.id", "invalid_function_name"),
                (
                    F::InvalidCallable,
                    "functions.id.function",
                    "invalid_callable",
                ),
                (
                    F::EmptyDescription,
                    "functions.id.description",
                    "empty_description",
                ),
                (
                    F::NegativeComparisonDecimals,
                    "functions.id.comparison_decimals",
                    "negative_comparison_decimals",
                ),
                (
                    F::InvalidParameterName { parameter: 0 },
                    "functions.id.params[0].name",
                    "invalid_parameter_name",
                ),
                (
                    F::DuplicateParameter { parameter: 0 },
                    "functions.id.params[0].name",
                    "duplicate_parameter",
                ),
                (
                    F::RequiredDefault { parameter: 0 },
                    "functions.id.params[0].default",
                    "required_parameter_default",
                ),
                (
                    F::MissingDefault { parameter: 0 },
                    "functions.id.params[0].default",
                    "missing_parameter_default",
                ),
                (
                    F::InvalidDefault { parameter: 0 },
                    "functions.id.params[0].default",
                    "invalid_parameter_default",
                ),
                (
                    F::InvalidCaseId { case: 0 },
                    "functions.id.tests[0].id",
                    "invalid_case_id",
                ),
                (
                    F::DuplicateCaseId { case: 0 },
                    "functions.id.tests[0].id",
                    "duplicate_case_id",
                ),
                (
                    F::EmptyCoverage { case: 0 },
                    "functions.id.tests[0].covers",
                    "empty_coverage",
                ),
                (
                    F::DuplicateCoverage { case: 0, tag: 0 },
                    "functions.id.tests[0].covers[0]",
                    "duplicate_coverage",
                ),
                (
                    F::InvalidCoverage { case: 0, tag: 0 },
                    "functions.id.tests[0].covers[0]",
                    "invalid_coverage",
                ),
                (
                    F::UnsupportedEvidence { case: 0, tag: 0 },
                    "functions.id.tests[0].covers[0]",
                    "unsupported_coverage_evidence",
                ),
                (
                    F::DuplicateArgument {
                        case: 0,
                        argument: 0,
                    },
                    "functions.id.tests[0].args.x",
                    "duplicate_case_argument",
                ),
                (
                    F::UnknownArgument {
                        case: 0,
                        argument: 0,
                    },
                    "functions.id.tests[0].args.x",
                    "unknown_case_argument",
                ),
                (
                    F::MissingRequiredArgument {
                        case: 0,
                        parameter: 0,
                    },
                    "functions.id.tests[0].args.x",
                    "missing_required_case_argument",
                ),
                (
                    F::ArgumentType {
                        case: 0,
                        argument: 0,
                    },
                    "functions.id.tests[0].args.x",
                    "invalid_case_argument_type",
                ),
                (
                    F::InvalidResult { case: 0 },
                    "functions.id.tests[0].result",
                    "invalid_case_result",
                ),
                (
                    F::MissingObligation("boundary".into()),
                    "functions.id.tests",
                    "missing_coverage_obligation",
                ),
            ];
            let draft = draft();
            for (finding, path, reason) in cases {
                let result = diagnostics(
                    &draft,
                    &[Finding::Function {
                        function: 0,
                        finding,
                    }],
                )
                .unwrap();
                assert_eq!(result.len(), 1);
                let d = &result[0];
                assert_eq!(
                    (d.definition().condition, d.definition().requirement),
                    ("project_environment_invalid", Some("REQ-0695"))
                );
                assert_eq!(d.spec_paths, [path]);
                assert_eq!(d.context["reason"], JsonValue::String(reason.into()));
                if path.ends_with(".args.x") && reason != "missing_required_case_argument" {
                    assert_eq!(d.context["value"], JsonValue::from(i64::MIN));
                }
                if path.ends_with(".result") {
                    assert_eq!(d.context["value"], JsonValue::from(i64::MAX));
                }
            }
        }
        #[test]
        fn repeated_findings_keep_complete_context_without_a_copy_quota() {
            use yamaa_core::diagnostic::{ConditionCode, Diagnostic};
            let mut draft = draft();
            let name = "f".repeat(16_384);
            draft.functions.as_mut().unwrap()[0].name = name.clone();
            let findings = vec![
                Finding::Function {
                    function: 0,
                    finding: F::InvalidCallable,
                };
                600
            ];
            let actual = diagnostics(&draft, &findings).unwrap();
            let expected = Diagnostic {
                code: ConditionCode::ProjectEnvironmentInvalid,
                spec_paths: vec![format!("functions.{name}.function")],
                context: [
                    ("function".into(), JsonValue::String(name)),
                    (
                        "reason".into(),
                        JsonValue::String("invalid_callable".into()),
                    ),
                ]
                .into(),
                source_span: None,
                operand_route: None,
            };
            assert_eq!(actual.len(), findings.len());
            assert!(actual.iter().all(|diagnostic| diagnostic == &expected));
        }
        #[test]
        fn invalid_held_indices_refuse_projection_without_inventing_a_semantic_diagnostic() {
            let draft = draft();
            for finding in [
                Finding::DuplicateFunction { function: 1 },
                Finding::Function {
                    function: 0,
                    finding: F::ArgumentType {
                        case: 0,
                        argument: 1,
                    },
                },
                Finding::Function {
                    function: 0,
                    finding: F::MissingRequiredArgument {
                        case: 0,
                        parameter: 1,
                    },
                },
            ] {
                assert_eq!(diagnostics(&draft, &[finding]), Err(Error::InvalidIndex));
            }
        }

        #[test]
        fn retained_admission_reports_all_independent_root_errors_and_captured_lock_syntax() {
            use yamaa_core::project_environment::{Environment, Submission};
            let mut d = draft();
            d.language = None;
            d.lock = None;
            d.functions = Some(vec![]);
            d.submissions = vec![Submission::Adam];
            let rejection = Environment::admit_retained(Language::Python, d).unwrap_err();
            let projected = diagnostics(&rejection.draft, &rejection.error).unwrap();
            assert_eq!(projected.len(), 3);
            for (finding, (path, reason)) in projected.iter().zip([
                ("language", "missing_language"),
                ("lock", "missing_lock"),
                ("study", "missing_study"),
            ]) {
                assert_eq!(finding.spec_paths, [path]);
                assert_eq!(finding.context["reason"], JsonValue::String(reason.into()));
                assert_eq!(finding.context.len(), 1);
            }
            let mut d = draft();
            d.functions = None;
            d.language = Some(Language::R);
            let findings = project_environment::validate(Language::Python, &d);
            let projected = diagnostics(&d, &findings).unwrap();
            assert_eq!(projected.len(), 2);
            assert_eq!(projected[1].spec_paths, ["lock"]);
            assert_eq!(
                projected[1].definition().condition,
                "project_environment_invalid"
            );
            assert_eq!(
                projected[1].context["actual_lock_kind"],
                JsonValue::String("uv".into())
            );
            assert_eq!(
                projected[1].context["language"],
                JsonValue::String("r".into())
            );
            assert_eq!(
                projected[1].context["reason"],
                JsonValue::String("lock_kind_mismatch".into())
            );
        }
    }
    mod project_environment_projection {
        use crate::support::project_environment as support;
        use serde_json::Value as CV;
        use std::collections::BTreeMap;
        use yamaa_core::{
            diagnostic::{ConditionCode as C, Diagnostic},
            project_environment::{self, Draft, Finding, LockKind, LockReference},
            project_environment_diagnostics::{
                self,
                projection::{self as p, Admission, Capture, Error, Origin, Root, ScalarSource},
            },
            project_function::{Case, Definition, Finding as F, Language, Parameter},
            project_function_document::{Finding as SF, Kind},
            schema::{Document, DocumentNode as N},
            value::{ColumnType, Value, ValueType},
        };
        fn document(nodes: Vec<N>) -> Document {
            let root = nodes.len() - 1;
            Document::new(nodes, root).unwrap()
        }
        fn root_document(functions: &[(&str, bool)], codelists: &[bool]) -> Document {
            let mut nodes = Vec::new();
            let mut entries = Vec::new();
            for (name, external) in functions {
                let key = nodes.len();
                nodes.push(N::Text((*name).into()));
                let value = nodes.len();
                nodes.push(if *external {
                    N::Text("function.yaml".into())
                } else {
                    N::Mapping(vec![])
                });
                entries.push((key, value));
            }
            let functions = nodes.len();
            nodes.push(N::Mapping(entries));
            let function_key = nodes.len();
            nodes.push(N::Text("functions".into()));
            let mut entries = Vec::new();
            for external in codelists {
                entries.push(nodes.len());
                nodes.push(if *external {
                    N::Text("ct.yaml".into())
                } else {
                    N::Mapping(vec![])
                });
            }
            let lists = nodes.len();
            nodes.push(N::Sequence(entries));
            let list_key = nodes.len();
            nodes.push(N::Text("codelists".into()));
            nodes.push(N::Mapping(vec![
                (function_key, functions),
                (list_key, lists),
            ]));
            document(nodes)
        }
        fn text(value: &str) -> CV {
            CV::String(value.into())
        }

        #[test]
        fn captured_reports_keep_all_origins_beyond_the_old_descriptor_array() {
            let mut draft = draft();
            let template = draft.functions.take().unwrap().remove(0);
            let names = (0..129)
                .map(|index| format!("function_{index}"))
                .collect::<Vec<_>>();
            draft.functions = Some(
                names
                    .iter()
                    .map(|name| {
                        let mut definition = template.clone();
                        definition.name = name.clone();
                        definition
                    })
                    .collect(),
            );
            let doc = root_document(
                &names
                    .iter()
                    .map(|name| (name.as_str(), true))
                    .collect::<Vec<_>>(),
                &[],
            );
            let captured = document(vec![
                N::Integer("9223372036854775808".into()),
                N::Text("result".into()),
                N::Mapping(vec![(1, 0)]),
            ]);
            let identities = (0..names.len())
                .map(|index| format!("held/function_{index}.yaml"))
                .collect::<Vec<_>>();
            let captures = names
                .iter()
                .zip(&identities)
                .map(|(name, identity)| Capture {
                    origin: Origin::Function(name),
                    document: &captured,
                    identity,
                })
                .collect::<Vec<_>>();
            let findings = (0..names.len())
                .map(|function| Finding::Function {
                    function,
                    finding: F::InvalidCallable,
                })
                .collect::<Vec<_>>();
            let root = Root {
                document: &doc,
                identity: "held/environment.yaml",
            };
            let report = p::admit_admission(
                Admission {
                    root,
                    draft: &draft,
                    function_origins: &names,
                    codelist_origins: &[],
                    captures: captures.iter().copied(),
                },
                &findings,
            )
            .unwrap();
            let want = names
                .iter()
                .zip(&identities)
                .map(|(name, identity)| {
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "function",
                        &[
                            ("function", text(name)),
                            ("reason", text("invalid_callable")),
                        ],
                        identity,
                        &format!("functions.{name}"),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(report.diagnostics().unwrap(), want);

            let scalar = [SF {
                node: 0,
                path: "result".into(),
                kind: Kind::IntegerRange,
            }];
            let report = p::admit_scalar_sources(
                root,
                captures.iter().map(|capture| ScalarSource {
                    capture: *capture,
                    findings: &scalar,
                }),
            )
            .unwrap();
            let want = names
                .iter()
                .zip(&identities)
                .map(|(name, identity)| {
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "result",
                        &[
                            ("reason", text("integer_out_of_range")),
                            ("value", CV::Number("9223372036854775808".parse().unwrap())),
                        ],
                        identity,
                        &format!("functions.{name}"),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(report.diagnostics().unwrap(), want);
        }
        fn expected(
            code: C,
            path: &str,
            fields: &[(&str, CV)],
            source: &str,
            declaration: &str,
        ) -> Diagnostic {
            let mut context: BTreeMap<String, CV> = fields
                .iter()
                .map(|(name, value)| ((*name).into(), value.clone()))
                .collect();
            context.extend([
                ("source".into(), text(source)),
                ("entry".into(), text("held/environment.yaml")),
                ("environment_path".into(), text(declaration)),
            ]);
            Diagnostic {
                code,
                spec_paths: vec![path.into()],
                context,
                source_span: None,
                operand_route: None,
            }
        }
        fn draft() -> Draft {
            Draft {
                language: Some(Language::Python),
                lock: Some(LockReference {
                    written: "uv.lock".into(),
                    kind: LockKind::Uv,
                }),
                functions: Some(vec![Definition {
                    name: "id".into(),
                    function: "project.id".into(),
                    description: "Identity".into(),
                    params: vec![Parameter {
                        name: "x".into(),
                        kind: ValueType::Int,
                        required: true,
                        default: None,
                        accepts_missing: false,
                    }],
                    returns: ColumnType::Int,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![Case {
                        id: "ordinary".into(),
                        covers: vec!["normal".into()],
                        args: vec![("x".into(), Value::Int(i64::MIN))],
                        result: Value::Int(i64::MAX),
                    }],
                }]),
                codelists: vec![],
                has_study: false,
                submissions: vec![],
            }
        }
        #[test]
        fn independent_complete_eighteen_record_environment_report_preserves_every_field_and_order()
        {
            assert_eq!(support::reached().len(), 9);
            let draft = support::draft();
            let doc = root_document(&[], &[false]);
            let findings = project_environment::validate(Language::Python, &draft);
            let origins = [0];
            let result = p::admit_admission(
                Admission {
                    root: Root {
                        document: &doc,
                        identity: "held/environment.yaml",
                    },
                    draft: &draft,
                    function_origins: &[],
                    codelist_origins: &origins,
                    captures: std::iter::empty(),
                },
                &findings,
            )
            .unwrap()
            .diagnostics()
            .unwrap();
            let mut want = vec![
                expected(
                    C::ProjectEnvironmentInvalid,
                    "lock",
                    &[("reason", text("missing_lock"))],
                    "held/environment.yaml",
                    "lock",
                ),
                expected(
                    C::RunnerLanguageMismatch,
                    "language",
                    &[("declared", text("r")), ("runner", text("python"))],
                    "held/environment.yaml",
                    "language",
                ),
            ];
            for list in 0..2 {
                let base = format!("codelists[0].codelists[{list}]");
                if list == 1 {
                    want.push(expected(
                        C::CodelistDuplicateIdentifier,
                        &format!("{base}.id"),
                        &[("codelist", text("SEX")), ("field", text("id"))],
                        "held/environment.yaml",
                        "codelists[0]",
                    ));
                    want.push(expected(
                        C::CodelistDuplicateIdentifier,
                        &format!("{base}.name"),
                        &[
                            ("codelist", text("SEX")),
                            ("field", text("name")),
                            ("name", text("Sex")),
                        ],
                        "held/environment.yaml",
                        "codelists[0]",
                    ));
                }
                want.push(expected(
                    C::CodelistInvalidShape,
                    &base,
                    &[("codelist", text("SEX"))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistInvalidValue,
                    &format!("{base}.items[0].value"),
                    &[("codelist", text("SEX")), ("value", CV::Bool(true))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistExtensionNotAdmitted,
                    &format!("{base}.items[0].extended"),
                    &[("codelist", text("SEX"))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistInvalidValue,
                    &format!("{base}.items[1].value"),
                    &[("codelist", text("SEX")), ("value", CV::Bool(true))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistDuplicateValue,
                    &format!("{base}.items[1].value"),
                    &[("codelist", text("SEX")), ("value", CV::Bool(true))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistPartialItemField,
                    &base,
                    &[("codelist", text("SEX")), ("field", text("decode"))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistPartialItemField,
                    &base,
                    &[("codelist", text("SEX")), ("field", text("rank"))],
                    "held/environment.yaml",
                    "codelists[0]",
                ));
            }
            assert_eq!(result, want);
            assert_eq!(result.len(), 18);
        }
        #[test]
        fn every_function_route_has_complete_independent_inline_and_external_context() {
            let cases = [
                (F::InvalidName, "", "invalid_function_name", vec![]),
                (F::InvalidCallable, "function", "invalid_callable", vec![]),
                (
                    F::EmptyDescription,
                    "description",
                    "empty_description",
                    vec![],
                ),
                (
                    F::NegativeComparisonDecimals,
                    "comparison_decimals",
                    "negative_comparison_decimals",
                    vec![],
                ),
                (
                    F::InvalidParameterName { parameter: 0 },
                    "params[0].name",
                    "invalid_parameter_name",
                    vec![("parameter", text("x"))],
                ),
                (
                    F::DuplicateParameter { parameter: 0 },
                    "params[0].name",
                    "duplicate_parameter",
                    vec![("parameter", text("x"))],
                ),
                (
                    F::RequiredDefault { parameter: 0 },
                    "params[0].default",
                    "required_parameter_default",
                    vec![("parameter", text("x"))],
                ),
                (
                    F::MissingDefault { parameter: 0 },
                    "params[0].default",
                    "missing_parameter_default",
                    vec![("parameter", text("x"))],
                ),
                (
                    F::InvalidDefault { parameter: 0 },
                    "params[0].default",
                    "invalid_parameter_default",
                    vec![("parameter", text("x"))],
                ),
                (
                    F::InvalidCaseId { case: 0 },
                    "tests[0].id",
                    "invalid_case_id",
                    vec![("case", text("ordinary"))],
                ),
                (
                    F::DuplicateCaseId { case: 0 },
                    "tests[0].id",
                    "duplicate_case_id",
                    vec![("case", text("ordinary"))],
                ),
                (
                    F::EmptyCoverage { case: 0 },
                    "tests[0].covers",
                    "empty_coverage",
                    vec![("case", text("ordinary"))],
                ),
                (
                    F::DuplicateCoverage { case: 0, tag: 0 },
                    "tests[0].covers[0]",
                    "duplicate_coverage",
                    vec![("case", text("ordinary")), ("tag", text("normal"))],
                ),
                (
                    F::InvalidCoverage { case: 0, tag: 0 },
                    "tests[0].covers[0]",
                    "invalid_coverage",
                    vec![("case", text("ordinary")), ("tag", text("normal"))],
                ),
                (
                    F::UnsupportedEvidence { case: 0, tag: 0 },
                    "tests[0].covers[0]",
                    "unsupported_coverage_evidence",
                    vec![("case", text("ordinary")), ("tag", text("normal"))],
                ),
                (
                    F::DuplicateArgument {
                        case: 0,
                        argument: 0,
                    },
                    "tests[0].args.x",
                    "duplicate_case_argument",
                    vec![
                        ("case", text("ordinary")),
                        ("argument", text("x")),
                        ("value", CV::from(i64::MIN)),
                    ],
                ),
                (
                    F::UnknownArgument {
                        case: 0,
                        argument: 0,
                    },
                    "tests[0].args.x",
                    "unknown_case_argument",
                    vec![
                        ("case", text("ordinary")),
                        ("argument", text("x")),
                        ("value", CV::from(i64::MIN)),
                    ],
                ),
                (
                    F::MissingRequiredArgument {
                        case: 0,
                        parameter: 0,
                    },
                    "tests[0].args.x",
                    "missing_required_case_argument",
                    vec![("case", text("ordinary")), ("argument", text("x"))],
                ),
                (
                    F::ArgumentType {
                        case: 0,
                        argument: 0,
                    },
                    "tests[0].args.x",
                    "invalid_case_argument_type",
                    vec![
                        ("case", text("ordinary")),
                        ("argument", text("x")),
                        ("value", CV::from(i64::MIN)),
                    ],
                ),
                (
                    F::InvalidResult { case: 0 },
                    "tests[0].result",
                    "invalid_case_result",
                    vec![("case", text("ordinary")), ("value", CV::from(i64::MAX))],
                ),
                (
                    F::MissingObligation("boundary".into()),
                    "tests",
                    "missing_coverage_obligation",
                    vec![("obligation", text("boundary"))],
                ),
            ];
            for external in [false, true] {
                let draft = draft();
                let doc = root_document(&[("id", external)], &[]);
                let captured = document(vec![N::Mapping(vec![])]);
                let names = ["id".into()];
                let captures = if external {
                    vec![Capture {
                        origin: Origin::Function("id"),
                        document: &captured,
                        identity: "held/environment.yaml",
                    }]
                } else {
                    vec![]
                };
                let findings = cases
                    .iter()
                    .map(|(finding, ..)| Finding::Function {
                        function: 0,
                        finding: finding.clone(),
                    })
                    .collect::<Vec<_>>();
                let result = p::admit_admission(
                    Admission {
                        root: Root {
                            document: &doc,
                            identity: "held/environment.yaml",
                        },
                        draft: &draft,
                        function_origins: &names,
                        codelist_origins: &[],
                        captures: captures.into_iter(),
                    },
                    &findings,
                )
                .unwrap()
                .diagnostics()
                .unwrap();
                let want = cases
                    .iter()
                    .map(|(_, suffix, reason, context)| {
                        let path = if suffix.is_empty() {
                            "functions.id".into()
                        } else if external {
                            (*suffix).into()
                        } else {
                            format!("functions.id.{suffix}")
                        };
                        let mut fields = vec![("function", text("id")), ("reason", text(reason))];
                        fields.extend(context.iter().cloned());
                        expected(
                            C::ProjectEnvironmentInvalid,
                            &path,
                            &fields,
                            "held/environment.yaml",
                            "functions.id",
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(result, want);
                for d in result {
                    assert_eq!(
                        (
                            d.definition().phase,
                            d.definition().condition,
                            d.definition().requirement
                        ),
                        (
                            "validation",
                            "project_environment_invalid",
                            Some("REQ-0695")
                        )
                    );
                }
            }
        }
        #[test]
        fn retained_findings_keep_complete_context_without_a_copy_quota() {
            let mut draft = draft();
            let name = "f".repeat(16_384);
            draft.functions.as_mut().unwrap()[0].name = name.clone();
            let doc = root_document(&[(&name, false)], &[]);
            let names = [name.clone()];
            let findings = vec![
                Finding::Function {
                    function: 0,
                    finding: F::InvalidCallable,
                };
                600
            ];
            let report = p::admit_admission(
                Admission {
                    root: Root {
                        document: &doc,
                        identity: "held/environment.yaml",
                    },
                    draft: &draft,
                    function_origins: &names,
                    codelist_origins: &[],
                    captures: std::iter::empty(),
                },
                &findings,
            )
            .unwrap();
            let actual = report.diagnostics().unwrap();
            let expected = expected(
                C::ProjectEnvironmentInvalid,
                &format!("functions.{name}.function"),
                &[
                    ("function", text(&name)),
                    ("reason", text("invalid_callable")),
                ],
                "held/environment.yaml",
                &format!("functions.{name}"),
            );
            assert_eq!(actual.len(), findings.len());
            assert!(actual.iter().all(|diagnostic| diagnostic == &expected));
        }
        #[test]
        fn malformed_origin_relationships_and_wrong_held_nodes_refuse_explicitly() {
            let draft = draft();
            let inline = root_document(&[("id", false)], &[]);
            let external = root_document(&[("id", true)], &[]);
            let captured = document(vec![N::Mapping(vec![])]);
            let names = ["id".into()];
            let wrong_names = ["other".into()];
            let findings = [Finding::MissingLock];
            let capture = Capture {
                origin: Origin::Function("id"),
                document: &captured,
                identity: "held/environment.yaml",
            };
            for (root, origins, captures) in [
                (&inline, &wrong_names, vec![]),
                (&inline, &names, vec![capture]),
                (&external, &names, vec![]),
                (&external, &names, vec![capture, capture]),
            ] {
                assert!(matches!(
                    p::admit_admission(
                        Admission {
                            root: Root {
                                document: root,
                                identity: "held/environment.yaml"
                            },
                            draft: &draft,
                            function_origins: origins,
                            codelist_origins: &[],
                            captures: captures.into_iter()
                        },
                        &findings
                    ),
                    Err(Error::InvalidOrigin)
                ));
            }
            let scalar = [SF {
                path: "result".into(),
                node: 100,
                kind: Kind::IntegerRange,
            }];
            assert!(matches!(
                p::admit_root_scalars(
                    Root {
                        document: &inline,
                        identity: "held/environment.yaml"
                    },
                    &scalar,
                    None
                ),
                Err(Error::Core(
                    project_environment_diagnostics::Error::InvalidIndex
                ))
            ));
            let scalar = [SF {
                path: "result".into(),
                node: inline.root(),
                kind: Kind::IntegerRange,
            }];
            assert!(matches!(
                p::admit_root_scalars(
                    Root {
                        document: &inline,
                        identity: "held/environment.yaml"
                    },
                    &scalar,
                    None
                ),
                Err(Error::Core(
                    project_environment_diagnostics::Error::UnavailableShape
                ))
            ));
        }
        #[test]
        fn root_headers_and_wide_scalars_are_one_admitted_complete_report() {
            let wide = "123456789012345678901234567890";
            let doc = document(vec![
                N::Integer(wide.into()),
                N::Text("result".into()),
                N::Mapping(vec![(1, 0)]),
                N::Text("id".into()),
                N::Mapping(vec![(3, 2)]),
                N::Text("functions".into()),
                N::Mapping(vec![(5, 4)]),
            ]);
            let findings = [SF {
                node: 0,
                path: "functions.id.result".into(),
                kind: Kind::IntegerRange,
            }];
            let root = Root {
                document: &doc,
                identity: "held/environment.yaml",
            };
            let report = p::admit_root_scalars(root, &findings, Some(Language::Python)).unwrap();
            assert_eq!(
                report.diagnostics().unwrap(),
                [
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "language",
                        &[("reason", text("missing_language"))],
                        "held/environment.yaml",
                        "$"
                    ),
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "lock",
                        &[("reason", text("missing_lock"))],
                        "held/environment.yaml",
                        "$"
                    ),
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "functions.id.result",
                        &[
                            ("reason", text("integer_out_of_range")),
                            (
                                "value",
                                CV::Number((wide).parse().expect("canonical integer"))
                            )
                        ],
                        "held/environment.yaml",
                        "$"
                    )
                ]
            );
        }
        #[test]
        fn scalar_sources_admit_all_captures_before_copying_and_keep_authored_holes() {
            let root_doc = root_document(&[], &[true, true, true]);
            let doc = document(vec![
                N::Integer("9223372036854775808".into()),
                N::Text("value".into()),
                N::Mapping(vec![(1, 0)]),
            ]);
            let findings = [SF {
                node: 0,
                path: "items[0].value".into(),
                kind: Kind::IntegerRange,
            }];
            let root = Root {
                document: &root_doc,
                identity: "held/environment.yaml",
            };
            let sources = [
                ScalarSource {
                    capture: Capture {
                        origin: Origin::Codelist(1),
                        document: &doc,
                        identity: "held/first.yaml",
                    },
                    findings: &findings,
                },
                ScalarSource {
                    capture: Capture {
                        origin: Origin::Codelist(2),
                        document: &doc,
                        identity: "held/second.yaml",
                    },
                    findings: &findings,
                },
            ];
            let result = p::admit_scalar_sources(root, sources.iter().copied())
                .unwrap()
                .diagnostics()
                .unwrap();
            assert_eq!(
                result,
                [
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "items[0].value",
                        &[
                            ("reason", text("integer_out_of_range")),
                            (
                                "value",
                                CV::Number(
                                    ("9223372036854775808").parse().expect("canonical integer")
                                )
                            )
                        ],
                        "held/first.yaml",
                        "codelists[1]"
                    ),
                    expected(
                        C::ProjectEnvironmentInvalid,
                        "items[0].value",
                        &[
                            ("reason", text("integer_out_of_range")),
                            (
                                "value",
                                CV::Number(
                                    ("9223372036854775808").parse().expect("canonical integer")
                                )
                            )
                        ],
                        "held/second.yaml",
                        "codelists[2]"
                    )
                ]
            );
        }

        #[test]
        fn immutable_capture_descriptor_handles_consume_non_clone_input_exactly_once() {
            use std::{cell::Cell, rc::Rc};
            struct Once<'a> {
                capture: Option<Capture<'a>>,
                calls: Rc<Cell<usize>>,
            }
            impl<'a> Iterator for Once<'a> {
                type Item = Capture<'a>;
                fn next(&mut self) -> Option<Self::Item> {
                    self.calls.set(self.calls.get() + 1);
                    self.capture.take()
                }
            }
            let draft = draft();
            let doc = root_document(&[("id", true)], &[]);
            let captured = document(vec![N::Mapping(vec![])]);
            let names = ["id".into()];
            let findings = [Finding::Function {
                function: 0,
                finding: F::InvalidCallable,
            }];
            let calls = Rc::new(Cell::new(0));
            let report = p::admit_admission(
                Admission {
                    root: Root {
                        document: &doc,
                        identity: "held/environment.yaml",
                    },
                    draft: &draft,
                    function_origins: &names,
                    codelist_origins: &[],
                    captures: Once {
                        capture: Some(Capture {
                            origin: Origin::Function("id"),
                            document: &captured,
                            identity: "held/function.yaml",
                        }),
                        calls: Rc::clone(&calls),
                    },
                },
                &findings,
            )
            .unwrap();
            assert_eq!(calls.get(), 2);
            let first = report.diagnostics().unwrap();
            assert_eq!(report.diagnostics().unwrap(), first);
            assert_eq!(calls.get(), 2);
            assert_eq!(
                first,
                [expected(
                    C::ProjectEnvironmentInvalid,
                    "function",
                    &[
                        ("function", text("id")),
                        ("reason", text("invalid_callable"))
                    ],
                    "held/function.yaml",
                    "functions.id"
                )]
            );
        }
        #[test]
        fn two_hundred_inline_functions_keep_all_six_hundred_independent_findings_in_order() {
            let mut draft = draft();
            let template = draft.functions.take().unwrap().remove(0);
            let names = (0..200)
                .map(|index| format!("function_{index}"))
                .collect::<Vec<_>>();
            draft.functions = Some(
                names
                    .iter()
                    .map(|name| {
                        let mut definition = template.clone();
                        definition.name = name.clone();
                        definition.tests[0].covers = vec!["normal".into(), "boundary".into()];
                        definition.tests[0].result = Value::Bool(true);
                        definition.params.clear();
                        definition.tests[0].args.clear();
                        definition
                    })
                    .collect(),
            );
            let doc = root_document(
                &names
                    .iter()
                    .map(|name| (name.as_str(), false))
                    .collect::<Vec<_>>(),
                &[],
            );
            let findings = project_environment::validate(Language::Python, &draft);
            assert_eq!(findings.len(), 600);
            let report = p::admit_admission(
                Admission {
                    root: Root {
                        document: &doc,
                        identity: "held/environment.yaml",
                    },
                    draft: &draft,
                    function_origins: &names,
                    codelist_origins: &[],
                    captures: std::iter::empty(),
                },
                &findings,
            )
            .unwrap();
            let mut want = vec![];
            for name in &names {
                let declaration = format!("functions.{name}");
                want.push(expected(
                    C::ProjectEnvironmentInvalid,
                    &format!("{declaration}.tests[0].result"),
                    &[
                        ("function", text(name)),
                        ("case", text("ordinary")),
                        ("reason", text("invalid_case_result")),
                        ("value", CV::Bool(true)),
                    ],
                    "held/environment.yaml",
                    &declaration,
                ));
                for obligation in ["boundary", "normal"] {
                    want.push(expected(
                        C::ProjectEnvironmentInvalid,
                        &format!("{declaration}.tests"),
                        &[
                            ("function", text(name)),
                            ("reason", text("missing_coverage_obligation")),
                            ("obligation", text(obligation)),
                        ],
                        "held/environment.yaml",
                        &declaration,
                    ));
                }
            }
            assert_eq!(report.diagnostics().unwrap(), want);
        }
    }
    mod project_function {
        use yamaa_core::{
            project_function::{
                validate, Case, Definition, Finding, Function, Language, Parameter,
            },
            value::{ColumnType, Value, ValueType},
        };

        fn case(id: &str, tags: &[&str], args: &[(&str, Value)], result: Value) -> Case {
            Case {
                id: id.into(),
                covers: tags.iter().map(|s| (*s).into()).collect(),
                args: args.iter().map(|(n, v)| ((*n).into(), v.clone())).collect(),
                result,
            }
        }
        fn integer() -> Definition {
            Definition {
                name: "identity".into(),
                function: "project.identity".into(),
                description: "Return the supplied integer.".into(),
                params: vec![Parameter {
                    name: "x".into(),
                    kind: ValueType::Int,
                    required: true,
                    default: None,
                    accepts_missing: false,
                }],
                returns: ColumnType::Int,
                may_return_missing: false,
                comparison_decimals: 4,
                tests: vec![
                    case(
                        "ordinary",
                        &["normal"],
                        &[("x", Value::Int(7))],
                        Value::Int(7),
                    ),
                    case(
                        "lower-bound",
                        &["boundary"],
                        &[("x", Value::Int(i64::MIN))],
                        Value::Int(i64::MIN),
                    ),
                    case(
                        "missing-x",
                        &["short-circuit-missing:x"],
                        &[("x", Value::Missing)],
                        Value::Missing,
                    ),
                ],
            }
        }
        fn nullable_float() -> Definition {
            Definition {
                name: "select".into(),
                function: "program.select".into(),
                description: "Select a nullable float.".into(),
                params: vec![
                    Parameter {
                        name: "choice".into(),
                        kind: ValueType::Bool,
                        required: true,
                        default: None,
                        accepts_missing: true,
                    },
                    Parameter {
                        name: "scale".into(),
                        kind: ValueType::Int,
                        required: false,
                        default: Some(Value::Int(2)),
                        accepts_missing: false,
                    },
                ],
                returns: ColumnType::Float,
                may_return_missing: true,
                comparison_decimals: 4,
                tests: vec![
                    case(
                        "ordinary",
                        &[
                            "normal",
                            "default:scale",
                            "boolean-true:choice",
                            "numeric-comparison",
                        ],
                        &[("choice", Value::Bool(true))],
                        Value::float(2.0),
                    ),
                    case(
                        "boundary",
                        &["boundary", "boolean-false:choice", "nullable-output"],
                        &[("choice", Value::Bool(false)), ("scale", Value::Int(0))],
                        Value::Missing,
                    ),
                    case(
                        "missing-choice",
                        &["accepted-missing:choice"],
                        &[("choice", Value::Missing), ("scale", Value::Int(2))],
                        Value::float(1.0),
                    ),
                    case(
                        "missing-scale",
                        &["short-circuit-missing:scale"],
                        &[("choice", Value::Bool(true)), ("scale", Value::Missing)],
                        Value::Missing,
                    ),
                ],
            }
        }

        #[test]
        fn complete_definitions_are_owned_and_versionless() {
            for def in [integer(), nullable_float()] {
                assert_eq!(validate(Language::Python, &def), vec![]);
                let expected = def.clone();
                let admitted = Function::admit(Language::Python, def).unwrap();
                assert_eq!(admitted.definition(), &expected);
            }
            let mut r = integer();
            r.function = "project::identity".into();
            assert!(Function::admit(Language::R, r).is_ok());
        }

        #[test]
        fn every_missing_obligation_is_reported_independently() {
            let mut def = nullable_float();
            def.tests.clear();
            assert_eq!(
                validate(Language::Python, &def),
                vec![
                    Finding::MissingObligation("accepted-missing:choice".into()),
                    Finding::MissingObligation("boolean-false:choice".into()),
                    Finding::MissingObligation("boolean-true:choice".into()),
                    Finding::MissingObligation("boundary".into()),
                    Finding::MissingObligation("default:scale".into()),
                    Finding::MissingObligation("normal".into()),
                    Finding::MissingObligation("nullable-output".into()),
                    Finding::MissingObligation("numeric-comparison".into()),
                    Finding::MissingObligation("short-circuit-missing:scale".into()),
                ]
            );
        }

        #[test]
        fn static_arguments_check_every_type_even_after_missing_short_circuit() {
            let mut def = integer();
            def.params.push(Parameter {
                name: "y".into(),
                kind: ValueType::Float,
                required: true,
                default: None,
                accepts_missing: false,
            });
            def.tests = vec![case(
                "wrong",
                &["normal", "boundary"],
                &[
                    ("x", Value::Missing),
                    ("y", Value::Int(1)),
                    ("other", Value::Bool(true)),
                    ("other", Value::Int(2)),
                ],
                Value::Missing,
            )];
            let f = validate(Language::Python, &def);
            assert_eq!(
                &f[..4],
                &[
                    Finding::ArgumentType {
                        case: 0,
                        argument: 1
                    },
                    Finding::UnknownArgument {
                        case: 0,
                        argument: 2
                    },
                    Finding::DuplicateArgument {
                        case: 0,
                        argument: 3
                    },
                    Finding::UnknownArgument {
                        case: 0,
                        argument: 3
                    },
                ]
            );
            assert!(f.contains(&Finding::MissingObligation("normal".into())));
            assert!(f.contains(&Finding::MissingObligation("boundary".into())));
        }

        #[test]
        fn missing_defaults_are_distinct_from_absent_defaults_and_never_widen() {
            for (default, accepting, expected) in [
                (None, false, Some(Finding::MissingDefault { parameter: 0 })),
                (
                    Some(Value::Missing),
                    false,
                    Some(Finding::InvalidDefault { parameter: 0 }),
                ),
                (
                    Some(Value::float(2.0)),
                    false,
                    Some(Finding::InvalidDefault { parameter: 0 }),
                ),
                (
                    Some(Value::Bool(true)),
                    false,
                    Some(Finding::InvalidDefault { parameter: 0 }),
                ),
                (Some(Value::Int(i64::MAX)), false, None),
                (Some(Value::Missing), true, None),
            ] {
                let mut def = integer();
                def.params[0].required = false;
                def.params[0].default = default;
                def.params[0].accepts_missing = accepting;
                let f = validate(Language::Python, &def);
                assert_eq!(
                    f.iter()
                        .find(|x| matches!(
                            x,
                            Finding::MissingDefault { .. } | Finding::InvalidDefault { .. }
                        ))
                        .cloned(),
                    expected
                );
            }
            let mut def = integer();
            def.params[0].default = Some(Value::Int(9));
            assert_eq!(
                validate(Language::Python, &def)[0],
                Finding::RequiredDefault { parameter: 0 }
            );
        }

        #[test]
        fn tagged_evidence_requires_the_named_behavior_and_an_invoked_nullable_result() {
            let mut def = nullable_float();
            def.tests = vec![case(
                "pretend",
                &[
                    "default:scale",
                    "accepted-missing:choice",
                    "nullable-output",
                    "numeric-comparison",
                    "boolean-false:choice",
                ],
                &[("choice", Value::Missing), ("scale", Value::Missing)],
                Value::Missing,
            )];
            assert_eq!(
                &validate(Language::Python, &def)[..5],
                &[
                    Finding::UnsupportedEvidence { case: 0, tag: 0 },
                    Finding::UnsupportedEvidence { case: 0, tag: 1 },
                    Finding::UnsupportedEvidence { case: 0, tag: 2 },
                    Finding::UnsupportedEvidence { case: 0, tag: 3 },
                    Finding::UnsupportedEvidence { case: 0, tag: 4 },
                ]
            );
            let mut def = integer();
            def.tests[2].result = Value::Int(0);
            let f = validate(Language::Python, &def);
            assert!(f.contains(&Finding::InvalidResult { case: 2 }));
            assert!(f.contains(&Finding::UnsupportedEvidence { case: 2, tag: 0 }));
            assert!(f.contains(&Finding::MissingObligation(
                "short-circuit-missing:x".into()
            )));
        }

        #[test]
        fn duplicate_ids_tags_and_unknown_obligations_do_not_hide_findings() {
            let mut def = integer();
            def.tests[0].covers = vec![
                "normal".into(),
                "normal".into(),
                "default:ghost".into(),
                "script:x".into(),
            ];
            def.tests[1].id = "ordinary".into();
            def.tests[2].id = "Bad_ID".into();
            def.tests[2].covers.clear();
            let f = validate(Language::Python, &def);
            assert_eq!(
                &f[..6],
                &[
                    Finding::DuplicateCoverage { case: 0, tag: 1 },
                    Finding::UnsupportedEvidence { case: 0, tag: 2 },
                    Finding::InvalidCoverage { case: 0, tag: 3 },
                    Finding::DuplicateCaseId { case: 1 },
                    Finding::InvalidCaseId { case: 2 },
                    Finding::EmptyCoverage { case: 2 },
                ]
            );
            assert!(f.contains(&Finding::MissingObligation("boundary".into())));
        }

        #[test]
        fn rejected_callables_and_host_parameter_names_are_checked_without_host_code() {
            for call in [
                "identity",
                "project.py/identity",
                "project.identity()",
                "project..identity",
                "project.class",
                "project::identity",
                "project.identity\n",
            ] {
                let mut def = integer();
                def.function = call.into();
                assert!(
                    validate(Language::Python, &def).contains(&Finding::InvalidCallable),
                    "{call}"
                );
            }
            for call in [
                "identity",
                "project:::identity",
                "project::function",
                "project::`identity`",
                "project.identity",
                "project::..1",
            ] {
                let mut def = integer();
                def.function = call.into();
                assert!(
                    validate(Language::R, &def).contains(&Finding::InvalidCallable),
                    "{call}"
                );
            }
            for (language, name) in [
                (Language::Python, "class"),
                (Language::Python, "9x"),
                (Language::R, "_x"),
                (Language::R, "NA"),
                (Language::R, "function"),
            ] {
                let mut def = integer();
                def.params[0].name = name.into();
                assert!(validate(language, &def)
                    .contains(&Finding::InvalidParameterName { parameter: 0 }));
            }
        }

        #[test]
        fn all_definition_and_result_faults_have_independent_diagnostics() {
            let mut def = integer();
            def.name.clear();
            def.function.clear();
            def.description.clear();
            def.comparison_decimals = -1;
            def.params.push(def.params[0].clone());
            def.tests[0].args.clear();
            def.tests[0].result = Value::Bool(true);
            let f = validate(Language::Python, &def);
            assert_eq!(
                &f[..8],
                &[
                    Finding::InvalidName,
                    Finding::InvalidCallable,
                    Finding::EmptyDescription,
                    Finding::NegativeComparisonDecimals,
                    Finding::DuplicateParameter { parameter: 1 },
                    Finding::MissingRequiredArgument {
                        case: 0,
                        parameter: 0
                    },
                    Finding::MissingRequiredArgument {
                        case: 0,
                        parameter: 1
                    },
                    Finding::InvalidResult { case: 0 },
                ]
            );
            for value in [
                Value::Missing,
                Value::Bool(true),
                Value::float(7.0),
                Value::Str("7".into()),
            ] {
                let mut def = integer();
                def.tests[0].result = value;
                assert!(
                    validate(Language::Python, &def).contains(&Finding::InvalidResult { case: 0 })
                );
            }
        }

        #[test]
        fn static_call_binding_checks_all_names_and_exact_types_without_missing_shortcuts() {
            use yamaa_core::project_function::{CallArgument, CallFinding};
            let function = Function::admit(Language::Python, nullable_float()).unwrap();
            let arguments = vec![
                CallArgument {
                    name: "choice".into(),
                    kind: None,
                },
                CallArgument {
                    name: "scale".into(),
                    kind: Some(ValueType::Float),
                },
                CallArgument {
                    name: "extra".into(),
                    kind: Some(ValueType::Int),
                },
                CallArgument {
                    name: "extra".into(),
                    kind: Some(ValueType::Str),
                },
            ];
            assert_eq!(
                function.bind_call(&arguments),
                vec![
                    CallFinding::ArgumentType {
                        argument: 1,
                        expected: ValueType::Int,
                        actual: ValueType::Float
                    },
                    CallFinding::UnknownArgument { argument: 2 },
                    CallFinding::DuplicateArgument { argument: 3 },
                    CallFinding::UnknownArgument { argument: 3 },
                ]
            );
            assert_eq!(
                function.bind_call(&[]),
                vec![CallFinding::MissingRequiredArgument { parameter: 0 }]
            );
            assert!(function
                .bind_call(&[CallArgument {
                    name: "choice".into(),
                    kind: Some(ValueType::Bool)
                }])
                .is_empty());
            assert!(function
                .bind_call(&[CallArgument {
                    name: "choice".into(),
                    kind: None
                }])
                .is_empty());
        }
    }
}
