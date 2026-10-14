//! Source-independent intermediate admission and owned applicable-key inference.
use super::*;
use crate::value::Value;
use crate::{diagnostic::ConditionCode as C, key_relation::Inference, value::ColumnType};

/// Declaration failures precede source access and key-type binding.
pub(super) fn declaration_findings(
    d: &Document,
    items: &[usize],
    sources: &[SourceDeclaration],
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let mut findings = Vec::new();
    let mut bytes = 0usize;
    let mut identifiers = BTreeMap::<&str, String>::new();
    for (index, &item) in items.iter().enumerate() {
        let path = format!("intermediates[{index}]");
        let name = text(d, field(d, item, "id")?)?;
        let dataset = text(d, field(d, item, "dataset")?)?;
        let mut admit_finding = || -> Result<(), PrepareError> {
            bytes = bytes
                .checked_add(path.len())
                .and_then(|n| n.checked_add(name.len()))
                .and_then(|n| n.checked_add(dataset.len()))
                .and_then(|n| n.checked_add(256))
                .filter(|&n| n <= 16_777_216)
                .ok_or(PrepareError::Limit("intermediate_findings"))?;
            Ok(())
        };
        if let Some(previous) = identifiers.get(name).cloned().or_else(|| {
            sources
                .iter()
                .find(|source| source.name == name)
                .map(|_| format!("input.{name}"))
        }) {
            admit_finding()?;
            findings.push(PreflightFinding::Scalar(crate::diagnostic::Diagnostic {
                code: C::IntermediateIdentifierCollision,
                spec_paths: vec![format!("{path}.id"), previous],
                context: [(
                    "identifier".into(),
                    crate::diagnostic::ContextValue::Scalar(Value::Str(name.into())),
                )]
                .into(),
                source_span: None,
                operand_route: None,
            }));
        } else {
            identifiers.insert(name, format!("{path}.id"));
        }
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
        if !changes_relation
            && d.field(item, "no_match")
                .is_some_and(|node| matches!(d.nodes()[node], N::Null))
        {
            admit_finding()?;
            let mut finding = LookupFinding::new(&path, C::IntermediateRenameOnly);
            finding.text("intermediate", name);
            finding.text("dataset", dataset);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
        for (declared, missing) in [("order_by", "keep"), ("keep", "order_by")] {
            if present(d, item, declared) && !present(d, item, missing) {
                admit_finding()?;
                let mut finding = LookupFinding::new(&path, C::IntermediateSelectionUnpaired);
                finding.text("intermediate", name);
                finding.texts("declared", &[declared]);
                finding.texts("missing", &[missing]);
                findings.push(PreflightFinding::Scalar(finding.diagnostic()));
            }
        }
    }
    Ok(findings)
}

/// Reuse the core REQ-0150 service, retaining its output-key order and first mismatch.
#[allow(clippy::too_many_arguments)]
pub(super) fn inferred_keys(
    catalog: &Catalog,
    output: &TableSchema,
    keys: &[usize],
    donor: &TableSchema,
    admits: &impl Fn(usize) -> bool,
    path: &str,
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
        .map(|(_, column)| reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    let inferred = crate::key_relation::infer(catalog, &names, &fields, Default::default())
        .map_err(BindError::Catalog)?;
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
            let mut finding = LookupFinding::new(path, C::IntermediateNoApplicableKeys);
            finding.text("dataset", dataset);
            finding.texts("keys", &names);
            finding.text("hint", "declare the `key` explicitly");
            findings.push(BindFinding::Lookup(finding));
            Ok(None)
        }
        Inference::Incompatible {
            key,
            expected,
            actual,
        } => {
            let mut finding = LookupFinding::new(path, C::ImplicitSourceKeyType);
            finding.text("source", names[key]);
            finding.text("expected", type_name(expected));
            finding.text("actual", type_name(actual));
            findings.push(BindFinding::Lookup(finding));
            Ok(None)
        }
        Inference::UndeclaredOutput { .. } => Err(BindError::Internal),
    }
}

fn type_name(kind: ColumnType) -> &'static str {
    match kind {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}
