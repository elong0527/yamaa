//! Static operand kinds from captured schemas; no cells or resolver calls.
use super::*;
use crate::{
    dataset::{Intermediate, SecondarySource},
    diagnostic::Diagnostic,
    value::ValueType,
};

/// Every static type fault of a numeric expression or predicate.
fn findings(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    collected_sources: bool,
) -> Result<Vec<Diagnostic>, BindError> {
    plan.type_findings(kind, collected_sources)
        .iter()
        .map(|finding| {
            finding
                .diagnostic(plan.spec_path(), plan.expression())
                .map_err(|_| BindError::InvalidDiagnostic)
        })
        .collect()
}

pub(super) fn numeric_kinds(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
) -> Vec<Diagnostic> {
    findings(plan, kind, false).expect("numeric findings project")
}

pub(super) fn aggregate_kinds(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    source_arguments: bool,
) -> Result<Vec<Diagnostic>, BindError> {
    findings(plan, kind, source_arguments)
}

pub(super) fn predicate_kinds(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
) -> Vec<Diagnostic> {
    findings(plan, kind, false).expect("predicate findings project")
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
    plan: &CompiledExpression,
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
