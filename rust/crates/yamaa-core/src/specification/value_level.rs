//! Compile a column's value-level entries (SUB-12, SUB-13): the static checks
//! that need no data, the where clauses value-level checks select rows by, and
//! the build checks that each row matches at most one entry of a column and
//! that every entry matches a row.
use super::*;
use crate::json::Value as V;
use crate::{
    dataset::{Check, Verification, WhereClause},
    diagnostic::{ConditionCode as C, Diagnostic},
    submission_values::{self as values, Declared, Entry, Finding},
};
use std::collections::BTreeSet;

/// An entry's where clause over the declared columns, when every where column
/// is a declared `str` column other than `own` listing a value; static
/// admission reports every other clause.
pub(super) fn clause(entry: &Entry<'_>, output: &TableSchema, own: usize) -> Option<WhereClause> {
    let terms = entry
        .clause
        .iter()
        .map(|(name, listed)| {
            let column = output
                .columns()
                .iter()
                .position(|column| column.name == *name)?;
            (column != own
                && output.columns()[column].kind == ColumnType::Str
                && !listed.is_empty())
            .then(|| {
                (
                    column,
                    listed.iter().map(|value| (*value).to_owned()).collect(),
                )
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
            vec![("column", text(column)), ("entry", text(id))],
        ),
        Finding::Where {
            id,
            column: where_column,
            fault,
        } => {
            let mut context = vec![
                ("column", text(column)),
                ("entry", text(id)),
                ("reason", text(fault.reason())),
            ];
            let path = match where_column {
                Some(where_column) => {
                    context.push(("where_column", text(where_column)));
                    format!("{}.where.{where_column}", values::path(column, id))
                }
                None => format!("{}.where", values::path(column, id)),
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

/// Admit every column's entries without data and give each column with
/// entries its build checks: one that fails a row two entries select, and one
/// `warning` per entry that matches no row. The checks follow the column's
/// declared verifications and precede its other metadata checks.
pub(super) fn admit(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let carried = sequence(d, field(d, field(d, d.root(), "output")?, "columns")?)?
        .iter()
        .map(|&id| text(d, id))
        .collect::<Result<BTreeSet<_>, _>>()?;
    for (index, &node) in columns.iter().enumerate() {
        let name = output.columns()[index].name.as_str();
        let entries = values::entries(d, node);
        if entries.is_empty() {
            continue;
        }
        let findings = values::admit(&entries, name, |where_column| {
            let declared = output
                .columns()
                .iter()
                .find(|column| column.name == where_column)?;
            Some(Declared {
                output: carried.contains(where_column),
                text: declared.kind == ColumnType::Str,
            })
        });
        if !findings.is_empty() {
            result.extend(findings.iter().map(|finding| diagnostic(name, finding)));
            continue;
        }
        let compiled = entries
            .iter()
            .map(|entry| clause(entry, output, index).map(|clause| (entry.id.to_owned(), clause)))
            .collect::<Option<Vec<_>>>()
            .ok_or(PrepareError::Internal)?;
        groups[index].append_metadata(Verification {
            path: format!("columns.{name}.submission.value_level"),
            check: Check::ValueEntries(compiled.clone()),
        });
        for (id, clause) in compiled {
            let path = values::path(name, &id);
            groups[index].warning_paths.push(path.clone());
            groups[index].append_metadata(Verification {
                path,
                check: Check::ValueEntryMatch { id, clause },
            });
        }
    }
    Ok(())
}
