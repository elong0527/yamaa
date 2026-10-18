//! Resolve every column's `mandatory` without a study document and enforce it.
//! Output-key validation enforces a key column; every other mandatory column
//! receives one presence check at its completed checkpoint, scoped by the
//! value-level where clauses wherever an entry changes the resolution.
use super::*;
use crate::json::Value as V;
use crate::{
    dataset::{Check, RowScope, Verification, WhereClause},
    diagnostic::{ConditionCode as C, Diagnostic},
    specification_submission_document::{resolve_mandatory, Core, DatasetClass},
    submission_values as values,
};

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

/// An entry whose own `mandatory` or `core` resolves differently from its
/// column: it decides the rows its where clause selects.
struct ValueResolution {
    clause: WhereClause,
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
    output: &TableSchema,
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let root = d.root();
    let keys = sequence(d, field(d, root, "keys")?)?
        .iter()
        .map(|&id| text(d, id))
        .collect::<Result<Vec<_>, _>>()?;
    let tabulation = tabulation(d);
    for (index, &node) in columns.iter().enumerate() {
        let name = output.columns()[index].name.as_str();
        let position = keys.iter().position(|key| *key == name);
        let submission = present(d, node, "submission");
        let declared = submission.and_then(|id| boolean(d, id, "mandatory"));
        let declared_core = submission.and_then(|id| core(d, id));
        let path = format!("columns.{name}.submission");
        if position.is_some() && declared == Some(false) {
            result.push(finding(
                C::SubmissionKeyMandatory,
                format!("{path}.mandatory"),
                &[("column", s(name))],
            ));
        }
        let mandatory = resolve_mandatory(position.is_some(), declared, declared_core, tabulation);
        // An entry resolves over its merged metadata, taking `mandatory` and
        // `core` from the column where it declares neither.
        let mut overrides = Vec::new();
        for entry in values::entries(d, node) {
            let entry_path = values::path(name, entry.id);
            let entry_declared = boolean(d, entry.node, "mandatory");
            if position.is_some() && entry_declared == Some(false) {
                result.push(finding(
                    C::SubmissionKeyMandatory,
                    format!("{entry_path}.mandatory"),
                    &[("column", s(name)), ("entry", s(entry.id))],
                ));
            }
            let entry_core = core(d, entry.node);
            if entry_declared.is_none() && entry_core.is_none() {
                continue;
            }
            let resolved = resolve_mandatory(
                position.is_some(),
                entry_declared.or(declared),
                entry_core.or(declared_core),
                tabulation,
            );
            if resolved == mandatory {
                continue;
            }
            // Value-level admission reports a clause it cannot select by.
            let Some(clause) = super::value_level::clause(&entry, output, index) else {
                continue;
            };
            overrides.push(ValueResolution {
                clause,
                path: format!(
                    "{entry_path}.{}",
                    if entry_declared.is_some() {
                        "mandatory"
                    } else {
                        "core"
                    }
                ),
            });
        }
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
        if overrides.is_empty() {
            if mandatory {
                groups[index].append_metadata(Verification {
                    path: stated_by,
                    check: Check::Mandatory(None),
                });
            }
            continue;
        }
        if mandatory {
            // Every row but those the relaxing entries select.
            groups[index].append_metadata(Verification {
                path: stated_by,
                check: Check::Mandatory(Some(RowScope {
                    clauses: overrides.into_iter().map(|value| value.clause).collect(),
                    include: false,
                })),
            });
        } else {
            for value in overrides {
                groups[index].append_metadata(Verification {
                    path: value.path,
                    check: Check::Mandatory(Some(RowScope {
                        clauses: vec![value.clause],
                        include: true,
                    })),
                });
            }
        }
    }
    Ok(())
}
