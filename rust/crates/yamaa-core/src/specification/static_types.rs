//! Static operand kinds from captured schemas; no cells or resolver calls.
use super::*;
use crate::{
    dataset::{Intermediate, SecondarySource},
    diagnostic::{Diagnostic, ProjectionBudget},
    evaluation::NumericCondition,
    predicate::Plan,
    value::ValueType,
};

fn budget(path: &str, expression: &str) -> Result<ProjectionBudget, BindError> {
    let mut budget = ProjectionBudget::new(crate::diagnostic::MAX_PROJECTION_BYTES)
        .map_err(|_| BindError::PredicateDiagnosticLimit)?;
    budget
        .text(path)
        .map_err(|_| BindError::PredicateDiagnosticLimit)?;
    budget
        .text(expression)
        .map_err(|_| BindError::PredicateDiagnosticLimit)?;
    Ok(budget)
}

pub(super) fn numeric_kinds(
    plan: &CompiledNumeric,
    kind: impl Fn(&str) -> Option<ValueType>,
) -> Result<Vec<Diagnostic>, BindError> {
    let mut budget = budget(plan.spec_path(), plan.expression())?;
    numeric_kinds_with_budget(plan, kind, &mut budget)
}

fn numeric_kinds_with_budget(
    plan: &CompiledNumeric,
    kind: impl Fn(&str) -> Option<ValueType>,
    budget: &mut ProjectionBudget,
) -> Result<Vec<Diagnostic>, BindError> {
    let mut findings = Vec::new();
    for name in plan.identifiers() {
        if let Some(actual) =
            kind(name).filter(|actual| !matches!(actual, ValueType::Int | ValueType::Float))
        {
            for text in [plan.spec_path(), plan.expression(), name] {
                budget
                    .text(text)
                    .map_err(|_| BindError::PredicateDiagnosticLimit)?;
            }
            budget
                .charge(1024)
                .map_err(|_| BindError::PredicateDiagnosticLimit)?;
            findings.push(
                NumericCondition::IncompatibleInput {
                    identifier: name.into(),
                    actual,
                }
                .static_diagnostic(plan.spec_path(), plan.expression()),
            );
        }
    }
    Ok(findings)
}

pub(super) fn aggregate_kinds(
    plan: &crate::aggregate_compiler::CompiledAggregate,
    kind: impl Fn(&str) -> Option<ValueType>,
    source_arguments: bool,
) -> Result<Vec<Diagnostic>, BindError> {
    use crate::{
        aggregate_compiler::{AggregateOperand, AggregateResult},
        aggregate_parser::Reducer,
        reduction::NumericReducer,
    };
    let mut budget = budget(plan.spec_path(), plan.expression())?;
    let mut findings = Vec::new();
    for reduction in plan.reductions() {
        if let AggregateOperand::Numeric(numeric) = &reduction.operand {
            findings.extend(numeric_kinds_with_budget(numeric, &kind, &mut budget)?);
        }
        let reducer = match reduction.reducer {
            Reducer::Sum => NumericReducer::Sum,
            Reducer::Mean => NumericReducer::Mean,
            _ => continue,
        };
        if let AggregateOperand::Value(name) = &reduction.operand {
            // A grouped source reduction checks the collected present values.
            // Its storage kind does not make missing cells numeric arguments.
            if source_arguments && name.contains('.') {
                continue;
            }
            if let Some(actual) =
                kind(name).filter(|actual| !matches!(actual, ValueType::Int | ValueType::Float))
            {
                for text in [plan.spec_path(), plan.expression(), name] {
                    budget
                        .text(text)
                        .map_err(|_| BindError::PredicateDiagnosticLimit)?;
                }
                budget
                    .charge(1024)
                    .map_err(|_| BindError::PredicateDiagnosticLimit)?;
                findings.push(
                    crate::diagnostic::DatasetFinding::ReductionInputType {
                        path: plan.spec_path(),
                        expression: plan.expression(),
                        reducer,
                        source: name,
                        actual,
                    }
                    .diagnostic()
                    .ok_or(BindError::Internal)?,
                );
            }
        }
    }
    if let AggregateResult::Numeric(numeric) = plan.result() {
        findings.extend(numeric_kinds_with_budget(numeric, kind, &mut budget)?);
    }
    Ok(findings)
}

pub(super) fn predicate_kinds(
    plan: &Plan,
    kind: impl Fn(&str) -> Option<ValueType>,
) -> Result<Vec<Diagnostic>, BindError> {
    let mut budget = budget(plan.spec_path(), plan.expression())?;
    let mut findings = Vec::new();
    for (condition, route) in plan.type_conditions(kind) {
        budget
            .text(plan.spec_path())
            .map_err(|_| BindError::PredicateDiagnosticLimit)?;
        budget
            .charge(1024 + route.len().saturating_mul(512))
            .map_err(|_| BindError::PredicateDiagnosticLimit)?;
        findings.push(
            condition
                .static_diagnostic_with_limit(
                    plan.spec_path(),
                    &route,
                    crate::diagnostic::MAX_PREDICATE_DIAGNOSTIC_BYTES,
                )
                .map_err(|_| BindError::PredicateDiagnosticLimit)?,
        );
    }
    Ok(findings)
}

pub(super) fn kind(
    name: &str,
    named: &intermediates::Declarations,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
) -> Result<Option<ValueType>, BindError> {
    if let Some((index, field)) = named.reference(name) {
        return Ok(intermediates
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|item| item.source_schema(secondary))
            .and_then(|schema| schema.columns().iter().find(|column| column.name == field))
            .map(|column| column.kind.into()));
    }
    if let Some((relation, field)) = name.split_once('.') {
        if let Some(joined) = catalog.joined(relation) {
            return Ok(joined.fields.get(field).map(|(_, kind)| (*kind).into()));
        }
    }
    Ok(catalog
        .bind(name)
        .map_err(BindError::Catalog)?
        .map(|binding| match binding {
            reference_binding::Binding::Output { column_type, .. }
            | reference_binding::Binding::Dataset { column_type, .. } => column_type.into(),
        }))
}

pub(super) fn numeric(
    plan: &CompiledNumeric,
    bindings: &[Binding],
    named: &intermediates::Declarations,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    let kinds = plan
        .identifiers()
        .filter(|name| bindings.iter().any(|binding| binding.name == *name))
        .map(|name| kind(name, named, catalog, secondary, intermediates).map(|kind| (name, kind)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    findings.extend(
        numeric_kinds(plan, |name| kinds.get(name).copied().flatten())?
            .into_iter()
            .map(BindFinding::Scalar),
    );
    Ok(())
}

#[cfg(test)]
mod tests;
