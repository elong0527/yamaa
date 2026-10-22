use super::rendered;
use crate::support::project_call_compiler as support;
use crate::support::specification::prepare_in;
use std::collections::BTreeSet;
use support::{call, definition, environment, specification, specification_with_rows, Tree};
use yamaa_core::json::Value as JsonValue;
use yamaa_core::{
    diagnostic::ConditionCode,
    project_environment::{Draft, Environment},
    project_function::Language,
    schema::DocumentNode as N,
    specification::PrepareError,
};
pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (name, value, functions, condition, rule, path) in [
        (
            "unknown",
            N::Integer("7".into()),
            vec![],
            "unknown_project_function",
            "FUNC-4",
            "columns.VALUE.derivation.function.name",
        ),
        (
            "id",
            N::Float(7.0),
            vec!["id"],
            "invalid_function_argument",
            "FUNC-5",
            "columns.VALUE.derivation.function.args.x",
        ),
    ] {
        let spec = specification(vec![(
            "VALUE",
            "int",
            call(name, vec![("x", Tree::Scalar(value))]),
        )]);
        let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &environment(&functions))
        else {
            panic!("compiler-owned function cause")
        };
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().rule
            ),
            ("validation", condition, Some(rule))
        );
        assert_eq!(diagnostic.spec_paths, [path]);
        assert_eq!(
            diagnostic.context["function"],
            JsonValue::String(name.into())
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        if name == "id" {
            assert_eq!(
                diagnostic.context["argument"],
                JsonValue::String("x".into())
            );
            assert_eq!(
                diagnostic.context["cause"],
                JsonValue::String("type_mismatch".into())
            );
            assert_eq!(
                diagnostic.context["expected_type"],
                JsonValue::String("int".into())
            );
            assert_eq!(
                diagnostic.context["actual_type"],
                JsonValue::String("float".into())
            );
            assert_eq!(diagnostic.context.len(), 5);
        } else {
            assert_eq!(diagnostic.context.len(), 1);
        }
        reached.insert(rendered(&diagnostic));
    }
    // A supplied 0.5 project that names no environment file for the host
    // reports each call against that absent environment rather than the
    // function name. Only a 0.5 project links environment files, so the cause
    // stays until #3448 deletes them (D8).
    let undeclared = Environment::admit(
        Some(Language::R),
        Draft {
            surface: yamaa_core::ast::Surface::V0_5,
            language: None,
            lock: None,
            functions: None,
            codelists: vec![],
        },
    )
    .unwrap()
    .into_execution();
    let spec = specification(vec![(
        "VALUE",
        "int",
        call("id", vec![("x", Tree::Scalar(N::Integer("7".into())))]),
    )]);
    let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &undeclared) else {
        panic!("undeclared environment cause")
    };
    assert_eq!(findings.len(), 1);
    let diagnostic = findings[0].diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "project_environment_missing", Some("FUNC-1"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.VALUE.derivation.function.name"]
    );
    assert_eq!(
        diagnostic.context,
        [
            ("function".into(), JsonValue::String("id".into())),
            ("language".into(), JsonValue::String("r".into())),
            ("cause".into(), JsonValue::String("undeclared".into())),
        ]
        .into()
    );
    assert_eq!(diagnostic.source_span, None);
    assert_eq!(diagnostic.operand_route, None);
    reached.insert(rendered(&diagnostic));
    // A 1.0 function file that names no callable for the host language is
    // admitted, and each call of it fails before any data is read (FUNC-1).
    let unimplemented = Environment::admit(
        Some(Language::Python),
        Draft {
            surface: yamaa_core::ast::Surface::V1 { minor: 0 },
            language: Some(Language::Python),
            lock: None,
            functions: Some(vec![definition("id").file(&[(Language::R, "project::id")])]),
            codelists: vec![],
        },
    )
    .unwrap()
    .into_execution();
    let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &unimplemented) else {
        panic!("missing implementation cause")
    };
    assert_eq!(findings.len(), 1);
    let diagnostic = findings[0].diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        (
            "validation",
            "function_implementation_missing",
            Some("FUNC-1")
        )
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.VALUE.derivation.function.name"]
    );
    assert_eq!(
        diagnostic.context,
        [
            ("function".into(), JsonValue::String("id".into())),
            ("language".into(), JsonValue::String("python".into())),
        ]
        .into()
    );
    reached.insert(rendered(&diagnostic));
    // An engine that claims no host language reads no 0.5 environment file,
    // so it knows no function a 0.5 project declares: each call is refused as
    // unsupported before any data is read (#3030 section 2.10, D110). The 0.5
    // project stays until #3448 deletes environment files (D8).
    let neutral = Environment::admit(
        None,
        Draft {
            surface: yamaa_core::ast::Surface::V0_5,
            language: None,
            lock: None,
            functions: None,
            codelists: vec![],
        },
    )
    .unwrap()
    .into_execution();
    let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &neutral) else {
        panic!("no host language cause")
    };
    assert_eq!(findings.len(), 1);
    let diagnostic = findings[0].diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("planning", "unsupported_operation", None)
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.VALUE.derivation.function.name"]
    );
    assert_eq!(
        diagnostic.context,
        [
            ("function".into(), JsonValue::String("id".into())),
            ("operation".into(), JsonValue::String("environment".into())),
        ]
        .into()
    );
    reached.insert(rendered(&diagnostic));
    // A 1.0 project's function files are language-neutral and read by every
    // engine, so one that claims no host language compiles a call of any of
    // them and misses no implementation (FUNC-1); a run of the call is refused
    // instead (D110).
    let neutral = Environment::admit(
        None,
        Draft {
            surface: yamaa_core::ast::Surface::V1 { minor: 0 },
            language: None,
            lock: None,
            functions: Some(vec![definition("id").file(&[(Language::R, "project::id")])]),
            codelists: vec![],
        },
    )
    .unwrap()
    .into_execution();
    let compiled = prepare_in(&spec, &neutral).expect("a call compiles without a host language");
    assert_eq!(compiled.called_functions(), [0]);
    assert_eq!(neutral.functions()[0].call(), "");
    let source = Tree::text("SRC.ID");
    let spec = specification_with_rows(
        vec![
            (
                "A",
                "int",
                Tree::map(vec![(
                    "aggregate",
                    Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
                )]),
            ),
            ("B", "int", Tree::Scalar(N::Integer("0".into()))),
        ],
        vec![Tree::map(vec![
            ("id", Tree::text("record")),
            (
                "derivations",
                Tree::map(vec![
                    ("ID", Tree::map(vec![("value", source)])),
                    (
                        "B",
                        Tree::map(vec![("value", call("id", vec![("x", Tree::text("A"))]))]),
                    ),
                ]),
            ),
        ])],
    );
    let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &environment(&["id"])) else {
        panic!("row phase cause")
    };
    assert_eq!(findings.len(), 1);
    let diagnostic = findings[0].diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "phase_boundary", Some("COLUMN-4"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["rows.record.derivations.B.function.args.x"]
    );
    for (key, value) in [
        ("function", "id"),
        ("argument", "x"),
        ("identifier", "A"),
        ("row", "record"),
        ("available_phase", "column_derivation"),
        ("required_phase", "row_construction"),
    ] {
        assert_eq!(diagnostic.context[key], JsonValue::String(value.into()));
    }
    assert_eq!(diagnostic.context.len(), 6);
    reached.insert(rendered(&diagnostic));
    reached
}

/// A project function argument of an intermediate's derivation reads another
/// intermediate that declares no `keep`: one value per donor record needs
/// one selected record (LOOKUP-22). The declarations decide it, so
/// preparation reports it, and `check()` with it.
pub fn donor_keep_reached() -> BTreeSet<ConditionCode> {
    use yamaa_core::specification::PreflightFinding;
    let source = |name: &str| Tree::map(vec![("value", Tree::text(name))]);
    let input = |path: &str| Tree::map(vec![("path", Tree::text(path))]);
    // Each intermediate a derivation reads declares the key 0.5 inferred
    // (D23, D111); the keep finding precedes binding.
    let spec = Tree::map(vec![
        ("yamaa", Tree::text("1.0")),
        ("type", Tree::text("spec")),
        ("domain", Tree::text("TEST")),
        ("base", Tree::text("SRC")),
        (
            "input",
            Tree::map(vec![
                ("SRC", input("src.csv")),
                ("DONOR", input("donor.csv")),
                ("OBS", input("obs.csv")),
            ]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        (
            "intermediates",
            Tree::List(vec![
                Tree::map(vec![
                    ("id", Tree::text("SUBJ")),
                    ("dataset", Tree::text("DONOR")),
                    ("key", Tree::List(vec![Tree::text("ID")])),
                    ("filter", Tree::text("DONOR.N > 0")),
                ]),
                Tree::map(vec![
                    ("id", Tree::text("FIRST")),
                    ("dataset", Tree::text("OBS")),
                    ("key", Tree::List(vec![Tree::text("ID")])),
                    (
                        "derivations",
                        Tree::map(vec![(
                            "NV",
                            Tree::map(vec![(
                                "value",
                                call("id", vec![("x", Tree::text("SUBJ.N"))]),
                            )]),
                        )]),
                    ),
                    (
                        "order_by",
                        Tree::List(vec![Tree::map(vec![("variable", Tree::text("OBS.K"))])]),
                    ),
                    ("keep", Tree::text("first")),
                ]),
            ]),
        ),
        (
            "columns",
            Tree::List(vec![
                Tree::map(vec![
                    ("name", Tree::text("ID")),
                    ("type", Tree::text("int")),
                    ("derivation", source("SRC.ID")),
                ]),
                Tree::map(vec![
                    ("name", Tree::text("NV")),
                    ("type", Tree::text("int")),
                    ("derivation", source("FIRST.NV")),
                ]),
            ]),
        ),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("out.csv")),
                (
                    "columns",
                    Tree::List(vec![Tree::text("ID"), Tree::text("NV")]),
                ),
            ]),
        ),
    ])
    .document();
    let Err(PrepareError::Invalid(findings)) = prepare_in(&spec, &environment(&["id"])) else {
        panic!("a donor function argument over an unkept intermediate must fail")
    };
    let [PreflightFinding::Scalar(diagnostic)] = findings.as_slice() else {
        panic!("one donor keep finding, not {findings:?}")
    };
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "invalid_derive_step", Some("LOOKUP-22"))
    );
    assert_eq!(
        diagnostic.spec_paths,
        ["intermediates.FIRST.derivations.NV.function.args.x"]
    );
    let text = |value: &str| JsonValue::String(value.into());
    assert_eq!(
        diagnostic.context,
        [
            ("function".into(), text("id")),
            ("argument".into(), text("x")),
            ("intermediate".into(), text("SUBJ")),
            (
                "reason".into(),
                text("a donor function argument reads only keep-declared named intermediates"),
            ),
        ]
        .into()
    );
    BTreeSet::from([rendered(&diagnostic)])
}
