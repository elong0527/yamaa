#[allow(dead_code, clippy::duplicate_mod)]
#[path = "support/project_call_compiler.rs"]
mod support;
use support::{specification, Tree};
use yamaa_core::{
    bound_expression::Read,
    dataset::{Flag, FlagBinding, FlagError, FlagRead},
    diagnostic::ConditionCode as C,
    predicate::{Condition, ErrorKind, EvaluationError, Route, Truth},
    predicate_compiler,
    schema::DocumentNode as N,
    specification::{BindError, CompilationLimits, PrepareError, PreparedSpecification},
    table::{Column, TableSchema, ValueRef},
    value::{ColumnType, Value, ValueType},
};
fn flag(condition: &str, false_value: Option<Tree>, missing: Option<Tree>) -> Tree {
    let mut fields = vec![("condition", Tree::text(condition))];
    if let Some(v) = false_value {
        fields.push(("false_value", v));
    }
    if let Some(v) = missing {
        fields.push(("missing", v));
    }
    Tree::map(vec![("flag", Tree::map(fields))])
}
fn diagnostics(columns: Vec<(&str, &str, Tree)>) -> Vec<yamaa_core::diagnostic::Diagnostic> {
    let spec = specification(columns);
    let prepared = PreparedSpecification::prepare(&spec).unwrap();
    let source = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "EXTRA".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
        panic!("findings")
    };
    findings
        .iter()
        .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
        .collect()
}
#[test]
fn raw_truth_literals_presence_null_and_complete_name_map() {
    let plan = predicate_compiler::compile("ID = 1", "flag.condition", Default::default()).unwrap();
    let bindings = vec![FlagBinding {
        name: "ID".into(),
        read: FlagRead::Read(Read::Column(0)),
    }];
    let flag = Flag::new(
        plan.clone(),
        bindings.clone(),
        Value::Str("bad".into()),
        Some(Value::Int(2)),
        Some(Value::Missing),
    )
    .unwrap();
    assert_eq!(flag.value(Truth::True), ValueRef::Str("bad"));
    assert_eq!(flag.value(Truth::False), ValueRef::Int(2));
    assert_eq!(flag.value(Truth::Unknown), ValueRef::Missing);
    assert_eq!(
        Flag::new(
            plan.clone(),
            bindings.clone(),
            Value::Int(1),
            Some(Value::Missing),
            None
        )
        .unwrap_err(),
        FlagError::MissingValueRequired
    );
    assert!(matches!(
        Flag::new(plan, vec![], Value::Missing, None, None),
        Err(FlagError::Binding(_))
    ));
}
#[test]
fn semantic_flag_declarations_are_deferred_and_interleaved_before_name_binding() {
    let f = diagnostics(vec![
        ("A", "str", flag("ABSENT = 1", Some(Tree::text("N")), None)),
        ("B", "str", flag("ID >> 1", Some(Tree::text("N")), None)),
    ]);
    assert_eq!(
        f.iter().map(|d| d.code).collect::<Vec<_>>(),
        [
            C::FlagMissingValueRequired,
            C::PredicateUnknownField,
            C::PredicateInvalidExpression,
            C::FlagMissingValueRequired
        ]
    );
    assert_eq!(f[0].spec_paths, ["columns.A.derivation.flag.missing"]);
    assert_eq!(f[1].spec_paths, ["columns.A.derivation.flag.condition"]);
    let f = diagnostics(vec![(
        "A",
        "str",
        flag("ABSENT = 1", Some(Tree::Scalar(N::Null)), None),
    )]);
    assert_eq!(f[0].code, C::FlagMissingValueRequired);
}
#[test]
fn nested_flag_declarations_precede_enclosing_case_reference_checkpoint() {
    let case = Tree::map(vec![(
        "case",
        Tree::List(vec![
            Tree::map(vec![
                ("when", Tree::text("UNKNOWN = 1")),
                ("then", flag("ID >> 1", Some(Tree::text("N")), None)),
            ]),
            Tree::map(vec![(
                "otherwise",
                Tree::map(vec![("literal", Tree::text("x"))]),
            )]),
        ]),
    )]);
    assert_eq!(
        diagnostics(vec![("F", "str", case)])
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        [
            C::PredicateInvalidExpression,
            C::FlagMissingValueRequired,
            C::SourceUnknownReference
        ]
    );
}
#[test]
fn whole_expression_quota_precedes_deferred_grammar_and_name_findings() {
    let spec = specification(vec![
        ("A", "str", flag("ID >> 1", None, None)),
        ("B", "str", flag("ABSENT = 1", None, None)),
    ]);
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &spec,
            CompilationLimits {
                numeric_bytes: 8,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("numeric_bytes"))
    ));
}
#[test]
fn regex_input_type_is_flag_owned_without_changing_generic_predicate_mapping() {
    let condition = Condition::ContainsExpectedText {
        actual: ValueType::Int,
    };
    assert_eq!(condition.requirement(), "REQ-1244");
    let error: EvaluationError<()> = EvaluationError {
        spec_path: "flag.condition".into(),
        expression: "original".into(),
        route: vec![Route::Value],
        kind: ErrorKind::Condition(condition),
    };
    let diagnostic = yamaa_core::dataset::flag_condition_diagnostic(&error).unwrap();
    assert_eq!(diagnostic.code, C::PredicateInputType);
    assert_eq!(diagnostic.definition().requirement, Some("REQ-0190"));
    assert_eq!(diagnostic.operand_route, Some(vec!["Value".into()]));
}

#[allow(dead_code)]
pub fn reached() -> std::collections::BTreeSet<C> {
    let mut reached = diagnostics(vec![(
        "F",
        "str",
        flag("ABSENT = 1", Some(Tree::text("N")), None),
    )])
    .into_iter()
    .map(|d| d.code)
    .collect::<std::collections::BTreeSet<_>>();
    reached.extend(
        diagnostics(vec![("F", "str", flag("EXTRA = 1", None, None))])
            .into_iter()
            .map(|d| d.code),
    );
    struct IntPort;
    impl yamaa_core::predicate::Resolver for IntPort {
        type Error = std::convert::Infallible;
        fn resolve(&mut self, _: &str) -> Result<yamaa_core::value::Selection, Self::Error> {
            Ok(yamaa_core::value::Selection::Present(Value::Int(1)))
        }
    }
    let plan = predicate_compiler::compile(
        "str_contains(SRC.EXTRA,'a')",
        "flag.condition",
        Default::default(),
    )
    .unwrap();
    let finding =
        yamaa_core::dataset::flag_condition_diagnostic(&plan.evaluate(&mut IntPort).unwrap_err())
            .unwrap();
    assert_eq!(finding.definition().requirement, Some("REQ-0190"));
    reached.insert(finding.code);
    assert_eq!(
        reached,
        [
            C::FlagMissingValueRequired,
            C::PredicateUnknownField,
            C::PredicateUnresolvableName,
            C::PredicateInputType
        ]
        .into()
    );
    reached
}

#[test]
fn all_unselected_literals_and_repeated_row_defaults_are_admitted_before_ownership() {
    use support::specification_with_rows;
    let names = (0..40).map(|i| format!("F{i}")).collect::<Vec<_>>();
    let columns = names
        .iter()
        .map(|name| {
            (
                name.as_str(),
                "str",
                flag(
                    "TRUE",
                    Some(Tree::text(&"x".repeat(10_000))),
                    Some(Tree::Scalar(N::Null)),
                ),
            )
        })
        .collect();
    let rows = (0..16)
        .map(|i| {
            Tree::Map(vec![
                ("id".into(), Tree::text(&format!("r{i}"))),
                ("derivations".into(), Tree::map(vec![])),
            ])
        })
        .collect();
    let spec = specification_with_rows(columns, rows);
    assert!(matches!(
        PreparedSpecification::prepare_with_limits(
            &spec,
            CompilationLimits {
                model_text_bytes: 1_048_576,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("flag_metadata"))
    ));
}

#[test]
fn existing_between_case_binds_one_name_and_preserves_two_subject_occurrences() {
    let case = Tree::map(vec![(
        "case",
        Tree::List(vec![Tree::map(vec![
            ("when", Tree::text("ID BETWEEN 1 AND 2")),
            ("then", flag("ID = 1", None, None)),
        ])]),
    )]);
    let spec = specification(vec![("F", "str", case)]);
    let prepared = PreparedSpecification::prepare(&spec).unwrap();
    let source = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let plan = prepared.bind(&source).unwrap();
    let yamaa_core::dataset::Expression::Case(branches) = &plan.columns()[0].expression else {
        panic!("case")
    };
    let when = branches[0].when.as_ref().unwrap();
    assert_eq!(when.bindings().len(), 1);
    assert_eq!(when.plan().identifiers(), ["ID", "ID"]);
}
