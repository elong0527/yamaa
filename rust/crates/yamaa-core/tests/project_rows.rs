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
    mod project_environment {
        use yamaa_core::{
            project_environment::{
                validate, Draft, Environment, ExecutionEnvironment, Finding, LockKind,
                LockReference,
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
        fn a_project_without_an_environment_for_the_host_needs_no_code_or_lock() {
            for host in [Language::Python, Language::R] {
                let admitted = Environment::admit(host, draft()).unwrap();
                assert_eq!(admitted.draft(), &draft());
                // The supplied project names no environment file for this host.
                let execution = admitted.into_execution();
                assert_eq!(execution.language(), None);
                assert_eq!(execution.undeclared_language(), Some(host));
                // A build without any project has no environment to name.
                let plain = ExecutionEnvironment::without_project(host);
                assert_eq!(plain.host(), host);
                assert_eq!(plain.language(), None);
                assert_eq!(plain.undeclared_language(), None);
                assert!(!plain.functions_present() && plain.lock().is_none());
                assert!(plain.catalogue().sources().is_empty());
            }
        }
        #[test]
        fn present_functions_even_empty_need_no_lock() {
            let mut d = draft();
            d.language = Some(Language::Python);
            d.functions = Some(vec![]);
            assert!(validate(Language::Python, &d).is_empty());
            d.functions = Some(vec![definition()]);
            let execution = Environment::admit(Language::Python, d)
                .unwrap()
                .into_execution();
            assert_eq!(execution.undeclared_language(), None);
            assert!(execution.lock().is_none());
        }
        #[test]
        fn captured_lock_kind_must_match_the_linked_environment_language() {
            for (language, kind, actual) in [
                (Language::Python, LockKind::Uv, LockKind::Renv),
                (Language::R, LockKind::Renv, LockKind::Uv),
            ] {
                let mut d = draft();
                d.language = Some(language);
                d.lock = Some(LockReference {
                    written: "custom-name.lock".into(),
                    kind: actual,
                });
                assert_eq!(
                    validate(language, &d),
                    vec![Finding::LockKindMismatch { language, actual }]
                );
                d.lock.as_mut().unwrap().kind = kind;
                assert!(validate(language, &d).is_empty());
            }
        }
        #[test]
        fn all_function_faults_remain_owned_without_any_effect_port() {
            let mut d = draft();
            d.language = Some(Language::Python);
            d.functions = Some(vec![definition(), definition()]);
            assert_eq!(
                validate(Language::Python, &d),
                vec![Finding::DuplicateFunction { function: 1 }]
            );
            d.lock = Some(LockReference {
                written: "../uv.lock".into(),
                kind: LockKind::Uv,
            });
            d.functions.as_mut().unwrap().pop();
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
            assert_eq!(empty.undeclared_language(), Some(Language::R));
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
            assert_eq!(execution.undeclared_language(), None);
            assert!(execution.catalogue().sources().is_empty());
        }

        #[test]
        fn rejected_admission_retains_exact_draft_ownership_and_all_findings() {
            let mut input = draft();
            input.language = Some(Language::Python);
            input.lock = Some(LockReference {
                written: "renv.lock".into(),
                kind: LockKind::Renv,
            });
            input.functions = Some(vec![definition(), definition()]);
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
                    Finding::LockKindMismatch {
                        language: Language::Python,
                        actual: LockKind::Renv
                    },
                    Finding::DuplicateFunction { function: 1 }
                ]
            );
        }
    }
    mod project_environment_diagnostics {
        use crate::support::project_environment as support;
        use yamaa_core::json::Value as JsonValue;
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
                let diagnostic = failure.diagnostic("/study/project.yaml");
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
                        Some("FUNC-1")
                    )
                );
                assert_eq!(diagnostic.spec_paths, ["environment"]);
                assert_eq!(diagnostic.context["cause"], JsonValue::String(cause.into()));
                assert_eq!(
                    diagnostic.context["path"],
                    JsonValue::String("/study/project.yaml".into())
                );
            }
        }
        #[test]
        fn every_lock_and_terminology_cause_has_independent_normative_paths_and_mapping() {
            assert_eq!(support::reached().len(), 8);
            let draft = support::draft();
            let projected = diagnostics(
                &draft,
                &project_environment::validate(Language::Python, &draft),
            )
            .unwrap();
            // The linked language has no finding of its own; only codelists fail.
            assert_eq!(projected.len(), 16);
            assert!(projected
                .iter()
                .all(|diagnostic| diagnostic.spec_paths[0].starts_with("codelists[0].codelists[")));
            let value = projected
                .iter()
                .find(|diagnostic| {
                    diagnostic.definition().condition == "codelist_shape_invalid"
                        && diagnostic.spec_paths == ["codelists[0].codelists[0].items[0].value"]
                })
                .unwrap();
            assert_eq!(value.context["value"], JsonValue::Bool(true));
            let partial_fields = projected
                .iter()
                .filter(|diagnostic| {
                    diagnostic.definition().condition == "codelist_partial_item_field"
                        && diagnostic.spec_paths == ["codelists[0].codelists[0]"]
                })
                .collect::<Vec<_>>();
            assert_eq!(partial_fields.len(), 2);
            assert_eq!(
                partial_fields[0].context["field"],
                JsonValue::String("decode".into())
            );
            assert_eq!(
                partial_fields[1].context["field"],
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
                    ("project_environment_invalid", Some("FUNC-2"))
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
        fn retained_admission_reports_the_captured_lock_syntax_against_the_linked_language() {
            use yamaa_core::project_environment::Environment;
            // Empty present functions and an absent environment need no finding.
            let mut d = draft();
            d.lock = None;
            d.functions = Some(vec![]);
            assert!(Environment::admit_retained(Language::Python, d).is_ok());
            let mut d = draft();
            d.language = None;
            d.lock = None;
            d.functions = None;
            assert!(Environment::admit_retained(Language::Python, d).is_ok());
            let mut d = draft();
            d.functions = None;
            d.language = Some(Language::R);
            let rejection = Environment::admit_retained(Language::R, d).unwrap_err();
            let projected = diagnostics(&rejection.draft, &rejection.error).unwrap();
            assert_eq!(projected.len(), 1);
            assert_eq!(projected[0].spec_paths, ["lock"]);
            assert_eq!(
                projected[0].definition().condition,
                "project_environment_invalid"
            );
            assert_eq!(projected[0].context.len(), 3);
            assert_eq!(
                projected[0].context["actual_lock_kind"],
                JsonValue::String("uv".into())
            );
            assert_eq!(
                projected[0].context["language"],
                JsonValue::String("r".into())
            );
            assert_eq!(
                projected[0].context["reason"],
                JsonValue::String("lock_kind_mismatch".into())
            );
        }
    }
    mod project_environment_projection {
        use crate::support::project_environment as support;
        use std::collections::BTreeMap;
        use yamaa_core::json::Value as CV;
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
        /// The caller-supplied project file and the environment file it links.
        const PROJECT: &str = "held/project.yaml";
        const ENVIRONMENT: &str = "held/environment.yaml";
        fn document(nodes: Vec<N>) -> Document {
            let root = nodes.len() - 1;
            Document::new(nodes, root).unwrap()
        }
        /// A project root that links a Python environment file and declares
        /// inline or external codelists.
        fn project_document(codelists: &[bool]) -> Document {
            let mut nodes = vec![
                N::Text("environment.yaml".into()),
                N::Text("python".into()),
                N::Mapping(vec![(1, 0)]),
                N::Text("environments".into()),
            ];
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
            nodes.push(N::Mapping(vec![(3, 2), (list_key, lists)]));
            document(nodes)
        }
        /// A linked environment file that declares inline or external functions.
        fn environment_document(functions: &[(&str, bool)]) -> Document {
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
            nodes.push(N::Mapping(vec![(function_key, functions)]));
            document(nodes)
        }
        fn project(document: &Document) -> Root<'_> {
            Root {
                document,
                identity: PROJECT,
            }
        }
        fn environment(document: &Document) -> Option<Root<'_>> {
            Some(Root {
                document,
                identity: ENVIRONMENT,
            })
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
            let project_doc = project_document(&[]);
            let environment_doc = environment_document(
                &names
                    .iter()
                    .map(|name| (name.as_str(), true))
                    .collect::<Vec<_>>(),
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
            let root = project(&project_doc);
            let linked = environment(&environment_doc);
            let report = p::admit_admission(
                Admission {
                    root,
                    environment: linked,
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
                linked,
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
                ("entry".into(), text(PROJECT)),
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
            }
        }
        #[test]
        fn independent_complete_sixteen_record_project_report_preserves_every_field_and_order() {
            assert_eq!(support::reached().len(), 8);
            let draft = support::draft();
            let project_doc = project_document(&[false]);
            let environment_doc = environment_document(&[]);
            let findings = project_environment::validate(Language::Python, &draft);
            let origins = [0];
            let result = p::admit_admission(
                Admission {
                    root: project(&project_doc),
                    environment: environment(&environment_doc),
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
            // Inline codelists are declared by the project itself.
            let mut want = vec![];
            for list in 0..2 {
                let base = format!("codelists[0].codelists[{list}]");
                if list == 1 {
                    want.push(expected(
                        C::CodelistDuplicateIdentifier,
                        &format!("{base}.id"),
                        &[("codelist", text("SEX")), ("field", text("id"))],
                        PROJECT,
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
                        PROJECT,
                        "codelists[0]",
                    ));
                }
                want.push(expected(
                    C::CodelistInvalidShape,
                    &base,
                    &[("codelist", text("SEX"))],
                    PROJECT,
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistInvalidValue,
                    &format!("{base}.items[0].value"),
                    &[("codelist", text("SEX")), ("value", CV::Bool(true))],
                    PROJECT,
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistExtensionNotAdmitted,
                    &format!("{base}.items[0].extended"),
                    &[("codelist", text("SEX"))],
                    PROJECT,
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistInvalidValue,
                    &format!("{base}.items[1].value"),
                    &[("codelist", text("SEX")), ("value", CV::Bool(true))],
                    PROJECT,
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistDuplicateValue,
                    &format!("{base}.items[1].value"),
                    &[("codelist", text("SEX")), ("value", CV::Bool(true))],
                    PROJECT,
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistPartialItemField,
                    &base,
                    &[("codelist", text("SEX")), ("field", text("decode"))],
                    PROJECT,
                    "codelists[0]",
                ));
                want.push(expected(
                    C::CodelistPartialItemField,
                    &base,
                    &[("codelist", text("SEX")), ("field", text("rank"))],
                    PROJECT,
                    "codelists[0]",
                ));
            }
            assert_eq!(result, want);
            assert_eq!(result.len(), 16);
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
                let project_doc = project_document(&[]);
                let environment_doc = environment_document(&[("id", external)]);
                let captured = document(vec![N::Mapping(vec![])]);
                let names = ["id".into()];
                let captures = if external {
                    vec![Capture {
                        origin: Origin::Function("id"),
                        document: &captured,
                        identity: "held/function.yaml",
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
                        root: project(&project_doc),
                        environment: environment(&environment_doc),
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
                        // The environment file authors every function name; an
                        // external function file holds the remaining fields.
                        let (path, source) = if suffix.is_empty() {
                            ("functions.id".into(), ENVIRONMENT)
                        } else if external {
                            ((*suffix).into(), "held/function.yaml")
                        } else {
                            (format!("functions.id.{suffix}"), ENVIRONMENT)
                        };
                        let mut fields = vec![("function", text("id")), ("reason", text(reason))];
                        fields.extend(context.iter().cloned());
                        expected(
                            C::ProjectEnvironmentInvalid,
                            &path,
                            &fields,
                            source,
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
                            Some("FUNC-2")
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
            let project_doc = project_document(&[]);
            let environment_doc = environment_document(&[(&name, false)]);
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
                    root: project(&project_doc),
                    environment: environment(&environment_doc),
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
                ENVIRONMENT,
                &format!("functions.{name}"),
            );
            assert_eq!(actual.len(), findings.len());
            assert!(actual.iter().all(|diagnostic| diagnostic == &expected));
        }
        #[test]
        fn malformed_origin_relationships_and_wrong_held_nodes_refuse_explicitly() {
            let draft = draft();
            let project_doc = project_document(&[]);
            let inline = environment_document(&[("id", false)]);
            let external = environment_document(&[("id", true)]);
            let captured = document(vec![N::Mapping(vec![])]);
            let names = ["id".into()];
            let wrong_names = ["other".into()];
            let findings = [Finding::LockKindMismatch {
                language: Language::Python,
                actual: LockKind::Renv,
            }];
            let capture = Capture {
                origin: Origin::Function("id"),
                document: &captured,
                identity: "held/function.yaml",
            };
            // The linked environment file is held beside the captures.
            let linked = Capture {
                origin: Origin::Environment("python"),
                document: &inline,
                identity: ENVIRONMENT,
            };
            for (linked_environment, origins, captures) in [
                (Some(&inline), &wrong_names, vec![]),
                (Some(&inline), &names, vec![capture]),
                (Some(&external), &names, vec![]),
                (Some(&external), &names, vec![capture, capture]),
                (Some(&inline), &names, vec![linked]),
                // Functions are declared only by a linked environment file.
                (None, &names, vec![]),
            ] {
                assert!(matches!(
                    p::admit_admission(
                        Admission {
                            root: project(&project_doc),
                            environment: linked_environment.and_then(environment),
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
            // The lock is declared by the linked environment file, so a lock
            // finding cannot be reported without it.
            let mut lock_only = draft.clone();
            lock_only.functions = None;
            assert!(matches!(
                p::admit_admission(
                    Admission {
                        root: project(&project_doc),
                        environment: None,
                        draft: &lock_only,
                        function_origins: &[],
                        codelist_origins: &[],
                        captures: std::iter::empty()
                    },
                    &findings
                ),
                Err(Error::InvalidOrigin)
            ));
            let scalar = [SF {
                path: "result".into(),
                node: 100,
                kind: Kind::IntegerRange,
            }];
            assert!(matches!(
                p::admit_root_scalars(project(&project_doc), &scalar),
                Err(Error::Core(
                    project_environment_diagnostics::Error::InvalidIndex
                ))
            ));
            let scalar = [SF {
                path: "result".into(),
                node: project_doc.root(),
                kind: Kind::IntegerRange,
            }];
            assert!(matches!(
                p::admit_root_scalars(project(&project_doc), &scalar),
                Err(Error::Core(
                    project_environment_diagnostics::Error::UnavailableShape
                ))
            ));
        }
        #[test]
        fn lock_findings_name_the_linked_environment_file_that_writes_the_lock() {
            let mut draft = draft();
            draft.functions = None;
            draft.lock.as_mut().unwrap().kind = LockKind::Renv;
            let findings = project_environment::validate(Language::Python, &draft);
            assert_eq!(
                findings,
                [Finding::LockKindMismatch {
                    language: Language::Python,
                    actual: LockKind::Renv
                }]
            );
            let project_doc = project_document(&[]);
            let environment_doc = environment_document(&[]);
            let result = p::admit_admission(
                Admission {
                    root: project(&project_doc),
                    environment: environment(&environment_doc),
                    draft: &draft,
                    function_origins: &[],
                    codelist_origins: &[],
                    captures: std::iter::empty(),
                },
                &findings,
            )
            .unwrap()
            .diagnostics()
            .unwrap();
            assert_eq!(
                result,
                [expected(
                    C::ProjectEnvironmentInvalid,
                    "lock",
                    &[
                        ("reason", text("lock_kind_mismatch")),
                        ("language", text("python")),
                        ("actual_lock_kind", text("renv")),
                    ],
                    ENVIRONMENT,
                    "lock",
                )]
            );
        }
        #[test]
        fn root_wide_scalars_are_one_admitted_complete_report() {
            let wide = "123456789012345678901234567890";
            let doc = document(vec![
                N::Integer(wide.into()),
                N::Text("value".into()),
                N::Mapping(vec![(1, 0)]),
                N::Sequence(vec![2]),
                N::Text("items".into()),
                N::Mapping(vec![(4, 3)]),
                N::Sequence(vec![5]),
                N::Text("codelists".into()),
                N::Mapping(vec![(7, 6)]),
                N::Sequence(vec![8]),
                N::Text("codelists".into()),
                N::Mapping(vec![(10, 9)]),
            ]);
            let findings = [SF {
                node: 0,
                path: "codelists[0].codelists[0].items[0].value".into(),
                kind: Kind::IntegerRange,
            }];
            let report = p::admit_root_scalars(project(&doc), &findings).unwrap();
            assert_eq!(
                report.diagnostics().unwrap(),
                [expected(
                    C::ProjectEnvironmentInvalid,
                    "codelists[0].codelists[0].items[0].value",
                    &[
                        ("reason", text("integer_out_of_range")),
                        (
                            "value",
                            CV::Number((wide).parse().expect("canonical integer"))
                        )
                    ],
                    PROJECT,
                    "$"
                )]
            );
        }
        #[test]
        fn environment_file_scalar_failures_report_against_the_project_link() {
            let project_doc = project_document(&[]);
            let doc = document(vec![
                N::Integer("9223372036854775808".into()),
                N::Text("result".into()),
                N::Mapping(vec![(1, 0)]),
                N::Text("inline".into()),
                N::Mapping(vec![(3, 2)]),
                N::Text("functions".into()),
                N::Mapping(vec![(5, 4)]),
            ]);
            let findings = [SF {
                node: 0,
                path: "functions.inline.result".into(),
                kind: Kind::IntegerRange,
            }];
            let value = (
                "value",
                CV::Number(("9223372036854775808").parse().expect("canonical integer")),
            );
            let source = |key| ScalarSource {
                capture: Capture {
                    origin: Origin::Environment(key),
                    document: &doc,
                    identity: ENVIRONMENT,
                },
                findings: &findings,
            };
            // A failed environment file is never also the linked environment.
            let result = p::admit_scalar_sources(
                project(&project_doc),
                None,
                [source("python")].into_iter(),
            )
            .unwrap()
            .diagnostics()
            .unwrap();
            assert_eq!(
                result,
                [expected(
                    C::ProjectEnvironmentInvalid,
                    "functions.inline.result",
                    &[("reason", text("integer_out_of_range")), value.clone()],
                    ENVIRONMENT,
                    "environments.python"
                )]
            );
            // Only an `environments` key the project writes names that file.
            assert!(matches!(
                p::admit_scalar_sources(project(&project_doc), None, [source("r")].into_iter()),
                Err(Error::InvalidOrigin)
            ));
            // A failed function file is declared by the linked environment file.
            let environment_doc = environment_document(&[("first", true)]);
            let function = document(vec![
                N::Integer("9223372036854775808".into()),
                N::Text("result".into()),
                N::Mapping(vec![(1, 0)]),
            ]);
            let function_findings = [SF {
                node: 0,
                path: "tests[0].result".into(),
                kind: Kind::IntegerRange,
            }];
            let function_source = ScalarSource {
                capture: Capture {
                    origin: Origin::Function("first"),
                    document: &function,
                    identity: "held/first.yaml",
                },
                findings: &function_findings,
            };
            assert!(matches!(
                p::admit_scalar_sources(project(&project_doc), None, [function_source].into_iter()),
                Err(Error::InvalidOrigin)
            ));
            let result = p::admit_scalar_sources(
                project(&project_doc),
                environment(&environment_doc),
                [function_source].into_iter(),
            )
            .unwrap()
            .diagnostics()
            .unwrap();
            assert_eq!(
                result,
                [expected(
                    C::ProjectEnvironmentInvalid,
                    "tests[0].result",
                    &[("reason", text("integer_out_of_range")), value],
                    "held/first.yaml",
                    "functions.first"
                )]
            );
        }
        #[test]
        fn scalar_sources_admit_all_captures_before_copying_and_keep_authored_holes() {
            let root_doc = project_document(&[true, true, true]);
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
            let root = project(&root_doc);
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
            // Codelist files are declared by the project, not the environment.
            let result = p::admit_scalar_sources(root, None, sources.iter().copied())
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
            let project_doc = project_document(&[]);
            let environment_doc = environment_document(&[("id", true)]);
            let captured = document(vec![N::Mapping(vec![])]);
            let names = ["id".into()];
            let findings = [Finding::Function {
                function: 0,
                finding: F::InvalidCallable,
            }];
            let calls = Rc::new(Cell::new(0));
            let report = p::admit_admission(
                Admission {
                    root: project(&project_doc),
                    environment: environment(&environment_doc),
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
            let project_doc = project_document(&[]);
            let environment_doc = environment_document(
                &names
                    .iter()
                    .map(|name| (name.as_str(), false))
                    .collect::<Vec<_>>(),
            );
            let findings = project_environment::validate(Language::Python, &draft);
            assert_eq!(findings.len(), 600);
            let report = p::admit_admission(
                Admission {
                    root: project(&project_doc),
                    environment: environment(&environment_doc),
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
                    ENVIRONMENT,
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
                        ENVIRONMENT,
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
