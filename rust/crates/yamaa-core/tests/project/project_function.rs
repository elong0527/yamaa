use yamaa_core::{
    project_function::{validate, Case, Definition, Finding, Function, Language, Parameter},
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
        assert!(validate(language, &def).contains(&Finding::InvalidParameterName { parameter: 0 }));
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
        assert!(validate(Language::Python, &def).contains(&Finding::InvalidResult { case: 0 }));
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
