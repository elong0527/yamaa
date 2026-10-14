//! Types of retained predicate operands, without resolution or scalar evaluation.
use super::{Condition, Node, Plan, Route, Scalar};
use crate::value::ValueType;
use alloc::vec::Vec;

impl Plan {
    pub(crate) fn type_conditions(
        &self,
        kind: impl Fn(&str) -> Option<ValueType>,
    ) -> Vec<(Condition, Vec<Route>)> {
        fn scalar(value: &Scalar, kind: &impl Fn(&str) -> Option<ValueType>) -> Option<ValueType> {
            match value {
                Scalar::Literal(value) => value.value_type(),
                Scalar::Identifier(name) => kind(name),
            }
        }
        fn pair(
            left: &Scalar,
            right: &Scalar,
            kind: &impl Fn(&str) -> Option<ValueType>,
        ) -> Option<Condition> {
            let (left, right) = (scalar(left, kind)?, scalar(right, kind)?);
            (!crate::value::comparable_types(left, right))
                .then_some(Condition::IncompatiblePair { left, right })
        }
        fn visit(
            plan: &Plan,
            index: usize,
            kind: &impl Fn(&str) -> Option<ValueType>,
            route: &mut Vec<Route>,
            findings: &mut Vec<(Condition, Vec<Route>)>,
        ) {
            let mut add = |condition: Option<Condition>| {
                if let Some(condition) = condition {
                    findings.push((condition, route.clone()));
                }
            };
            match &plan.nodes[index] {
                Node::Not(child) => {
                    route.push(Route::Child);
                    visit(plan, *child, kind, route, findings);
                    route.pop();
                }
                Node::And(left, right) | Node::Or(left, right) => {
                    route.push(Route::Left);
                    visit(plan, *left, kind, route, findings);
                    route.pop();
                    route.push(Route::Right);
                    visit(plan, *right, kind, route, findings);
                    route.pop();
                }
                Node::Compare { left, right, .. } => add(pair(left, right, kind)),
                Node::In { value, items, .. } => {
                    for item in items {
                        add(pair(value, item, kind));
                    }
                }
                Node::Between {
                    value,
                    lower,
                    upper,
                    ..
                } => {
                    add(pair(value, lower, kind));
                    add(pair(value, upper, kind));
                }
                Node::Contains { value, .. } => add(scalar(value, kind)
                    .filter(|actual| *actual != ValueType::Str)
                    .map(|actual| Condition::ContainsExpectedText { actual })),
                Node::Like { value, pattern, .. } => {
                    for value in [value, pattern] {
                        add(scalar(value, kind)
                            .filter(|actual| *actual != ValueType::Str)
                            .map(|actual| Condition::ExpectedText { actual }));
                    }
                }
                Node::Boolean(_) | Node::IsNull { .. } => {}
            }
        }
        let mut findings = Vec::new();
        visit(self, self.root, &kind, &mut Vec::new(), &mut findings);
        findings
    }
}
