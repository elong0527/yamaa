//! Resolve every column's `mandatory` without a study document and enforce it.
//! Output-key validation enforces a key column; every other mandatory column
//! receives one presence check at its completed checkpoint, scoped by the
//! completed discriminator wherever value-level metadata changes the resolution.
use super::*;
use crate::json::Value as V;
use crate::{
    dataset::{Check, CodeScope, Verification},
    diagnostic::{ConditionCode as C, Diagnostic},
    specification_submission_document::{resolve_mandatory, Core, DatasetClass},
};
use std::collections::{BTreeMap, BTreeSet};

fn present(d: &Document, node: usize, name: &str) -> Option<usize> {
    d.field(node, name)
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
}
fn boolean(d: &Document, node: usize, name: &str) -> Option<bool> {
    match d.nodes()[present(d, node, name)?] {
        N::Boolean(value) => Some(value),
        _ => None,
    }
}
fn core(d: &Document, node: usize) -> Option<Core> {
    match &d.nodes()[present(d, node, "core")?] {
        N::Text(value) => Core::from_name(value),
        _ => None,
    }
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn finding(code: C, path: String, context: &[(&str, V)]) -> PreflightFinding {
    PreflightFinding::Scalar(Diagnostic {
        code,
        spec_paths: vec![path],
        context: context
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone()))
            .collect(),
        source_span: None,
        operand_route: None,
    })
}

/// A row template's own resolution for a column whose `mandatory` or `core` it
/// declares; any other value-level entry keeps the column's resolution.
struct ValueResolution {
    code: String,
    mandatory: bool,
    path: String,
}

/// The dataset's class decides whether `core` derives `mandatory`.
fn tabulation(d: &Document) -> bool {
    present(d, d.root(), "submission")
        .and_then(|id| present(d, id, "class"))
        .is_some_and(
            |id| matches!(&d.nodes()[id], N::Text(name) if DatasetClass::tabulation_name(name)),
        )
}

pub(super) fn admit(
    d: &Document,
    columns: &[usize],
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let root = d.root();
    let keys = sequence(d, field(d, root, "keys")?)?
        .iter()
        .map(|&id| text(d, id))
        .collect::<Result<Vec<_>, _>>()?;
    let tabulation = tabulation(d);
    let discriminator_name = format!("{}TESTCD", text(d, field(d, root, "domain")?)?);
    let names = columns
        .iter()
        .map(|&node| text(d, field(d, node, "name")?))
        .collect::<Result<Vec<_>, _>>()?;
    let discriminator = names.iter().position(|name| *name == discriminator_name);
    let mut values = BTreeMap::<usize, Vec<ValueResolution>>::new();
    if let Some(rows) = present(d, root, "rows") {
        for &row in sequence(d, rows)? {
            let Some(submission) = present(d, row, "submission") else {
                continue;
            };
            let row_id = text(d, field(d, row, "id")?)?;
            // Submission admission owns an absent or dynamic discriminator.
            let code = present(d, row, "derivations")
                .and_then(|id| present(d, id, &discriminator_name))
                .and_then(|id| present(d, id, "value"))
                .and_then(|id| present(d, id, "literal"))
                .and_then(|id| match &d.nodes()[id] {
                    N::Text(code) => Some(code.as_str()),
                    _ => None,
                });
            for &(name, entry) in mapping(d, submission)? {
                let name = text(d, name)?;
                let Some(column) = names.iter().position(|known| *known == name) else {
                    continue;
                };
                let key = keys.contains(&name);
                let path = format!("rows.{row_id}.submission.{name}");
                let declared = boolean(d, entry, "mandatory");
                if key && declared == Some(false) {
                    result.push(finding(
                        C::SubmissionKeyMandatory,
                        format!("{path}.mandatory"),
                        &[("column", s(name)), ("row", s(row_id))],
                    ));
                }
                let entry_core = core(d, entry);
                let (Some(code), true) = (code, declared.is_some() || entry_core.is_some()) else {
                    continue;
                };
                let base = present(d, columns[column], "submission");
                let mandatory = resolve_mandatory(
                    key,
                    declared.or_else(|| base.and_then(|id| boolean(d, id, "mandatory"))),
                    entry_core.or_else(|| base.and_then(|id| core(d, id))),
                    tabulation,
                );
                values.entry(column).or_default().push(ValueResolution {
                    code: code.into(),
                    mandatory,
                    path: format!(
                        "{path}.{}",
                        if declared.is_some() {
                            "mandatory"
                        } else {
                            "core"
                        }
                    ),
                });
            }
        }
    }
    for (index, &node) in columns.iter().enumerate() {
        let name = names[index];
        let position = keys.iter().position(|key| *key == name);
        let submission = present(d, node, "submission");
        let declared = submission.and_then(|id| boolean(d, id, "mandatory"));
        let path = format!("columns.{name}.submission");
        if position.is_some() && declared == Some(false) {
            result.push(finding(
                C::SubmissionKeyMandatory,
                format!("{path}.mandatory"),
                &[("column", s(name))],
            ));
        }
        let mandatory = resolve_mandatory(
            position.is_some(),
            declared,
            submission.and_then(|id| core(d, id)),
            tabulation,
        );
        // Several row templates sharing a test code declare identical metadata
        // under submission admission; the first occurrence speaks for the code.
        let mut seen = BTreeSet::new();
        let overrides = values
            .remove(&index)
            .unwrap_or_default()
            .into_iter()
            .filter(|value| seen.insert(value.code.clone()))
            .filter(|value| value.mandatory != mandatory)
            .collect::<Vec<_>>();
        let stated_by = match position {
            Some(position) => format!("keys[{position}]"),
            None if declared.is_some() => format!("{path}.mandatory"),
            None => format!("{path}.core"),
        };
        if mandatory && overrides.is_empty() {
            // A key's presence belongs to output-key validation; any other
            // column's to its resolved submission `mandatory`.
            let code = if position.is_some() {
                C::VerificationNotMissingRedundant
            } else {
                C::SubmissionMandatoryRedundant
            };
            if let Some(entries) = present(d, node, "verifications") {
                for (entry, &id) in sequence(d, entries)?.iter().enumerate() {
                    if d.field(id, "not_missing").is_some() {
                        result.push(finding(
                            code,
                            format!("columns.{name}.verifications[{entry}].not_missing"),
                            &[("column", s(name)), ("stated_by", s(&stated_by))],
                        ));
                    }
                }
            }
        }
        if position.is_some() {
            continue;
        }
        let Some(discriminator) = discriminator.filter(|_| !overrides.is_empty()) else {
            if mandatory {
                groups[index].append_metadata(Verification {
                    path: stated_by,
                    check: Check::Mandatory(None),
                });
            }
            continue;
        };
        if mandatory {
            groups[index].append_metadata(Verification {
                path: stated_by,
                check: Check::Mandatory(Some(CodeScope {
                    discriminator,
                    codes: overrides.into_iter().map(|value| value.code).collect(),
                    include: false,
                })),
            });
        } else {
            for value in overrides {
                groups[index].append_metadata(Verification {
                    path: value.path,
                    check: Check::Mandatory(Some(CodeScope {
                        discriminator,
                        codes: vec![value.code],
                        include: true,
                    })),
                });
            }
        }
    }
    Ok(())
}
