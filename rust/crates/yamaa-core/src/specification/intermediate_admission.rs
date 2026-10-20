//! Source-independent intermediate admission and owned applicable-key inference.
use super::*;
use crate::{
    diagnostic::{
        lookup::{self, Owner},
        ConditionCode as C,
    },
    key_relation::Inference,
    value::ColumnType,
};

/// Without a relation change or `require_match`, an intermediate whose absence
/// answer is missing, by default or as `no_match: {literal: null}`, reads what
/// the implicit join reads (LOOKUP-4).
fn is_rename_only(d: &Document, item: usize) -> bool {
    let changes_relation = [
        "key",
        "between",
        "filter",
        "order_by",
        "keep",
        "columns",
        "derivations",
        "verifications",
    ]
    .iter()
    .any(|field| present(d, item, field));
    !changes_relation
        && !requires_match(d, item)
        && d.field(item, "no_match")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
            .is_none_or(|node| missing_literal(d, node))
}

/// Declaration failures precede source access and key-type binding.
pub(super) fn declaration_findings(
    d: &Document,
    items: &[usize],
    sources: &[SourceDeclaration],
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let mut findings = Vec::new();
    let mut identifiers = BTreeMap::<&str, String>::new();
    let domain = text(d, field(d, d.root(), "domain")?)?;
    for (index, &item) in items.iter().enumerate() {
        let path = format!("intermediates[{index}]");
        let name = text(d, field(d, item, "id")?)?;
        let dataset = text(d, field(d, item, "dataset")?)?;
        if let Some(previous) = identifiers.get(name).cloned().or_else(|| {
            sources
                .iter()
                .find(|source| source.name == name)
                .map(|_| format!("input.{name}"))
                .or_else(|| (name == domain).then(|| "domain".into()))
        }) {
            findings.push(PreflightFinding::Scalar(crate::diagnostic::Diagnostic {
                code: C::IntermediateIdentifierCollision,
                spec_paths: vec![format!("{path}.id"), previous],
                context: [("identifier".into(), crate::json::Value::String(name.into()))].into(),
                source_span: None,
                operand_route: None,
            }));
        } else {
            identifiers.insert(name, format!("{path}.id"));
        }
        if !(sources.iter().any(|source| source.name == dataset)
            || dataset == "SELF" && present(d, d.root(), "rows"))
        {
            let mut finding =
                LookupFinding::new(&format!("{path}.dataset"), C::IntermediateDatasetReference);
            finding.text("identifier", dataset);
            finding.text("intermediate", name);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
        if d.field(item, "key").is_some_and(|key| {
            matches!(&d.nodes()[key], N::Mapping(pairs) if pairs.is_empty())
                || matches!(&d.nodes()[key], N::Sequence(pairs) if pairs.is_empty())
        }) {
            findings.push(PreflightFinding::Scalar(lookup::empty_key(
                &format!("{path}.key"),
                Owner::Intermediate(name),
                dataset,
            )));
        }
        // A list pairs each column with its same-named value once (LOOKUP-5).
        if let Some(N::Sequence(columns)) = d.field(item, "key").map(|key| &d.nodes()[key]) {
            for (position, &column) in columns.iter().enumerate() {
                let column = text(d, column)?;
                if columns[..position]
                    .iter()
                    .any(|&prior| text(d, prior).is_ok_and(|prior| prior == column))
                {
                    let mut finding = LookupFinding::new(
                        &format!("{path}.key[{position}]"),
                        C::IntermediateRepeatedKey,
                    );
                    finding.text("intermediate", name);
                    finding.text("dataset", dataset);
                    finding.text("identifier", column);
                    finding.text("expected", "each column once");
                    findings.push(PreflightFinding::Scalar(finding.diagnostic()));
                }
            }
        }
        if is_rename_only(d, item) {
            let mut finding = LookupFinding::new(&path, C::IntermediateRenameOnly);
            finding.text("intermediate", name);
            finding.text("dataset", dataset);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
        // Asserting a match and answering its absence are two absence policies.
        if requires_match(d, item) && present(d, item, "no_match") {
            findings.push(PreflightFinding::Scalar(lookup::absence_conflict(
                format!("{path}.no_match"),
                format!("{path}.require_match"),
                Owner::Intermediate(name),
            )));
        }
        // An `order_by` with no term orders nothing, so a `keep` beside it has no
        // order to choose by, as for a source's selection (LOOKUP-10).
        let ordered = d
            .field(item, "order_by")
            .is_some_and(|id| matches!(&d.nodes()[id], N::Sequence(terms) if !terms.is_empty()));
        for (declared, missing, unpaired) in [
            (
                "order_by",
                "keep",
                present(d, item, "order_by") && !present(d, item, "keep"),
            ),
            ("keep", "order_by", present(d, item, "keep") && !ordered),
        ] {
            if unpaired {
                findings.push(PreflightFinding::Scalar(lookup::selection_unpaired(
                    &path,
                    Owner::Intermediate(name),
                    declared,
                    missing,
                )));
            }
        }
    }
    Ok(findings)
}

/// Reuse the core LOOKUP-1 service, retaining its output-key order and first mismatch.
#[allow(clippy::too_many_arguments)]
pub(super) fn inferred_keys(
    catalog: &Catalog,
    output: &TableSchema,
    keys: &[usize],
    donor: &TableSchema,
    donor_kind: &impl Fn(usize) -> Option<ColumnType>,
    admits: &impl Fn(usize) -> bool,
    path: &str,
    intermediate: &str,
    dataset: &str,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Vec<(String, match_values::Declaration)>>, BindError> {
    let names = keys
        .iter()
        .map(|&key| output.columns()[key].name.as_str())
        .collect::<Vec<_>>();
    let fields = donor
        .columns()
        .iter()
        .enumerate()
        .filter(|(index, _)| admits(*index))
        .map(|(index, column)| (column.name.as_str(), donor_kind(index)))
        .collect::<Vec<_>>();
    let inferred =
        crate::key_relation::infer_partial(catalog, &names, &fields).map_err(BindError::Catalog)?;
    match inferred {
        Inference::Keys { keys } => Ok(Some(
            keys.into_iter()
                .map(|key| {
                    let name = String::from(names[key]);
                    (name.clone(), match_values::Declaration::Variable(name))
                })
                .collect(),
        )),
        Inference::NoApplicableKeys => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::no_applicable_keys(path, dataset, &names, true),
            )));
            Ok(None)
        }
        Inference::Incompatible {
            key,
            expected,
            actual,
        } => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::key_type(
                    path,
                    Owner::Intermediate(intermediate),
                    names[key],
                    expected.into(),
                    actual.into(),
                ),
            )));
            Ok(None)
        }
        Inference::UndeclaredOutput { .. } => Err(BindError::Internal),
    }
}
