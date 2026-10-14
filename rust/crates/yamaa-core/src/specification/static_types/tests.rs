use super::*;
use crate::{
    diagnostic::ConditionCode as C,
    schema::{DocumentLimits, SpecificationDocument, ValidationBudget},
};

#[derive(Clone)]
enum Tree<'a> {
    Scalar(N),
    Text(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Text(text) => N::Text(text.into()),
            Self::List(values) => N::Sequence(
                values
                    .into_iter()
                    .map(|value| value.append(nodes))
                    .collect(),
            ),
            Self::Map(values) => N::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| (Self::Text(key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
}
fn operation<'a>(name: &'a str, fields: Vec<(&'a str, Tree<'a>)>) -> Tree<'a> {
    Tree::Map(vec![(name, Tree::Map(fields))])
}
fn literal(value: N) -> Tree<'static> {
    Tree::Map(vec![("literal", Tree::Scalar(value))])
}
fn column<'a>(name: &'a str, kind: &'a str, value: Tree<'a>) -> Tree<'a> {
    use Tree::*;
    Map(vec![
        ("name", Text(name)),
        ("type", Text(kind)),
        ("derivation", Map(vec![("value", value)])),
    ])
}
fn document(value: Tree<'_>, verification: Option<Tree<'_>>) -> SpecificationDocument {
    use Tree::*;
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("SRC.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column(
                    "ID",
                    "int",
                    operation("source", vec![("variable", Text("SRC.ID"))]),
                ),
                column("TERM", "str", literal(N::Text("alpha".into()))),
                column("DAY", "date", literal(N::Null)),
                column("COUNT", "int", literal(N::Integer("1".into()))),
                column("RESULT", "float", value),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("RESULT")])),
            ]),
        ),
    ];
    if let Some(verification) = verification {
        fields.push(("verifications", List(vec![verification])));
    }
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    SpecificationDocument::admit(
        Document::new(nodes, root, DocumentLimits::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap()
}
fn source() -> TableSchema {
    TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}
fn binding_codes(value: Tree<'_>, verification: Option<Tree<'_>>) -> Vec<C> {
    let spec = PreparedSpecification::prepare(&document(value, verification)).unwrap();
    let BindError::Invalid(findings) = spec.bind(&source()).unwrap_err() else {
        panic!("typed binding findings")
    };
    findings
        .into_iter()
        .flat_map(|finding| finding.diagnostics(spec.source()).unwrap())
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn independent_case_types_survive_an_unknown_operand() {
    use Tree::*;
    let case = Map(vec![(
        "case",
        List(vec![Map(vec![
            (
                "when",
                Text("UNKNOWN = 1 OR DAY = '2025-01-01' OR COUNT LIKE '1%'"),
            ),
            ("then", literal(N::Integer("1".into()))),
        ])]),
    )]);
    let codes = binding_codes(case, None);
    assert_eq!(codes.len(), 3);
    assert!(codes.contains(&C::PredicateIncompatiblePair));
    assert!(codes.contains(&C::PredicateExpectedText));
}

#[test]
fn compute_and_reduction_reject_known_text_without_evaluation() {
    assert_eq!(
        binding_codes(
            operation("compute", vec![("expr", Tree::Text("TERM + 1"))]),
            None
        ),
        vec![C::NumericInputType]
    );
    assert_eq!(
        binding_codes(
            operation(
                "aggregate",
                vec![
                    ("group_by", Tree::List(vec![Tree::Text("ID")])),
                    ("expr", Tree::Text("SUM(TERM)"))
                ]
            ),
            None
        ),
        vec![C::DatasetReductionInputType]
    );
}

#[test]
fn numeric_and_aggregate_result_kinds_are_native_metadata() {
    let numeric = crate::numeric_compiler::compile_numeric_with_policy(
        "A + POWER(B, 2) / -3",
        "result",
        Default::default(),
        crate::numeric_compiler::MathPolicy::PortableLibmV1,
    )
    .unwrap();
    assert_eq!(numeric.result_type(), ColumnType::Float);
    let aggregate = crate::aggregate_compiler::compile_aggregate(
        "SUM(EX.DOSE) / NULLIF(EX.PLANDOSE, 0)",
        "result",
        Default::default(),
    )
    .unwrap();
    assert_eq!(aggregate.numeric_result_type(), Some(ColumnType::Float));
    let bare =
        crate::aggregate_compiler::compile_aggregate("MIN(EX.TERM)", "result", Default::default())
            .unwrap();
    assert_eq!(bare.numeric_result_type(), None);
}

#[test]
fn independent_calendar_findings_keep_first_error_priority() {
    use crate::{
        dataset::{date_policy, date_policy_findings, DatePolicyError, DayDeclaration},
        temporal::DatePrecision,
    };
    let findings = date_policy_findings(
        Some("13"),
        DayDeclaration::Number("0"),
        DatePrecision::Year,
        "date",
    )
    .unwrap()
    .unwrap_err();
    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        vec![C::ImputationMonthRange, C::ImputationDayRange]
    );
    let DatePolicyError::Diagnostic(first) = date_policy(
        Some("13"),
        DayDeclaration::Number("0"),
        DatePrecision::Year,
        "date",
    )
    .unwrap_err() else {
        panic!("first finding")
    };
    assert_eq!(first, findings[0]);
    let invalid = operation(
        "date_impute",
        vec![
            ("source", Tree::Text("TERM")),
            ("month", Tree::Scalar(N::Integer("13".into()))),
            ("day", Tree::Scalar(N::Integer("0".into()))),
        ],
    );
    let PrepareError::Invalid(findings) =
        PreparedSpecification::prepare(&document(invalid, None)).unwrap_err()
    else {
        panic!("two declaration findings")
    };
    assert_eq!(
        findings
            .into_iter()
            .map(|finding| finding.diagnostic().code)
            .collect::<Vec<_>>(),
        vec![C::ImputationMonthRange, C::ImputationDayRange]
    );
}

#[test]
fn independent_cut_findings_keep_first_error_priority() {
    use crate::mapping_cut::{AdmissionError, Cut};
    let labels = vec![String::from("a")];
    let findings = Cut::declaration_findings("COUNT", "cut", &[10.0, 5.0], &labels).unwrap();
    assert_eq!(findings.len(), 2);
    assert_ne!(findings[0].context, findings[1].context);
    let AdmissionError::Diagnostic(first) = Cut::new(
        "COUNT".into(),
        "cut".into(),
        vec![10.0, 5.0],
        labels,
        false,
        None,
    )
    .unwrap_err() else {
        panic!("first finding")
    };
    assert_eq!(first, findings[0]);
    let invalid = operation(
        "cut",
        vec![
            ("source", Tree::Text("COUNT")),
            (
                "breaks",
                Tree::List(vec![
                    Tree::Scalar(N::Integer("10".into())),
                    Tree::Scalar(N::Integer("5".into())),
                ]),
            ),
            ("labels", Tree::List(vec![Tree::Text("a")])),
        ],
    );
    let PrepareError::Invalid(findings) =
        PreparedSpecification::prepare(&document(invalid, None)).unwrap_err()
    else {
        panic!("two declaration findings")
    };
    assert_eq!(findings.len(), 2);
}

#[test]
fn repeated_type_findings_refuse_whole_projection_before_output() {
    let mut terms = (0..1024)
        .map(|index| format!("X{index}"))
        .collect::<Vec<_>>();
    while terms.len() > 1 {
        terms = terms
            .chunks_exact(2)
            .map(|pair| format!("({} + {})", pair[0], pair[1]))
            .collect();
    }
    let expression = terms.pop().unwrap();
    let plan = crate::numeric_compiler::compile_numeric(&expression, "result", Default::default())
        .unwrap();
    assert!(matches!(
        numeric_kinds(&plan, |_| Some(crate::value::ValueType::Str)),
        Err(BindError::PredicateDiagnosticLimit)
    ));
}
