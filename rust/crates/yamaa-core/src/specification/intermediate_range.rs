//! Pure range/projection binding; donor data and native effects remain in execution.
use super::*;
use crate::{
    dataset::{CurrentRead, RelationRange},
    diagnostic::{lookup::Owner, ConditionCode as C},
};
/// A named intermediate's range (LOOKUP-9), refused at preparation when it
/// states neither bound.
pub(super) fn prepare(
    spec: &ast::Spec,
    item: &ast::Intermediate,
) -> Result<Option<lookup_range::Declaration>, PrepareError> {
    let Some(between) = &item.between else {
        return Ok(None);
    };
    let declaration = lookup_range::Declaration::from_tree(spec, between)?;
    let intermediate = item.id.name.as_str();
    if let Some(finding) = declaration.bounds_finding(Owner::Intermediate(intermediate)) {
        return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
            finding,
        )]));
    }
    Ok(Some(declaration))
}
fn reference(path: &str, name: &str, intermediate: &str, code: C, findings: &mut Vec<BindFinding>) {
    let mut finding = LookupFinding::new(path, code);
    finding.text("intermediate", intermediate);
    finding.text("identifier", name);
    findings.push(BindFinding::Lookup(finding));
}
/// Bind a named intermediate's range. The value names a value of the row that
/// reads the intermediate; an input's record keeps that input's record port,
/// and every context that reads the intermediate admits it (LOOKUP-9). A bare
/// name is an output column alone, so one that no column takes fails at
/// `between.value`, whatever the bounds bind; when another intermediate's
/// derivations read this one (`contextual`), it stays contextual instead: a
/// donor read takes the donor record's field of that name (LOOKUP-16), as
/// does a qualified value its input does not store; every row that reads an
/// unresolved value refuses it ([`lookup_range::admit`]).
pub(super) fn bind(
    declaration: &lookup_range::Declaration,
    relation: &intermediates::Donor<'_>,
    catalog: &Catalog,
    contextual: bool,
    donors: binder::Donors<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationRange>, BindError> {
    let intermediates::Donor {
        lookup: intermediate,
        schema: donor,
        kind: donor_kind,
        admits,
        ..
    } = *relation;
    let owner = Owner::Intermediate(intermediate);
    let wrap = |diagnostic| BindFinding::Lookup(LookupFinding::from_diagnostic(diagnostic));
    let mut diagnostics = Vec::new();
    // All stated donor bounds precede current-reference validation.
    let bounds = declaration.bind_bounds(owner, donor, donor_kind, admits, &mut diagnostics);
    findings.extend(diagnostics.into_iter().map(wrap));
    let unread = declaration.names_no_column(catalog);
    let Some(bounds) = bounds else {
        if unread && !contextual {
            findings.push(wrap(declaration.unknown_value(owner)));
        }
        return Ok(None);
    };
    if unread && contextual {
        return Ok(
            match declaration.finish(CurrentRead::Contextual, None, bounds, owner)? {
                Ok(range) => Some(range),
                Err(diagnostic) => {
                    findings.push(wrap(diagnostic));
                    None
                }
            },
        );
    }
    let current = declaration
        .current_value(catalog, donors, None, None, &mut Vec::new())?
        .or_else(|| contextual.then_some((CurrentRead::Contextual, None)));
    let Some((read, kind)) = current else {
        findings.push(wrap(declaration.unknown_value(owner)));
        return Ok(None);
    };
    Ok(match declaration.finish(read, kind, bounds, owner)? {
        Ok(range) => Some(range),
        Err(diagnostic) => {
            findings.push(wrap(diagnostic));
            None
        }
    })
}
pub(super) fn prepare_columns(
    item: &ast::Intermediate,
) -> Result<Option<Vec<String>>, PrepareError> {
    item.columns
        .as_ref()
        .map(|columns| {
            if columns.len() > crate::limits::Limits::COLUMNS {
                return Err(PrepareError::limit(
                    "intermediate_columns",
                    crate::limits::Limits::COLUMNS,
                    Some(columns.len()),
                    None,
                ));
            }
            Ok(columns.iter().map(|column| column.name.clone()).collect())
        })
        .transpose()
}
pub(super) fn bind_columns(
    names: &Option<Vec<String>>,
    donor_name: &str,
    schema: &TableSchema,
    admits: &impl Fn(usize) -> bool,
    path: &str,
    intermediate: &str,
    findings: &mut Vec<BindFinding>,
) -> Option<Vec<usize>> {
    names.as_ref().map(|names| {
        let mut columns = names
            .iter()
            .filter_map(|name| {
                if let Some(column) = schema
                    .columns()
                    .iter()
                    .position(|field| field.name == *name)
                    .filter(|&column| admits(column))
                {
                    Some(column)
                } else {
                    reference(
                        &format!("{path}.columns"),
                        &format!("{donor_name}.{name}"),
                        intermediate,
                        C::IntermediateColumnReference,
                        findings,
                    );
                    None
                }
            })
            .collect::<Vec<_>>();
        // A repeated readable name expresses the same restriction once.
        columns.sort_unstable();
        columns.dedup();
        columns
    })
}
