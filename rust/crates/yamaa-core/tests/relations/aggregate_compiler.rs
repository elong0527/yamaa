use yamaa_core::{
    aggregate_compiler::{compile_aggregate, AggregateOperand, AggregateResult},
    evaluation::NumericResolver,
    numeric::Number,
    value::{Selection, Value},
};

struct Resolver {
    calls: Vec<String>,
}

#[test]
fn all_retained_aggregate_plans_keep_the_complete_expression_and_authored_path() {
    let name = format!("D.{}", "A".repeat(60));
    let mut terms = vec![format!("SUM({name} + 1)"); 512];
    while terms.len() > 1 {
        terms = terms
            .chunks(2)
            .map(|pair| format!("({} + {})", pair[0], pair[1]))
            .collect();
    }
    let expression = terms.pop().unwrap();
    let path = "columns.VALUE.aggregate";
    let plan = compile_aggregate(&expression, path).unwrap();
    assert_eq!(plan.reductions().len(), 512);
    for reduction in plan.reductions() {
        let AggregateOperand::Numeric(operand) = &reduction.operand else {
            panic!("numeric aggregate operand")
        };
        assert_eq!(operand.expression(), expression);
        assert_eq!(operand.spec_path(), path);
    }
    assert_eq!(expression.matches("SUM(").count(), 512);
}

#[test]
fn a_bare_identifier_outside_reduction_uses_numeric_coercion() {
    let plan = compile_aggregate("A", "columns.VALUE.aggregate").unwrap();
    let AggregateResult::Numeric(result) = plan.result() else {
        panic!("outer expressions use the numeric language")
    };
    assert_eq!(
        result
            .evaluate(&mut Resolver { calls: Vec::new() })
            .unwrap(),
        Number::Int(7)
    );
    assert_eq!(plan.ungrouped_identifiers().collect::<Vec<_>>(), ["A"]);
}
impl NumericResolver for Resolver {
    type Error = std::convert::Infallible;
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.calls.push(name.into());
        Ok(Selection::Present(Value::Int(match name {
            "EX.DOSE" => 3,
            "SUM(EX.DOSE * 2)" => 6,
            "MAX(EX.DOSE)" => 4,
            "A" => 7,
            _ => panic!("unbound aggregate resolver identity {name}"),
        })))
    }
}

#[test]
fn outer_and_per_record_arithmetic_share_the_numeric_evaluator_without_rewriting() {
    let expression = "POWER(SUM(EX.DOSE * 2), 2) + MAX(EX.DOSE)";
    let plan = compile_aggregate(expression, "columns.VALUE.derivation.aggregate").unwrap();
    assert_eq!(plan.identifiers().collect::<Vec<_>>(), ["EX.DOSE"]);
    let AggregateOperand::Numeric(operand) = &plan.reductions()[0].operand else {
        panic!()
    };
    let mut resolver = Resolver { calls: Vec::new() };
    assert_eq!(operand.evaluate(&mut resolver).unwrap(), Number::Int(6));
    assert_eq!(operand.expression(), expression);
    let AggregateResult::Numeric(result) = plan.result() else {
        panic!()
    };
    assert_eq!(result.evaluate(&mut resolver).unwrap(), Number::float(40.0));
    assert_eq!(
        resolver.calls,
        ["EX.DOSE", "SUM(EX.DOSE * 2)", "MAX(EX.DOSE)"]
    );
    assert_eq!(result.expression(), expression);
}

#[test]
fn bare_reducers_retain_typed_reads_and_record_stars() {
    let plan = compile_aggregate("(ONLY(EX.TEXT))", "rows[0].derivations.TEXT.aggregate").unwrap();
    assert_eq!(plan.result(), &AggregateResult::Reduction(0));
    assert_eq!(
        plan.reductions()[0].operand,
        AggregateOperand::Value("EX.TEXT".into())
    );
    let count = compile_aggregate("COUNT(EX.*)", "columns.N.aggregate").unwrap();
    assert_eq!(count.record_relations().collect::<Vec<_>>(), ["EX"]);
    assert_eq!(
        count.reductions()[0].operand,
        AggregateOperand::Records("EX".into())
    );
    let group = compile_aggregate("KEY + SUM(EX.DOSE)", "columns.X.aggregate").unwrap();
    assert_eq!(group.ungrouped_identifiers().collect::<Vec<_>>(), ["KEY"]);
}

#[test]
fn failures_preserve_full_authored_expression_and_inner_span() {
    let expression = "SUM(A / 0) + 1";
    let plan = compile_aggregate(expression, "columns.X.aggregate").unwrap();
    let AggregateOperand::Numeric(operand) = &plan.reductions()[0].operand else {
        panic!()
    };
    let error = operand
        .evaluate(&mut Resolver { calls: Vec::new() })
        .unwrap_err();
    assert_eq!(error.evaluation.location.expression, expression);
    assert_eq!(
        &expression[error.source_span.start..error.source_span.end],
        "A / 0"
    );
    assert_eq!(error.evaluation.location.spec_path, "columns.X.aggregate");
    let complete = compile_aggregate("SUM(A + B) + SUM(C)", "columns.X.aggregate").unwrap();
    assert_eq!(complete.reductions().len(), 2);
    assert_eq!(complete.identifiers().collect::<Vec<_>>(), ["A", "B", "C"]);
    assert!(matches!(
        compile_aggregate("SUM(MEAN(A))", "columns.X.aggregate"),
        Err(yamaa_core::aggregate_parser::ParseError::Grammar { .. })
    ));
}
