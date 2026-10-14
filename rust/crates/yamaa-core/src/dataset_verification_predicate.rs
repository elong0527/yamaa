//! Completed-row predicates retain the declared selector and template scope checks.
use super::*;

pub(super) fn validate(
    predicate: &BoundPredicate,
    primary: &TableSchema,
    secondary: &[SecondarySource],
    items: &[Intermediate],
    output: &TableSchema,
    templates: &[RowTemplate],
) -> Result<(), PlanError> {
    let available = vec![true; output.columns().len()];
    if !predicate.bindings().iter().any(|binding| {
        matches!(
            binding.read,
            crate::bound_expression::Read::Intermediate { .. }
        )
    }) {
        return predicate
            .validate(0, &available, true)
            .map_err(PlanError::Predicate);
    }
    predicate
        .validate_contextual(0, &available)
        .map_err(PlanError::Predicate)?;
    if !predicate.collected().is_empty() {
        return Err(PlanError::InvalidSource);
    }
    for binding in predicate.bindings() {
        match binding.read {
            crate::bound_expression::Read::Intermediate { index, column } => {
                let item = items.get(index).ok_or(PlanError::InvalidIntermediate)?;
                for template in templates {
                    intermediates::validate_context_read(
                        item,
                        column,
                        primary,
                        secondary,
                        &available,
                        output,
                        &template.mode,
                    )?;
                    relation_predicates::validate_named_read(
                        index,
                        items,
                        template.mode.source_index(),
                        &template.mode,
                        &available,
                        0,
                    )?;
                }
            }
            crate::bound_expression::Read::Column(_) => {}
            _ => return Err(PlanError::InvalidSource),
        }
    }
    Ok(())
}
