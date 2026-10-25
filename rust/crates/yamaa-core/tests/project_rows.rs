//! Project contracts: project compiled rows, project environment, project
//! environment diagnostics, project environment projection and project
//! function.
mod project {
    mod project_compiled_rows {
        //! Execute compiler-selected row/default calls without a host-authored plan.
        use crate::support::project_call_compiler as support;
        use std::cell::RefCell;
        use support::{call, environment, specification_with_rows, Tree};
        use yamaa_core::function_invocation::{Argument, FunctionPort, HostError};
        use yamaa_core::{
            function_signature::{InvocationPlan, ProjectInvocationPlan},
            schema::DocumentNode as N,
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        /// An integer written as a YAML scalar, which a 1.0 expression field
        /// reads as a literal (D18).
        fn literal(value: i64) -> Tree {
            Tree::Scalar(N::Integer(value.to_string()))
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
            // Seam: a prepared plan executed through a scripted function port
            // and an observing table, whose output cells and call log are no
            // issue a reader returns (core outcome and ports).
            let spec = specification_with_rows(
                vec![
                    ("A", "int", call("id", vec![("x", Tree::text("B"))])),
                    ("B", "int", call("id", vec![("x", Tree::text("C"))])),
                    ("C", "int", literal(7)),
                    ("D", "int", literal(0)),
                ],
                vec![
                    template("first", Tree::text("SRC.ID"), false),
                    template("second", literal(2), true),
                ],
            );
            let prepared =
                crate::support::specification::prepare_in(&spec, &environment(&["unused", "id"]))
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
                    .execute(&table, &[], &mut bindings, None, &|| false)
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
        use crate::support::roots::{Case, Definition};
        use yamaa_core::{
            ast,
            project_environment::{
                validate, Draft, Environment, ExecutionEnvironment, Finding, LockKind,
                LockReference,
            },
            project_function::Language,
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
        fn definition() -> ast::Function {
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
            .root(Language::Python)
        }
        #[test]
        fn a_project_without_an_environment_for_the_host_needs_no_code_or_lock() {
            // Seam: the admitted and execution environments' accessors; that
            // such a project has no finding is
            // `environment.json::a_project_without_an_environment_for_the_host_needs_no_finding`.
            for host in [Language::Python, Language::R] {
                let admitted = Environment::admit(Some(host), draft()).unwrap();
                assert_eq!(admitted.draft(), &draft());
                let execution = admitted.into_execution();
                assert_eq!(execution.language(), None);
                assert!(!execution.functions_present() && execution.lock().is_none());
                // A build without any project has no environment to name.
                let plain = ExecutionEnvironment::without_project(Some(host));
                assert_eq!(plain.host(), Some(host));
                assert_eq!(plain.language(), None);
                assert!(!plain.functions_present() && plain.lock().is_none());
                assert!(plain.catalogue().sources().is_empty());
            }
            // An engine that claims no host language reads no lock (#3030
            // section 2.10).
            let neutral = Environment::admit(None, draft()).unwrap().into_execution();
            assert_eq!(neutral.host(), None);
            assert_eq!(neutral.language(), None);
            assert!(neutral.lock().is_none());
        }
        #[test]
        fn present_functions_even_empty_need_no_lock() {
            // Seam: the execution environment's `functions_present` and `lock`
            // accessors; that empty functions need no finding is
            // `environment.json::present_empty_functions_without_a_lock_need_no_finding`.
            let mut d = draft();
            d.language = Some(Language::Python);
            d.functions = Some(vec![definition()]);
            let execution = Environment::admit(Some(Language::Python), d)
                .unwrap()
                .into_execution();
            assert!(execution.functions_present());
            assert!(execution.lock().is_none());
        }
        #[test]
        fn captured_lock_kind_must_match_the_host_language() {
            // Seam: `validate`'s internal `Finding` model, the lock kind it
            // captured before a projection names the lock's path.
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
                    validate(&d),
                    vec![Finding::LockKindMismatch { language, actual }]
                );
                d.lock.as_mut().unwrap().kind = kind;
                assert!(validate(&d).is_empty());
            }
        }
        #[test]
        fn all_function_faults_remain_owned_without_any_effect_port() {
            // Seam: `validate`'s internal `Finding` model and the draft an
            // admitted environment keeps.
            let mut d = draft();
            d.language = Some(Language::Python);
            d.functions = Some(vec![definition(), definition()]);
            assert_eq!(
                validate(&d),
                vec![Finding::DuplicateFunction { function: 1 }]
            );
            d.lock = Some(LockReference {
                written: "../uv.lock".into(),
                kind: LockKind::Uv,
            });
            d.functions.as_mut().unwrap().pop();
            let expected = d.clone();
            let admitted = Environment::admit(Some(Language::Python), d).unwrap();
            assert_eq!(admitted.draft(), &expected);
        }

        #[test]
        fn execution_conversion_moves_exact_definitions_and_preserves_absent_or_empty_functions() {
            // Seam: the execution environment's accessors and the ownership
            // pointers its conversion moves.
            let empty = Environment::admit(Some(Language::R), draft())
                .unwrap()
                .into_execution();
            assert_eq!(empty.host(), Some(Language::R));
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
            let empty = Environment::admit(Some(Language::Python), d.clone())
                .unwrap()
                .into_execution();
            assert!(empty.functions_present());
            assert!(empty.functions().is_empty());
            d.functions = Some(vec![definition()]);
            let expected = d.clone();
            let original_name = d.functions.as_ref().unwrap()[0].name.name.as_ptr();
            let original_cases = d.functions.as_ref().unwrap()[0].tests.as_ptr();
            let execution = Environment::admit(Some(Language::Python), d)
                .unwrap()
                .into_execution();
            assert_eq!(execution.language(), Some(Language::Python));
            assert_eq!(execution.lock(), expected.lock.as_ref());
            assert_eq!(
                execution.functions()[0].definition(),
                &expected.functions.as_ref().unwrap()[0]
            );
            assert_eq!(
                execution.functions()[0].definition().name.name.as_ptr(),
                original_name
            );
            assert_eq!(
                execution.functions()[0].definition().tests.as_ptr(),
                original_cases
            );
            assert_eq!(execution.functions()[0].language(), Some(Language::Python));
            assert!(execution.catalogue().sources().is_empty());
        }

        #[test]
        fn rejected_admission_retains_exact_draft_ownership_and_all_findings() {
            // Seam: the ownership pointers of the draft a rejection retains,
            // and `validate`'s internal `Finding` model.
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
            let rejected = Environment::admit_retained(Some(Language::Python), input).unwrap_err();
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
        use crate::support::roots::{Case, Definition, Parameter};
        use yamaa_core::json::Value as JsonValue;
        use yamaa_core::{
            project_environment::{Draft, Finding, LockKind, LockReference},
            project_environment_diagnostics::{diagnostics, Error},
            project_function::{Finding as F, Language},
            value::{ColumnType, Value, ValueType},
        };
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
                }
                .root(Language::Python)]),
                codelists: vec![],
            }
        }
        #[test]
        fn repeated_findings_keep_complete_context_without_a_copy_quota() {
            // Seam: capacity, 600 copies of one finding on a generated
            // 16,384-character name.
            use yamaa_core::diagnostic::{ConditionCode, Diagnostic};
            let mut draft = draft();
            let name = "f".repeat(16_384);
            draft.functions.as_mut().unwrap()[0].name.name = name.clone();
            let findings = vec![
                Finding::Function {
                    function: 0,
                    finding: F::MissingImplementation,
                };
                600
            ];
            let actual = diagnostics(&draft, &findings).unwrap();
            let expected = Diagnostic {
                code: ConditionCode::FunctionFileInvalid,
                spec_paths: vec!["implementations".into()],
                context: [
                    ("function".into(), JsonValue::String(name)),
                    (
                        "reason".into(),
                        JsonValue::String("missing_implementation".into()),
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
            // Seam: the internal `Error::InvalidIndex` refusal, which reports
            // no issue.
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
    }
    mod project_environment_projection {
        use crate::support::roots::{meta, text as written, Case, Definition, Parameter};
        use yamaa_core::json::Value as CV;
        use yamaa_core::{
            ast::{self, FunctionDeclaration},
            diagnostic::{ConditionCode as C, Diagnostic},
            project_environment::{self, Draft, Finding, LockKind, LockReference},
            project_environment_diagnostics::{
                self,
                projection::{self as p, Admission, Capture, Error, Origin, ScalarSource},
            },
            project_function::{Finding as F, Language},
            project_function_document::{Finding as SF, Kind},
            value::{ColumnType, Value, ValueType},
        };
        /// The caller-supplied project file.
        const PROJECT: &str = "held/project.yaml";
        /// A project root that holds the Python lock, declares no codelist
        /// and, when `functions` is given, lists that many function files
        /// (D8).
        fn project_root(functions: Option<usize>) -> ast::Project {
            ast::Project {
                header: ast::Header {
                    version: written("1.0"),
                    meta: meta(),
                },
                functions: functions.map(|count| {
                    (0..count)
                        .map(|index| FunctionDeclaration {
                            file: written(&format!("functions/f{index}.yaml")),
                            meta: meta(),
                        })
                        .collect()
                }),
                environments: vec![ast::HostEnvironment {
                    language: Language::Python,
                    lock: Some(written("uv.lock")),
                    meta: meta(),
                }],
                codelists: vec![],
                submission: Ok(Default::default()),
                paths: ast::Paths::new(),
                meta: meta(),
            }
        }
        /// The function file the project lists at `functions[index]`, as
        /// captured under `identity`.
        fn listed(index: usize, identity: &str) -> Capture<'_> {
            Capture {
                origin: Origin::FunctionFile(index),
                identity,
            }
        }
        /// A scalar a captured file writes out of range.
        fn wide(path: &str, value: &str) -> SF {
            SF {
                node: 0,
                path: path.into(),
                kind: Kind::IntegerRange,
                value: Some(value.into()),
            }
        }
        fn text(value: &str) -> CV {
            CV::String(value.into())
        }

        #[test]
        fn captured_reports_keep_all_origins_beyond_the_old_descriptor_array() {
            // Seam: capacity, 129 generated function files, one beyond the
            // old 128-entry descriptor array.
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
                        definition.name.name = name.clone();
                        definition
                    })
                    .collect(),
            );
            let project = project_root(Some(names.len()));
            let identities = (0..names.len())
                .map(|index| format!("held/function_{index}.yaml"))
                .collect::<Vec<_>>();
            let files = (0..names.len()).collect::<Vec<_>>();
            let captures = identities
                .iter()
                .enumerate()
                .map(|(index, identity)| listed(index, identity))
                .collect::<Vec<_>>();
            let findings = (0..names.len())
                .map(|function| Finding::Function {
                    function,
                    finding: F::MissingImplementation,
                })
                .collect::<Vec<_>>();
            let report = p::admit_admission(
                Admission {
                    project: &project,
                    root: PROJECT,
                    draft: &draft,
                    function_origins: &names,
                    function_files: &files,
                    codelist_origins: &[],
                    captures: captures.iter().copied(),
                },
                &findings,
            )
            .unwrap();
            let want = names
                .iter()
                .zip(&identities)
                .enumerate()
                .map(|(index, (name, identity))| {
                    expected(
                        C::FunctionFileInvalid,
                        "implementations",
                        &[
                            ("function", text(name)),
                            ("reason", text("missing_implementation")),
                        ],
                        identity,
                        &format!("functions[{index}]"),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(report.diagnostics().unwrap(), want);

            let scalar = [wide("result", "9223372036854775808")];
            let report = p::admit_scalar_sources(
                &project,
                PROJECT,
                captures.iter().map(|capture| ScalarSource {
                    capture: *capture,
                    findings: &scalar,
                }),
            )
            .unwrap();
            let want = identities
                .iter()
                .enumerate()
                .map(|(index, identity)| {
                    expected(
                        C::FunctionFileInvalid,
                        "result",
                        &[
                            ("reason", text("integer_out_of_range")),
                            ("value", CV::Number("9223372036854775808".parse().unwrap())),
                        ],
                        identity,
                        &format!("functions[{index}]"),
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
            let mut context: yamaa_core::json::Map<String, CV> = fields
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
                }
                .root(Language::Python)]),
                codelists: vec![],
            }
        }
        #[test]
        fn retained_findings_keep_complete_context_without_a_copy_quota() {
            // Seam: capacity, 600 copies of one finding on a generated
            // 16,384-character name.
            let mut draft = draft();
            let name = "f".repeat(16_384);
            draft.functions.as_mut().unwrap()[0].name.name = name.clone();
            let project = project_root(Some(1));
            let names = [name.clone()];
            let findings = vec![
                Finding::Function {
                    function: 0,
                    finding: F::MissingImplementation,
                };
                600
            ];
            let report = p::admit_admission(
                Admission {
                    project: &project,
                    root: PROJECT,
                    draft: &draft,
                    function_origins: &names,
                    function_files: &[0],
                    codelist_origins: &[],
                    captures: [listed(0, "held/function.yaml")].into_iter(),
                },
                &findings,
            )
            .unwrap();
            let actual = report.diagnostics().unwrap();
            let expected = expected(
                C::FunctionFileInvalid,
                "implementations",
                &[
                    ("function", text(&name)),
                    ("reason", text("missing_implementation")),
                ],
                "held/function.yaml",
                "functions[0]",
            );
            assert_eq!(actual.len(), findings.len());
            assert!(actual.iter().all(|diagnostic| diagnostic == &expected));
        }
        #[test]
        fn malformed_origin_relationships_and_wrong_held_declarations_refuse_explicitly() {
            // Seam: the internal `Error::InvalidOrigin` and `UnavailableShape`
            // refusals, which report no issue.
            let draft = draft();
            let one = project_root(Some(1));
            // A project that lists no function file declares no function.
            let unlisted = project_root(None);
            let names = ["id".into()];
            let wrong_names = ["other".into()];
            let findings = [Finding::LockKindMismatch {
                language: Language::Python,
                actual: LockKind::Renv,
            }];
            let capture = listed(0, "held/function.yaml");
            let unknown = listed(1, "held/unknown.yaml");
            for (project, origins, files, captures) in [
                // The admitted name is not the one the held file declares.
                (&one, &wrong_names, &[0][..], vec![capture]),
                // An admitted definition names no file it was read from.
                (&one, &names, &[][..], vec![capture]),
                // A listed file whose definition was admitted is not held.
                (&one, &names, &[0][..], vec![]),
                (&one, &names, &[0][..], vec![capture, capture]),
                // A held file the project does not list.
                (&one, &names, &[0][..], vec![capture, unknown]),
                (&unlisted, &names, &[0][..], vec![capture]),
            ] {
                assert!(matches!(
                    p::admit_admission(
                        Admission {
                            project,
                            root: PROJECT,
                            draft: &draft,
                            function_origins: origins,
                            function_files: files,
                            codelist_origins: &[],
                            captures: captures.into_iter()
                        },
                        &findings
                    ),
                    Err(Error::InvalidOrigin)
                ));
            }
            // Only a project that lists a function file declares it, so its
            // scalar failures have no declaration to report against.
            let function_findings = [wide("tests[0].result", "9223372036854775808")];
            assert!(matches!(
                p::admit_scalar_sources(
                    &unlisted,
                    PROJECT,
                    [ScalarSource {
                        capture: listed(0, "held/first.yaml"),
                        findings: &function_findings,
                    }]
                    .into_iter()
                ),
                Err(Error::InvalidOrigin)
            ));
            // A scalar finding that kept no written value has nothing to report.
            let scalar = [SF {
                path: "result".into(),
                node: 0,
                kind: Kind::IntegerRange,
                value: None,
            }];
            assert!(matches!(
                p::admit_root_scalars(PROJECT, &scalar),
                Err(Error::Core(
                    project_environment_diagnostics::Error::UnavailableShape
                ))
            ));
        }
        #[test]
        fn immutable_capture_descriptor_handles_consume_non_clone_input_exactly_once() {
            // Seam: the capture iterator is consumed exactly once, a count no
            // issue states.
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
            let project = project_root(Some(1));
            let names = ["id".into()];
            let findings = [Finding::Function {
                function: 0,
                finding: F::MissingImplementation,
            }];
            let calls = Rc::new(Cell::new(0));
            let report = p::admit_admission(
                Admission {
                    project: &project,
                    root: PROJECT,
                    draft: &draft,
                    function_origins: &names,
                    function_files: &[0],
                    codelist_origins: &[],
                    captures: Once {
                        capture: Some(listed(0, "held/function.yaml")),
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
                    C::FunctionFileInvalid,
                    "implementations",
                    &[
                        ("function", text("id")),
                        ("reason", text("missing_implementation"))
                    ],
                    "held/function.yaml",
                    "functions[0]"
                )]
            );
        }
        #[test]
        fn two_hundred_function_files_keep_all_six_hundred_independent_findings_in_order() {
            // Seam: capacity, 200 generated function files and their 600
            // findings.
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
                        definition.name.name = name.clone();
                        definition.tests[0].covers = vec![written("normal"), written("boundary")];
                        definition.tests[0].result = Value::Bool(true);
                        definition.params.clear();
                        definition.tests[0].args.clear();
                        definition
                    })
                    .collect(),
            );
            let project = project_root(Some(names.len()));
            let identities = (0..names.len())
                .map(|index| format!("held/function_{index}.yaml"))
                .collect::<Vec<_>>();
            let files = (0..names.len()).collect::<Vec<_>>();
            let findings = project_environment::validate(&draft);
            assert_eq!(findings.len(), 600);
            let report = p::admit_admission(
                Admission {
                    project: &project,
                    root: PROJECT,
                    draft: &draft,
                    function_origins: &names,
                    function_files: &files,
                    codelist_origins: &[],
                    captures: identities
                        .iter()
                        .enumerate()
                        .map(|(index, identity)| listed(index, identity)),
                },
                &findings,
            )
            .unwrap();
            let mut want = vec![];
            for (index, (name, identity)) in names.iter().zip(&identities).enumerate() {
                let declaration = format!("functions[{index}]");
                want.push(expected(
                    C::FunctionFileInvalid,
                    "tests[0].result",
                    &[
                        ("function", text(name)),
                        ("case", text("ordinary")),
                        ("reason", text("invalid_case_result")),
                        ("value", CV::Bool(true)),
                    ],
                    identity,
                    &declaration,
                ));
                for obligation in ["boundary", "normal"] {
                    want.push(expected(
                        C::FunctionFileInvalid,
                        "tests",
                        &[
                            ("function", text(name)),
                            ("reason", text("missing_coverage_obligation")),
                            ("obligation", text(obligation)),
                        ],
                        identity,
                        &declaration,
                    ));
                }
            }
            assert_eq!(report.diagnostics().unwrap(), want);
        }
    }
    mod project_function {
        use crate::support::roots::{Case, Definition, Parameter};
        use yamaa_core::{
            project_function::{validate, Finding, Function, Language},
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
            // Seam: the admitted definition, a value no issue states.
            for def in [integer(), nullable_float()] {
                assert_eq!(validate(&def.clone().root(Language::Python)), vec![]);
                let expected = def.clone();
                let admitted =
                    Function::admit(Language::Python, def.root(Language::Python)).unwrap();
                assert_eq!(admitted.definition(), &expected.root(Language::Python));
            }
            let mut r = integer();
            r.function = "project::identity".into();
            assert!(Function::admit(Language::R, r.root(Language::R)).is_ok());
        }

        #[test]
        fn every_missing_obligation_is_reported_independently() {
            // Seam: `validate`'s internal function `Finding` model.
            let mut def = nullable_float();
            def.tests.clear();
            assert_eq!(
                validate(&def.clone().root(Language::Python)),
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
            // Seam: `validate`'s internal function `Finding` model.
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
            let f = validate(&def.clone().root(Language::Python));
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
            // Seam: `validate`'s internal function `Finding` model.
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
                let f = validate(&def.clone().root(Language::Python));
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
                validate(&def.clone().root(Language::Python))[0],
                Finding::RequiredDefault { parameter: 0 }
            );
        }

        #[test]
        fn tagged_evidence_requires_the_named_behavior_and_an_invoked_nullable_result() {
            // Seam: `validate`'s internal function `Finding` model.
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
                &validate(&def.clone().root(Language::Python))[..5],
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
            let f = validate(&def.clone().root(Language::Python));
            assert!(f.contains(&Finding::InvalidResult { case: 2 }));
            assert!(f.contains(&Finding::UnsupportedEvidence { case: 2, tag: 0 }));
            assert!(f.contains(&Finding::MissingObligation(
                "short-circuit-missing:x".into()
            )));
        }

        #[test]
        fn duplicate_ids_tags_and_unknown_obligations_do_not_hide_findings() {
            // Seam: `validate`'s internal function `Finding` model.
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
            let f = validate(&def.clone().root(Language::Python));
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
            // Seam: `validate`'s internal function `Finding` model.
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
                    validate(&def.clone().root(Language::Python))
                        .contains(&Finding::InvalidImplementation(Language::Python)),
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
                    validate(&def.clone().root(Language::R))
                        .contains(&Finding::InvalidImplementation(Language::R)),
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
                assert!(validate(&def.clone().root(language))
                    .contains(&Finding::InvalidParameterName { parameter: 0 }));
            }
        }

        #[test]
        fn all_definition_and_result_faults_have_independent_diagnostics() {
            // Seam: `validate`'s internal function `Finding` model.
            let mut def = integer();
            def.name.clear();
            def.function.clear();
            def.description.clear();
            def.comparison_decimals = -1;
            def.params.push(def.params[0].clone());
            def.tests[0].args.clear();
            def.tests[0].result = Value::Bool(true);
            let f = validate(&def.clone().root(Language::Python));
            assert_eq!(
                &f[..8],
                &[
                    Finding::InvalidName,
                    Finding::InvalidImplementation(Language::Python),
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
                assert!(validate(&def.clone().root(Language::Python))
                    .contains(&Finding::InvalidResult { case: 0 }));
            }
        }

        #[test]
        fn static_call_binding_checks_all_names_and_exact_types_without_missing_shortcuts() {
            // Seam: `bind_call`'s internal `CallFinding` model.
            use yamaa_core::project_function::{CallArgument, CallFinding};
            let function =
                Function::admit(Language::Python, nullable_float().root(Language::Python)).unwrap();
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
