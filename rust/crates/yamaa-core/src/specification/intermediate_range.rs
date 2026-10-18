//! Pure range/projection binding; donor data and native effects remain in execution.
use super::*;
use crate::{
    dataset::{CurrentRead, Intermediate, RelationRange},
    diagnostic::{lookup::Owner, ConditionCode as C},
};
/// A named intermediate's range (LOOKUP-9), refused at preparation when it
/// states neither bound.
pub(super) fn prepare(
    d: &Document,
    node: usize,
    path: &str,
    intermediate: &str,
) -> Result<Option<lookup_range::Declaration>, PrepareError> {
    let Some(declaration) = lookup_range::Declaration::prepare(d, node, path)? else {
        return Ok(None);
    };
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
/// reads the intermediate, or, in a donor derivation, a bare field of the donor
/// record; an input's record keeps that input's record port, and every context
/// that reads the intermediate admits it (LOOKUP-9).
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    declaration: &lookup_range::Declaration,
    intermediate: &str,
    donor: &TableSchema,
    donor_kind: &impl Fn(usize) -> Option<ColumnType>,
    admits: &impl Fn(usize) -> bool,
    catalog: &Catalog,
    driver: &TableSchema,
    previous: &[Option<Intermediate>],
    secondary: &[crate::dataset::SecondarySource],
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationRange>, BindError> {
    let owner = Owner::Intermediate(intermediate);
    let wrap = |diagnostic| BindFinding::Lookup(LookupFinding::from_diagnostic(diagnostic));
    let mut diagnostics = Vec::new();
    // All stated donor bounds precede current-reference validation.
    let bounds = declaration.bind_bounds(owner, donor, donor_kind, admits, &mut diagnostics);
    findings.extend(diagnostics.into_iter().map(wrap));
    let Some(bounds) = bounds else {
        return Ok(None);
    };
    let name = declaration.value.as_str();
    let current = declaration
        .current_value(catalog, secondary, previous, None, None, &mut Vec::new())?
        .or_else(|| {
            // A bare name that no output column takes is a donor record field.
            (!name.contains('.'))
                .then(|| {
                    driver
                        .columns()
                        .iter()
                        .position(|column| column.name == name)
                })
                .flatten()
                .map(|column| {
                    (
                        CurrentRead::Record { source: 0, column },
                        Some(driver.columns()[column].kind.into()),
                    )
                })
        });
    let Some((read, kind)) = current else {
        findings.push(wrap(declaration.unknown_value(owner)));
        return Ok(None);
    };
    Ok(
        match declaration.finish(read, kind, bounds, Some(intermediate))? {
            Ok(range) => Some(range),
            Err(diagnostic) => {
                findings.push(wrap(diagnostic));
                None
            }
        },
    )
}
pub(super) fn prepare_columns(
    d: &Document,
    item: usize,
) -> Result<Option<Vec<String>>, PrepareError> {
    d.field(item, "columns")
        .filter(|&node| !matches!(d.nodes()[node], N::Null))
        .map(|node| {
            let fields = sequence(d, node)?;
            if fields.len() > crate::limits::Limits::COLUMNS {
                return Err(PrepareError::limit(
                    "intermediate_columns",
                    crate::limits::Limits::COLUMNS,
                    Some(fields.len()),
                    None,
                ));
            }
            fields
                .iter()
                .map(|&node| text(d, node).map(String::from))
                .collect()
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
