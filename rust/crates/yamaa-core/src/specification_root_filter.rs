//! A root predicate reads only the base driver, before key construction (REQ-1170).
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    predicate_compiler,
    value::Value,
};

#[derive(Debug)]
pub(super) struct RootFilter {
    expression: String,
    compiled: Result<crate::predicate::Plan, predicate_compiler::Error>,
}
impl RootFilter {
    pub(super) fn prepare(
        d: &Document,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Option<Self>, PrepareError> {
        let Some(id) = d
            .field(d.root(), "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            return Ok(None);
        };
        let expression = text(d, id)?;
        let compiled = predicate_compiler::compile(expression, "filter", Default::default());
        match &compiled {
            Ok(_)
            | Err(predicate_compiler::Error::Parse(
                crate::predicate_parser::ParseError::Grammar { .. },
            )) => (),
            Err(predicate_compiler::Error::Parse(
                crate::predicate_parser::ParseError::UnsupportedRegex { .. },
            )) => reject(extra, "predicate_regex", "filter".into()),
            Err(predicate_compiler::Error::Internal) => return Err(PrepareError::Internal),
            Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
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
            Err(predicate_compiler::Error::Parse(
                crate::predicate_parser::ParseError::Grammar {
                    position, failure, ..
                },
            )) => {
                findings.push(BindFinding::Lookup(LookupFinding::grammar(
                    "filter",
                    &self.expression,
                    position.character,
                    failure,
                )?));
                return Ok(None);
            }
            Err(error) => return Err(BindError::PredicatePolicy(error.clone())),
        };
        let start = findings.len();
        let mut seen = alloc::collections::BTreeSet::new();
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
                    .map(|(name, text)| {
                        (name.into(), ContextValue::Scalar(Value::Str(text.into())))
                    })
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
        BoundPredicate::new(plan.clone(), bindings)
            .map(Some)
            .map_err(BindError::InvalidPredicateBinding)
    }
}

impl RootFilter {
    pub(super) fn check_findings(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        static_checks::predicate(&self.compiled, &self.expression, "filter", findings)
    }
}
