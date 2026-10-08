//! Source eligibility is compiled in core and bound after actual source ingestion.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{ConditionCode as C, ContextValue, Diagnostic},
    predicate_compiler,
    value::Value,
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub variable: String,
    filter: Option<Filter>,
}
#[derive(Clone, Debug)]
struct Filter {
    expression: String,
    compiled: Option<Result<crate::predicate::Plan, predicate_compiler::Error>>,
}
#[derive(Debug)]
pub struct SourceFinding(Diagnostic);
impl SourceFinding {
    pub fn diagnostic(&self) -> Diagnostic {
        self.0.clone()
    }
    pub fn definition(&self) -> crate::diagnostic::Definition {
        self.0.definition()
    }
    fn new(code: C, path: &str, fields: &[(&str, &str)]) -> Self {
        Self(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: fields
                .iter()
                .map(|(name, value)| {
                    (
                        (*name).into(),
                        ContextValue::Scalar(Value::Str((*value).into())),
                    )
                })
                .collect(),
            source_span: None,
            operand_route: None,
        })
    }
}
impl Declaration {
    pub fn prepare(
        d: &Document,
        payload: usize,
        path: &str,
        extra: &mut Vec<UnsupportedFeature>,
        intermediates: &intermediates::Declarations,
    ) -> Result<Self, PrepareError> {
        if let N::Text(variable) = &d.nodes()[payload] {
            return Ok(Self {
                variable: variable.clone(),
                filter: None,
            });
        }
        for &(name, _) in mapping(d, payload)? {
            let name = text(d, name)?;
            if !["variable", "filter"].contains(&name) {
                reject(extra, name, format!("{path}.{name}"));
            }
        }
        let variable: String = text(d, field(d, payload, "variable")?)?.into();
        let filter = if let Some(id) = d
            .field(payload, "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let expression = text(d, id)?;
            let compiled = if variable
                .split_once('.')
                .is_some_and(|(name, _)| !intermediates.contains(name))
            {
                Some(predicate_compiler::compile(
                    expression,
                    &format!("{path}.filter"),
                    Default::default(),
                ))
            } else {
                None
            };
            match compiled.as_ref() {
                Some(Ok(_))
                | Some(Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::Grammar { .. },
                )))
                | None => {}
                Some(Err(predicate_compiler::Error::UnsupportedLiteral { .. })) => {
                    reject(extra, "predicate_literal", format!("{path}.filter"))
                }
                Some(Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::UnsupportedRegex { .. },
                ))) => reject(extra, "predicate_regex", format!("{path}.filter")),
                Some(Err(predicate_compiler::Error::Internal)) => {
                    return Err(PrepareError::Internal)
                }
                Some(Err(_)) => return Err(PrepareError::Limit("predicate_compilation")),
            }
            Some(Filter {
                expression: expression.into(),
                compiled,
            })
        } else {
            None
        };
        Ok(Self { variable, filter })
    }
    /// Scope checks precede source field resolution. Every occurrence is admitted,
    /// including identifiers behind a short-circuiting predicate branch.
    pub fn bind_filter(
        &self,
        path: &str,
        source: &SourceDeclaration,
        schema: &TableSchema,
        intermediates: &intermediates::Declarations,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<BoundPredicate>, BindError> {
        let Some(Filter {
            expression,
            compiled,
        }) = &self.filter
        else {
            return Ok(None);
        };
        let path = format!("{path}.filter");
        let Some((dataset, _)) = self
            .variable
            .split_once('.')
            .filter(|(name, _)| !intermediates.contains(name))
        else {
            findings.push(BindFinding::Source(SourceFinding::new(
                C::SourceFilterSingleValue,
                &path,
                &[("identifier", &self.variable)],
            )));
            return Ok(None);
        };
        let plan = match compiled.as_ref().ok_or(BindError::Internal)? {
            Ok(plan) => plan,
            Err(predicate_compiler::Error::Parse(
                crate::predicate_parser::ParseError::Grammar {
                    position, failure, ..
                },
            )) => {
                findings.push(BindFinding::Source(SourceFinding(failure.diagnostic(
                    &path,
                    expression,
                    position.character,
                ))));
                return Ok(None);
            }
            Err(error) => return Err(BindError::PredicatePolicy(error.clone())),
        };
        let start = findings.len();
        let mut bindings = Vec::new();
        let mut seen = alloc::collections::BTreeSet::new();
        for name in plan.identifiers() {
            if !seen.insert(name) {
                continue;
            }
            let reference = name.split_once('.');
            if reference.is_none_or(|(name, _)| name != dataset) {
                findings.push(BindFinding::Source(SourceFinding::new(
                    C::SourceFilterReference,
                    &path,
                    &[("identifier", name), ("dataset", dataset)],
                )));
                continue;
            }
            // This closed slice only admits a driver-backed source expression.
            if dataset != source.name {
                return Err(BindError::Internal);
            }
            let field = reference.expect("qualified filter reference").1;
            if let Some(column) = schema
                .columns()
                .iter()
                .position(|column| column.name == field)
            {
                if !bindings
                    .iter()
                    .any(|binding: &Binding| binding.name == name)
                {
                    bindings.push(Binding {
                        name: name.into(),
                        read: Read::Source(column),
                    });
                }
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: path.clone(),
                    name: name.into(),
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
    pub fn has_filter(&self) -> bool {
        self.filter.is_some()
    }
}

#[derive(Clone, Debug)]
pub(super) struct FirstAvailable {
    pub sources: Vec<Declaration>,
    pub missing: Value,
}
impl FirstAvailable {
    pub fn prepare(
        d: &Document,
        payload: usize,
        path: &str,
        extra: &mut Vec<UnsupportedFeature>,
        intermediates: &intermediates::Declarations,
    ) -> Result<Self, PrepareError> {
        for &(name, _) in mapping(d, payload)? {
            let name = text(d, name)?;
            if !["sources", "missing"].contains(&name) {
                reject(extra, name, format!("{path}.{name}"));
            }
        }
        let mut sources = Vec::new();
        for (index, &operand) in sequence(d, field(d, payload, "sources")?)?
            .iter()
            .enumerate()
        {
            sources.push(Declaration::prepare(
                d,
                operand,
                &format!("{path}.sources[{index}]"),
                extra,
                intermediates,
            )?);
        }
        let missing = d
            .field(payload, "missing")
            .map(|id| literal(d, id, &format!("{path}.missing")))
            .transpose()?
            .unwrap_or(Value::Missing);
        Ok(Self { sources, missing })
    }
}
