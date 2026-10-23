//! Bridge named input selections into one completed record context.
//! The matching language remains owned by the shared match-value compiler.
use super::*;

/// The value sites through which the intermediate at `index` matches on
/// output column `column`: each `between` value followed into a further
/// intermediate, then the value that reads the column, a key's match value
/// or the reference of a nearest selection (LOOKUP-26).
fn match_sites(
    index: usize,
    column: usize,
    items: &[Option<crate::dataset::Intermediate>],
    depth: usize,
) -> Option<Vec<String>> {
    use crate::dataset::{CurrentRead, MatchRead};
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return None;
    }
    let item = items.get(index).and_then(Option::as_ref)?;
    let reference = item.nearest.iter().map(|nearest| &nearest.reference);
    if let Some(key) = item.keys.iter().chain(reference).find(|key| {
        key.value
            .reads()
            .any(|read| matches!(read, MatchRead::Output { column: read, .. } if *read == column))
    }) {
        return Some(vec![key.path.clone()]);
    }
    let range = item.range.as_ref()?;
    let value = format!("{}.value", range.path());
    match range.current().read {
        CurrentRead::Output(read) if read == column => Some(vec![value]),
        CurrentRead::Intermediate { index: child, .. } if child < index => {
            let mut sites = vec![value];
            sites.extend(match_sites(child, column, items, depth + 1)?);
            Some(sites)
        }
        _ => None,
    }
}

/// Whether `findings` already holds this lookup finding: a template reads
/// one intermediate through several identifiers of one site.
pub(super) fn reported(findings: &[BindFinding], finding: &BindFinding) -> bool {
    findings.iter().any(|prior| {
        matches!((prior, finding), (BindFinding::Lookup(prior), BindFinding::Lookup(finding))
            if prior.diagnostic() == finding.diagnostic())
    })
}

pub(super) fn dependencies(
    index: usize,
    items: &[Option<crate::dataset::Intermediate>],
) -> Result<Vec<usize>, BindError> {
    let mut columns = Vec::new();
    crate::dataset::visit_output_dependencies(
        index,
        |index| items.get(index).and_then(Option::as_ref),
        |column| columns.push(column),
    )
    .map_err(|_| BindError::Internal)?;
    columns.sort_unstable();
    columns.dedup();
    Ok(columns)
}

/// A row template reads a lookup that matches on a column only the column
/// phase completes (LOOKUP-2): a key match value, its `between` value, or a
/// match value of another lookup a `between` value reads. The finding is at
/// the template's read of the lookup.
pub(super) fn match_phase_boundary(
    path: &str,
    column: &str,
    intermediate: &str,
    row: &str,
) -> BindFinding {
    let mut finding = LookupFinding::new(
        path,
        crate::diagnostic::ConditionCode::IntermediateMatchPhaseBoundary,
    );
    finding.text("identifier", column);
    finding.text("intermediate", intermediate);
    finding.text("row", row);
    finding.text("available_phase", "column_derivation");
    finding.text("required_phase", "row_construction");
    BindFinding::Lookup(finding)
}

/// Bind one direct read using the same selector and actual completed outputs.
/// `row` names the row template that reads it during row construction.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    reference: &str,
    path: &str,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    items: &[Option<crate::dataset::Intermediate>],
    output: &TableSchema,
    catalog: &Catalog,
    row: Option<&str>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let Some((index, field)) = declarations.reference(reference) else {
        return Ok(None);
    };
    let Some(item) = items.get(index).and_then(Option::as_ref) else {
        findings.push(BindFinding::UnknownReference {
            path: path.into(),
            name: reference.into(),
        });
        return Ok(None);
    };
    if let Some(reader) = declarations.reader() {
        if item.source.is_self() {
            let mut finding = LookupFinding::new(
                path,
                crate::diagnostic::ConditionCode::IntermediateDerivationSelfPhase,
            );
            finding.text("intermediate", reader);
            finding.text("identifier", reference);
            findings.push(BindFinding::Lookup(finding));
            return Ok(None);
        }
    }
    let Some(column) = item.source_column(secondary, field) else {
        findings.push(BindFinding::Lookup(LookupFinding::reference(
            path,
            reference,
            None,
            ReferenceCause::Value,
            None,
        )));
        return Ok(None);
    };
    let missing_dependency = |identifier: &str, key_path: &str| {
        if let Some(reader) = declarations.reader() {
            let mut finding = LookupFinding::new(
                path,
                crate::diagnostic::ConditionCode::IntermediateDerivationDependency,
            );
            finding.text("intermediate", reader);
            finding.text("identifier", identifier);
            finding.text("read", &item.identifier);
            BindFinding::Lookup(finding)
        } else {
            BindFinding::Lookup(LookupFinding::reference(
                key_path,
                identifier,
                Some(&item.identifier),
                ReferenceCause::OutputKey,
                None,
            ))
        }
    };
    let before = findings.len();
    // A phase refusal that this site already reported still refuses the read.
    let mut phase_refused = false;
    // The reference of a nearest selection is read as a key's match value
    // is (LOOKUP-26).
    let reference = item.nearest.iter().map(|nearest| &nearest.reference);
    for key in item.keys.iter().chain(reference) {
        if let crate::dataset::MatchValue::Contextual(declaration) = &key.value {
            findings.push(missing_dependency(declaration.identifier(), &key.path));
            continue;
        }
        for read in key.value.reads() {
            if let crate::dataset::MatchRead::Output { column, identifier } = read {
                if available.is_some_and(|available| !available.contains(column)) {
                    // A row template's read of a lookup matching on a column
                    // it leaves to the column phase (LOOKUP-2), once per site.
                    match row.filter(|_| declarations.reader().is_none()) {
                        Some(row) => {
                            let finding =
                                match_phase_boundary(path, identifier, &item.identifier, row);
                            if !reported(findings, &finding) {
                                findings.push(finding);
                            }
                            phase_refused = true;
                        }
                        None => findings.push(missing_dependency(identifier, &key.path)),
                    }
                    continue;
                }
            }
            if let crate::dataset::MatchRead::SecondaryRecord { identifier, .. } = read {
                if catalog.bind(identifier).is_some_and(|read| {
                    matches!(read, reference_binding::Binding::Dataset { dataset: 0, .. })
                }) {
                    continue;
                }
                findings.push(missing_dependency(identifier, &key.path));
            }
        }
    }
    if findings.len() != before || phase_refused {
        return Ok(None);
    }
    let donor = item.source_schema(secondary).ok_or(BindError::Internal)?;
    // A row driver declares no key, so it is read only as the driver of its
    // rows, never as a keyed lookup (LOOKUP-3, ROW-2). Each such read reports
    // it: a read refused without a finding of its own would leave its
    // formula, predicate or filter to bind without it.
    if let Some(key) = declarations
        .key_required(index)
        .filter(|_| item.keys.is_empty())
    {
        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
            crate::diagnostic::lookup::key_required(key),
        )));
        return Ok(None);
    }
    for key in &item.keys {
        let read_kind = key
            .value
            .read()
            .and_then(|read| catalog.type_of(read.identifier()));
        let field = donor
            .columns()
            .get(key.source_column)
            .ok_or(BindError::Internal)?;
        let kind = key.value.result_type(read_kind);
        if let Some(donor_kind) = item.source_kind(secondary, key.source_column) {
            if !kind.comparable(donor_kind.into()) {
                let name = if key.value.is_expression() {
                    format!("key[{}]", field.name)
                } else {
                    key.value
                        .read()
                        .map_or(field.name.clone(), |read| read.identifier().into())
                };
                findings.push(BindFinding::Lookup(LookupFinding::key_type(
                    &format!("{}.key", item.path),
                    &item.identifier,
                    &name,
                    kind,
                    donor_kind,
                )));
            }
        }
    }
    if findings.len() != before {
        return Ok(None);
    }
    for dependency in dependencies(index, items)? {
        let name = &output.columns()[dependency].name;
        if let Some(finding) = catalog
            .validate_output(name, None, available, &[0])
            .map_err(BindError::Catalog)?
        {
            // A lookup's own dependency is a match value it reads, not a read
            // by this row derivation: one the row template leaves to the
            // column phase fails at the template's read (LOOKUP-2), naming
            // each value followed when a `between` value leads to it.
            if let (Some(row), crate::reference_binding::Diagnostic::PhaseBoundary { .. }) =
                (row, &finding)
            {
                let BindFinding::Lookup(mut finding) =
                    match_phase_boundary(path, name, &item.identifier, row)
                else {
                    return Err(BindError::Internal);
                };
                if let Some(sites) =
                    match_sites(index, dependency, items, 0).filter(|sites| sites.len() > 1)
                {
                    finding.extend_paths(sites);
                }
                let finding = BindFinding::Lookup(finding);
                if !reported(findings, &finding) {
                    findings.push(finding);
                }
                phase_refused = true;
                continue;
            }
            findings.push(BindFinding::OutputReference {
                path: path.into(),
                name: name.clone(),
                row: None,
                finding,
            });
        } else {
            edges.push(dependency);
        }
    }
    Ok((findings.len() == before && !phase_refused)
        .then_some(Expression::Intermediate { index, column }))
}
