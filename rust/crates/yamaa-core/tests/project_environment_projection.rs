#[path = "diagnostic/project_environment.rs"]
mod support;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    collections::BTreeMap,
};
use yamaa_core::{
    diagnostic::{ConditionCode as C, ContextValue as CV, Diagnostic},
    project_environment::{self, Draft, Finding, LockKind, LockReference},
    project_environment_diagnostics::{
        self,
        projection::{self as p, Admission, Capture, Error, Limits, Origin, Root, ScalarSource},
    },
    project_function::{Case, Definition, Finding as F, Language, Parameter},
    project_function_document::{Finding as SF, Kind},
    schema::{Document, DocumentLimits, DocumentNode as N},
    value::{ColumnType, Value, ValueType},
};
struct Allocator;
thread_local! { static TRACK: Cell<bool> = const { Cell::new(false) }; static COPIES: Cell<usize> = const { Cell::new(0) }; }
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACK.with(|track| {
            if track.get() {
                COPIES.with(|copies| copies.set(copies.get() + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        TRACK.with(|track| {
            if track.get() {
                COPIES.with(|copies| copies.set(copies.get() + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn allocations<T>(f: impl FnOnce() -> T) -> (T, usize) {
    COPIES.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    let result = f();
    TRACK.with(|track| track.set(false));
    (result, COPIES.with(Cell::get))
}
fn document(nodes: Vec<N>) -> Document {
    let root = nodes.len() - 1;
    Document::new(nodes, root, DocumentLimits::default()).unwrap()
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
    CV::Scalar(Value::Str(value.into()))
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
fn independent_complete_eighteen_record_environment_report_preserves_every_field_and_order() {
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
        Limits::default(),
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
            &[
                ("codelist", text("SEX")),
                ("value", CV::Scalar(Value::Bool(true))),
            ],
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
            &[
                ("codelist", text("SEX")),
                ("value", CV::Scalar(Value::Bool(true))),
            ],
            "held/environment.yaml",
            "codelists[0]",
        ));
        want.push(expected(
            C::CodelistDuplicateValue,
            &format!("{base}.items[1].value"),
            &[
                ("codelist", text("SEX")),
                ("value", CV::Scalar(Value::Bool(true))),
            ],
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
                ("value", CV::Scalar(Value::Int(i64::MIN))),
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
                ("value", CV::Scalar(Value::Int(i64::MIN))),
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
                ("value", CV::Scalar(Value::Int(i64::MIN))),
            ],
        ),
        (
            F::InvalidResult { case: 0 },
            "tests[0].result",
            "invalid_case_result",
            vec![
                ("case", text("ordinary")),
                ("value", CV::Scalar(Value::Int(i64::MAX))),
            ],
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
            Limits::default(),
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
fn independent_text_and_issue_limits_admit_no_owned_facts_even_on_late_refusal() {
    let draft = draft();
    let doc = root_document(&[("id", false)], &[]);
    let names = ["id".into()];
    let finding = Finding::Function {
        function: 0,
        finding: F::InvalidCallable,
    };
    let repeated = vec![finding.clone(); 50_000];
    let input = || Admission {
        root: Root {
            document: &doc,
            identity: "held/environment.yaml",
        },
        draft: &draft,
        function_origins: &names,
        codelist_origins: &[],
        captures: std::iter::empty(),
    };
    // This fixed independent raw text boundary is inherited from the legacy API.
    assert!(p::admit_admission(
        input(),
        std::slice::from_ref(&finding),
        Limits {
            semantic_text_bytes: 181,
            issue_bytes: usize::MAX
        }
    )
    .is_ok());
    let pair = [finding.clone(), finding.clone()];
    let (semantic, copies) = allocations(|| {
        p::admit_admission(
            input(),
            &pair,
            Limits {
                semantic_text_bytes: 361,
                issue_bytes: usize::MAX,
            },
        )
    });
    // Finding fixture cloning above allocates no authored strings for this cause.
    assert!(matches!(
        semantic,
        Err(Error::Core(project_environment_diagnostics::Error::Limit))
    ));
    assert_eq!(copies, 0);
    let limits = Limits {
        semantic_text_bytes: usize::MAX,
        issue_bytes: 2_000,
    };
    assert!(p::admit_admission(input(), std::slice::from_ref(&finding), limits).is_ok());
    let (late, copies) = allocations(|| p::admit_admission(input(), &repeated, limits));
    assert!(matches!(late, Err(Error::IssueLimit)));
    assert_eq!(copies, 0);
    let (tiny, copies) = allocations(|| {
        p::admit_admission(
            input(),
            &repeated,
            Limits {
                semantic_text_bytes: usize::MAX,
                issue_bytes: 1,
            },
        )
    });
    assert!(matches!(tiny, Err(Error::IssueLimit)));
    assert_eq!(copies, 0);
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
                &findings,
                Limits::default()
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
            None,
            Limits::default()
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
            None,
            Limits::default()
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
    let report =
        p::admit_root_scalars(root, &findings, Some(Language::Python), Limits::default()).unwrap();
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
                    ("value", CV::Integer(wide.into()))
                ],
                "held/environment.yaml",
                "$"
            )
        ]
    );
    let limits = Limits {
        issue_bytes: 2_000,
        ..Default::default()
    };
    assert!(p::admit_root_scalars(root, &findings, None, limits).is_ok());
    let (all, copies) =
        allocations(|| p::admit_root_scalars(root, &findings, Some(Language::Python), limits));
    assert!(matches!(all, Err(Error::IssueLimit)));
    assert_eq!(copies, 0);
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
    let limits = Limits {
        issue_bytes: 2_000,
        ..Default::default()
    };
    assert!(p::admit_scalar_sources(root, sources[..1].iter().copied(), limits).is_ok());
    let (all, copies) =
        allocations(|| p::admit_scalar_sources(root, sources.iter().copied(), limits));
    assert!(matches!(all, Err(Error::IssueLimit)));
    assert_eq!(copies, 0);
    let result = p::admit_scalar_sources(root, sources.iter().copied(), Limits::default())
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
                    ("value", CV::Integer("9223372036854775808".into()))
                ],
                "held/first.yaml",
                "codelists[1]"
            ),
            expected(
                C::ProjectEnvironmentInvalid,
                "items[0].value",
                &[
                    ("reason", text("integer_out_of_range")),
                    ("value", CV::Integer("9223372036854775808".into()))
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
        Limits::default(),
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
        Limits::default(),
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
                ("value", CV::Scalar(Value::Bool(true))),
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
