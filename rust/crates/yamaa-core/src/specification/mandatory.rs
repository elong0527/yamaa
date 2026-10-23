//! Resolve every column's `mandatory` without a study document and enforce it.
//! Output-key validation enforces a key column; every other mandatory column
//! receives one presence check at its completed checkpoint, scoped by the
//! value-level where clauses wherever an entry changes the resolution.
use super::*;
use crate::json::Value as V;
use crate::{
    ast::{self, ColumnCheckKind, SubmissionMetadata},
    cdisc_vocabulary::{Core, DatasetClass},
    dataset::{Check, RowScope, Verification, WhereClause},
    diagnostic::{ConditionCode as C, Diagnostic},
    specification_submission_document::resolve_mandatory,
    submission_values as values,
};

fn core(metadata: &SubmissionMetadata) -> Option<Core> {
    metadata
        .core
        .as_ref()
        .and_then(|core| Core::from_name(&core.text))
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

pub(super) fn admit(
    spec: &ast::Spec,
    output: &TableSchema,
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) {
    // The dataset's class decides whether `core` derives `mandatory`.
    let tabulation = spec
        .submission
        .as_ref()
        .is_some_and(|dataset| DatasetClass::tabulation_name(&dataset.class.text));
    for (index, column) in spec.columns.iter().enumerate() {
        let name = output.columns()[index].name.as_str();
        let key = spec.keys.iter().find(|key| key.name == name);
        let submission = column
            .submission
            .as_ref()
            .map(|submission| &submission.metadata);
        let declared = submission.and_then(|metadata| metadata.mandatory);
        let declared_core = submission.and_then(core);
        let path = format!("{}.submission", spec.cite(column.meta));
        if key.is_some() && declared == Some(false) {
            result.push(finding(
                C::SubmissionKeyMandatory,
                format!("{path}.mandatory"),
                &[("column", s(name)), ("value", V::Bool(false))],
            ));
        }
        let mandatory = resolve_mandatory(key.is_some(), declared, declared_core, tabulation);
        // An entry resolves over its merged metadata, taking `mandatory` and
        // `core` from the column where it declares neither.
        let mut overrides = Vec::new();
        for entry in values::levels(column) {
            let entry_path = spec.cite(entry.meta);
            let entry_declared = entry.metadata.mandatory;
            if key.is_some() && entry_declared == Some(false) {
                result.push(finding(
                    C::SubmissionKeyMandatory,
                    format!("{entry_path}.mandatory"),
                    &[
                        ("column", s(name)),
                        ("entry", s(&entry.id.name)),
                        ("value", V::Bool(false)),
                    ],
                ));
            }
            let entry_core = core(&entry.metadata);
            if entry_declared.is_none() && entry_core.is_none() {
                continue;
            }
            let resolved = resolve_mandatory(
                key.is_some(),
                entry_declared.or(declared),
                entry_core.or(declared_core),
                tabulation,
            );
            if resolved == mandatory {
                continue;
            }
            // Value-level admission reports a clause it cannot select by.
            let Some(clause) = super::value_level::clause(entry, output, index) else {
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
        let stated_by = match key {
            Some(key) => spec.cite(key.meta),
            None if declared.is_some() => format!("{path}.mandatory"),
            None => format!("{path}.core"),
        };
        if mandatory && overrides.is_empty() {
            // A key's presence belongs to output-key validation; any other
            // column's to its resolved submission `mandatory`.
            let code = if key.is_some() {
                C::VerificationNotMissingRedundant
            } else {
                C::SubmissionMandatoryRedundant
            };
            for check in &column.checks {
                if matches!(check.kind, ColumnCheckKind::NotMissing { .. }) {
                    result.push(finding(
                        code,
                        spec.cite(check.meta),
                        &[("column", s(name)), ("stated_by", s(&stated_by))],
                    ));
                }
            }
        }
        if key.is_some() {
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
}
