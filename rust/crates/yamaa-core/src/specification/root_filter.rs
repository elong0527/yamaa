//! A root predicate reads only the base driver, before key construction (REQ-1170).
use super::*;
use crate::json::Value as JsonValue;
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{ConditionCode, Diagnostic},
};

#[derive(Debug)]
pub(super) struct RootFilter {
    expression: String,
    compiled: Result<CompiledExpression, CompileError>,
}
impl RootFilter {
    pub(super) fn prepare(d: &Document) -> Result<Option<Self>, PrepareError> {
        let Some(id) = d
            .field(d.root(), "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            return Ok(None);
        };
        let expression = text(d, id)?;
        let compiled = compile(expression, ExpressionContext::Predicate, "filter");
        if let Err(error @ CompileError::Limit { .. }) = &compiled {
            return Err(PrepareError::expression(error.clone(), Some("filter")));
        }
        Ok(Some(Self {
            expression: expression.into(),
            compiled,
        }))
    }
    pub(super) fn bind(
        &self,
        driver: &str,
        source: &TableSchema,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<BoundPredicate>, BindError> {
        let plan = match &self.compiled {
            Ok(plan) => plan,
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Lookup(LookupFinding::grammar(
                    "filter",
                    &self.expression,
                    position.character,
                    failure,
                )));
                return Ok(None);
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
        };
        let start = findings.len();
        let mut seen = std::collections::BTreeSet::new();
        let mut bindings = Vec::new();
        for identifier in plan.identifiers() {
            if !seen.insert(identifier) {
                continue;
            }
            let Some((dataset, field)) = identifier.split_once('.') else {
                findings.push(BindFinding::Predicate(Diagnostic {
                    code: ConditionCode::RootFilterPhaseBoundary,
                    spec_paths: vec!["filter".into()],
                    context: [
                        ("identifier", identifier),
                        ("available_phase", "column_derivation"),
                        ("required_phase", "row_filter"),
                    ]
                    .into_iter()
                    .map(|(name, text)| (name.into(), JsonValue::String(text.into())))
                    .collect(),
                    source_span: None,
                    operand_route: None,
                }));
                continue;
            };
            let selected = (dataset == driver)
                .then(|| {
                    source
                        .columns()
                        .iter()
                        .position(|column| column.name == field)
                })
                .flatten();
            if let Some(column) = selected {
                bindings.push(Binding {
                    name: identifier.into(),
                    read: Read::Source(column),
                });
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: "filter".into(),
                    name: identifier.into(),
                });
            }
        }
        if findings.len() != start {
            return Ok(None);
        }
        let typed = plan.typed(|name| {
            crate::result_type::ResultType::declared(
                name.split_once('.')
                    .filter(|(dataset, _)| *dataset == driver)
                    .and_then(|(_, field)| {
                        source.columns().iter().find(|column| column.name == field)
                    })
                    .map(|column| column.kind),
            )
        });
        BoundPredicate::new(typed, bindings)
            .map(Some)
            .map_err(BindError::InvalidPredicateBinding)
    }
}

impl RootFilter {
    pub(super) fn check_findings(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        static_checks::predicate(&self.compiled, &self.expression, "filter", findings)
    }
}
