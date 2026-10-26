//! Compile a column's value-level entries (SUB-12, SUB-13): the static checks
//! that need no data, the where clauses value-level checks select rows by, and
//! the build checks that each row matches at most one entry of a column and
//! that every entry matches a row.
use super::*;
use crate::json::Value as V;
use crate::{
    ast,
    dataset::{Check, Verification, WhereClause, WhereTerm},
    diagnostic::{ConditionCode as C, Diagnostic},
    project_terminology::Catalogue,
    specification_submission_document::SubmissionMode,
    submission_values::{self as values, Declared, Finding},
};
use std::collections::BTreeSet;

/// An entry's where clause over the declared columns, when every where column
/// is a declared `str` column other than `own` listing a value; static
/// admission reports every other clause. An `NE` or `NOTIN` term selects the
/// rows that hold none of its values, a missing value included (SUB-12).
pub(super) fn clause(
    entry: &ast::ValueLevel,
    output: &TableSchema,
    own: usize,
) -> Option<WhereClause> {
    let terms = entry
        .conditions
        .iter()
        .map(|condition| {
            let column = output
                .columns()
                .iter()
                .position(|column| column.name == condition.column.name)?;
            (column != own
                && output.columns()[column].kind == ColumnType::Str
                && !condition.values.is_empty())
            .then(|| WhereTerm {
                column,
                values: condition
                    .values
                    .iter()
                    .map(|value| value.text.clone())
                    .collect(),
                excludes: condition.comparator.excludes(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!terms.is_empty()).then_some(WhereClause { terms })
}

fn diagnostic(column: &str, finding: &Finding<'_>) -> PreflightFinding {
    let text = |value: &str| V::String(value.into());
    let (code, paths, context) = match *finding {
        Finding::DuplicateId { id } => (
            C::SubmissionValueDuplicateId,
            vec![format!("{}.id", values::path(column, id))],
            vec![
                ("column", text(column)),
                ("entry", text(id)),
                ("identifier", text(id)),
            ],
        ),
        Finding::Where {
            id,
            column: where_column,
            ref fault,
        } => {
            let mut context = vec![
                ("column", text(column)),
                ("entry", text(id)),
                ("reason", text(fault.reason())),
            ];
            // The refused where values, in declared order.
            if let Some(refused) = fault.values() {
                let refused = refused.iter().map(|value| text(value)).collect();
                context.push(("values", V::Array(refused)));
            }
            // The comparator whose value count is refused.
            if let Some(comparator) = fault.comparator() {
                context.push(("comparator", text(comparator)));
            }
            // A clause that names no column has no where column.
            let path = match where_column {
                Some(where_column) => {
                    context.push(("where_column", text(where_column)));
                    format!("{}.where.{where_column}", values::path(column, id))
                }
                None => {
                    context.push(("where_column", V::Null));
                    format!("{}.where", values::path(column, id))
                }
            };
            (C::SubmissionValueWhereInvalid, vec![path], context)
        }
        // The failure names both where clauses, in declaration order.
        Finding::Overlap { id, other } => (
            C::SubmissionValueOverlap,
            vec![
                format!("{}.where", values::path(column, other)),
                format!("{}.where", values::path(column, id)),
            ],
            vec![
                ("column", text(column)),
                ("entries", V::Array(vec![text(other), text(id)])),
            ],
        ),
    };
    PreflightFinding::Scalar(Diagnostic {
        code,
        spec_paths: paths,
        context: context
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        source_span: None,
        operand_route: None,
    })
}

/// The values a where column's codelists admit: its own binding's and those
/// of its entries, read where a project supplies the codelists.
fn codelist<'c>(catalogue: &'c Catalogue, column: &ast::Column) -> Option<BTreeSet<&'c str>> {
    let submission = column.submission.as_ref()?;
    values::admitted(
        catalogue,
        submission
            .metadata
            .codelist
            .as_ref()
            .map(|list| list.name.as_str()),
        submission
            .value_level
            .iter()
            .filter_map(|entry| entry.metadata.codelist.as_ref())
            .map(|list| list.name.as_str()),
    )
}

/// Admit every column's entries without data and give each column with
/// entries its build checks: one that fails a row two entries select, and one
/// `warning` per entry that matches no row. The checks follow the column's
/// declared verifications and precede its other metadata checks.
pub(super) fn admit(
    spec: &ast::Spec,
    output: &TableSchema,
    catalogue: Option<&Catalogue>,
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let carried = spec
        .output
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<BTreeSet<_>>();
    for (index, column) in spec.columns.iter().enumerate() {
        let name = output.columns()[index].name.as_str();
        let Some(submission) = column
            .submission
            .as_ref()
            .filter(|submission| !submission.value_level.is_empty())
        else {
            continue;
        };
        let entries = &submission.value_level;
        let clauses = entries.iter().map(values::clause).collect::<Vec<_>>();
        let written = entries
            .iter()
            .zip(&clauses)
            .map(|(entry, clause)| (entry.id.name.as_str(), clause));
        let findings = values::admit(written, name, SubmissionMode::Validated, |where_column| {
            let index = output
                .columns()
                .iter()
                .position(|column| column.name == where_column)?;
            Some(Declared {
                output: carried.contains(where_column),
                text: output.columns()[index].kind == ColumnType::Str,
                codelist: catalogue.and_then(|catalogue| codelist(catalogue, &spec.columns[index])),
            })
        });
        if !findings.is_empty() {
            result.extend(findings.iter().map(|finding| diagnostic(name, finding)));
            continue;
        }
        let compiled = entries
            .iter()
            .map(|entry| clause(entry, output, index).map(|clause| (entry.id.name.clone(), clause)))
            .collect::<Option<Vec<_>>>()
            .ok_or(PrepareError::Internal)?;
        groups[index].append_metadata(Verification {
            path: format!("{}.value_level", spec.cite(submission.meta)),
            check: Check::ValueEntries(compiled.clone()),
        });
        for (entry, (id, clause)) in entries.iter().zip(compiled) {
            let path = spec.cite(entry.meta);
            groups[index].warning_paths.push(path.clone());
            groups[index].append_metadata(Verification {
                path,
                check: Check::ValueEntryMatch { id, clause },
            });
        }
    }
    Ok(())
}
