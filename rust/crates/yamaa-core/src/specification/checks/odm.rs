//! ODM reads: the inputs they read, and their scopes and filters, which
//! depend only on the specification (ODM-*).
use super::*;
use crate::diagnostic::{ConditionCode as C, OdmFinding, OdmLocation};
use std::collections::BTreeSet;

/// Every derivation root, each with where its reads are: column defaults,
/// then row overrides, then intermediate derivations.
fn derivation_sites(spec: &Spec) -> Vec<(ExprId, OdmLocation, Option<usize>, Option<&str>)> {
    let mut sites = Vec::new();
    for column in &spec.columns {
        if let Some(derivation) = &column.derivation {
            sites.push((
                derivation.expr,
                OdmLocation::Column,
                None,
                Some(column.name.name.as_str()),
            ));
        }
    }
    for (index, template) in spec.rows.templates.iter().enumerate() {
        for (_, derivation) in &template.overrides {
            sites.push((derivation.expr, OdmLocation::Row, Some(index), None));
        }
    }
    for (index, item) in spec.intermediates.iter().enumerate() {
        for binding in &item.bindings {
            sites.push((
                binding.derivation.expr,
                OdmLocation::Intermediate,
                Some(index),
                None,
            ));
        }
    }
    sites
}

/// The ODM reads below `root` in written order, each with whether it is
/// read inside an aggregate.
fn reads(spec: &Spec, root: ExprId) -> Vec<(ExprId, bool)> {
    let mut found = Vec::new();
    let mut stack = vec![(root, false)];
    while let Some((e, derive)) = stack.pop() {
        if matches!(spec.expr(e).kind, ExprKind::Odm(_)) {
            found.push((e, derive));
            continue;
        }
        let derive = derive || matches!(spec.expr(e).kind, ExprKind::Aggregate(_));
        let children = spec.children(e).collect::<Vec<_>>();
        stack.extend(children.into_iter().rev().map(|(_, child)| (child, derive)));
    }
    found
}

/// Whether a derivation reads ODM items anywhere below `root`.
pub(in crate::specification) fn reads_odm(spec: &Spec, root: ExprId) -> bool {
    !reads(spec, root).is_empty()
}

/// The ODM item an `odm` read names, with its dataset.
fn item(spec: &Spec, e: ExprId) -> Result<(&str, &str), PrepareError> {
    let ExprKind::Odm(read) = &spec.expr(e).kind else {
        return Err(PrepareError::Internal);
    };
    let item = name(spec, read.item).ok_or(PrepareError::Internal)?;
    let (dataset, _) = item.split_once('.').ok_or(PrepareError::Internal)?;
    Ok((item, dataset))
}

/// The inputs read as ODM: the dataset of each `odm` read in a derivation.
pub(in crate::specification) fn inputs(spec: &Spec) -> Result<BTreeSet<&str>, PrepareError> {
    let mut found = BTreeSet::new();
    for (root, ..) in derivation_sites(spec) {
        for (read, _) in reads(spec, root) {
            found.insert(item(spec, read)?.1);
        }
    }
    Ok(found)
}

/// Each `odm` read's scope and filter names, in derivation order: a read of
/// an undeclared input, a read outside a context its dataset drives, and
/// each filter name that is no ODM field of its dataset.
pub(in crate::specification) fn context_findings(
    spec: &Spec,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let inputs = spec
        .inputs
        .iter()
        .map(|input| input.name.name.as_str())
        .collect::<Vec<_>>();
    let rows = &spec.rows.templates;
    let mut findings = Vec::new();
    for (root, location, row, column) in derivation_sites(spec) {
        for (read, derive) in reads(spec, root) {
            let (item, dataset) = item(spec, read)?;
            let path = path(spec, read);
            if !inputs.contains(&dataset) {
                findings.push(PreflightFinding::Odm(
                    source_expressions::SourceFinding::new(
                        C::SourceUnknownReference,
                        &format!("{path}.item"),
                        &[("identifier", item)],
                    )
                    .diagnostic(),
                ));
                continue;
            }
            let location = if derive {
                OdmLocation::Derive
            } else {
                location
            };
            let scoped = match location {
                OdmLocation::Intermediate | OdmLocation::Derive => false,
                OdmLocation::Row => {
                    let template = rows.get(row.ok_or(PrepareError::Internal)?);
                    template.and_then(|template| template_driver(spec, template)) == Some(dataset)
                }
                OdmLocation::Column if rows.is_empty() => default_driver(spec) == Some(dataset),
                OdmLocation::Column => {
                    let column = column.ok_or(PrepareError::Internal)?;
                    rows.iter().all(|template| {
                        template
                            .overrides
                            .iter()
                            .any(|(name, _)| name.name == column)
                            || template_driver(spec, template) == Some(dataset)
                    })
                }
            };
            if !scoped {
                findings.push(PreflightFinding::Odm(
                    OdmFinding::InvalidContext {
                        path: &path,
                        dataset,
                        location,
                    }
                    .diagnostic(),
                ));
            }
            let ExprKind::Odm(odm) = &spec.expr(read).kind else {
                return Err(PrepareError::Internal);
            };
            let Some(filter) = odm.filter else {
                continue;
            };
            let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                continue;
            };
            let path = render(spec, formula.text.meta);
            match &formula.compiled {
                Ok(plan) => {
                    let mut seen = BTreeSet::new();
                    for identifier in plan.identifiers() {
                        if !seen.insert(identifier) {
                            continue;
                        }
                        let code = match identifier.split_once('.') {
                            Some((owner, field))
                                if owner == dataset
                                    && crate::odm_source::FIELDS.contains(&field) =>
                            {
                                continue
                            }
                            Some((owner, _)) if owner == dataset => C::SourceUnknownReference,
                            _ => C::SourceFilterReference,
                        };
                        let fields = [("identifier", identifier), ("dataset", dataset)];
                        findings.push(PreflightFinding::Odm(
                            source_expressions::SourceFinding::new(code, &path, &fields)
                                .diagnostic(),
                        ));
                    }
                }
                Err(CompileError::Invalid {
                    position, failure, ..
                }) => findings.push(PreflightFinding::Odm(failure.diagnostic(
                    ExpressionContext::Predicate,
                    &path,
                    &formula.text.text,
                    position.character,
                ))),
                // Expression preparation keeps resource refusals.
                Err(_) => {}
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree;

    #[test]
    fn dictionary_data_and_literal_values_select_no_odm_read() {
        for expression in [
            Tree::map(vec![(
                "mapping",
                Tree::map(vec![
                    ("source", Tree::text("SRC.X")),
                    (
                        "dict",
                        Tree::map(vec![("odm", Tree::text("SRC.IT.TEST.OID"))]),
                    ),
                ]),
            )]),
            Tree::map(vec![("literal", Tree::text("SRC.IT.TEST.OID"))]),
        ] {
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                (
                    "input",
                    Tree::map(vec![(
                        "SRC",
                        Tree::map(vec![("path", Tree::text("source.csv"))]),
                    )]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "columns",
                    Tree::List(vec![Tree::map(vec![
                        ("name", Tree::text("ID")),
                        ("type", Tree::text("str")),
                        ("derivation", Tree::map(vec![("value", expression)])),
                    ])]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("result.csv")),
                        ("columns", Tree::List(vec![Tree::text("ID")])),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            let document = Document::new(nodes, root).unwrap();
            let admitted = SpecificationDocument::admit(document).unwrap().unwrap();
            let lowered = crate::front_end::v0_5::lower(&admitted, None);
            let spec = lowered.spec();
            let derivation = spec.columns[0].derivation.as_ref().unwrap().expr;
            assert!(!reads_odm(spec, derivation));
            assert!(inputs(spec).unwrap().is_empty());
            assert!(context_findings(spec).unwrap().is_empty());
        }
    }
}
