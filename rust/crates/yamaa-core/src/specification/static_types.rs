//! Static operand kinds from captured schemas; no cells or resolver calls.
use super::*;
use crate::{
    dataset::{Intermediate, SecondarySource},
    diagnostic::Diagnostic,
    evaluation::NumericCondition,
    predicate::Plan,
    value::ValueType,
};

pub(super) fn numeric_kinds(
    plan: &CompiledNumeric,
    kind: impl Fn(&str) -> Option<ValueType>,
) -> Vec<Diagnostic> {
    let mut findings = Vec::new();
    for name in plan.identifiers() {
        if let Some(actual) =
            kind(name).filter(|actual| !matches!(actual, ValueType::Int | ValueType::Float))
        {
            findings.push(
                NumericCondition::IncompatibleInput {
                    identifier: name.into(),
                    actual,
                }
                .static_diagnostic(plan.spec_path(), plan.expression()),
            );
        }
    }
    findings
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
    let mut findings = Vec::new();
    for reduction in plan.reductions() {
        if let AggregateOperand::Numeric(numeric) = &reduction.operand {
            findings.extend(numeric_kinds(numeric, &kind));
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
                findings.push(
                    crate::diagnostic::DatasetFinding::ReductionInputType {
                        path: plan.spec_path(),
                        expression: plan.expression(),
                        reducer,
                        source: name,
                        actual,
                    }
                    .diagnostic()
                    .map_err(|_| BindError::InvalidDiagnostic)?,
                );
            }
        }
    }
    if let AggregateResult::Numeric(numeric) = plan.result() {
        findings.extend(numeric_kinds(numeric, kind));
    }
    Ok(findings)
}

pub(super) fn predicate_kinds(
    plan: &Plan,
    kind: impl Fn(&str) -> Option<ValueType>,
) -> Vec<Diagnostic> {
    plan.type_conditions(kind)
        .into_iter()
        .map(|(condition, route)| condition.static_diagnostic(plan.spec_path(), &route))
        .collect()
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
            .and_then(|item| {
                let column = item.source_column(secondary, field)?;
                item.source_kind(secondary, column).map(Into::into)
            }));
    }
    Ok(catalog.type_of(name).map(Into::into))
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
        numeric_kinds(plan, |name| kinds.get(name).copied().flatten())
            .into_iter()
            .map(BindFinding::Scalar),
    );
    Ok(())
}

#[cfg(test)]
mod tests;
