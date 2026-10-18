//! Lower column and value-level terminology against one admitted environment.
use super::*;
use crate::{
    dataset::{Check, ValueCodelist, Verification, WhereClause},
    project_codelist_binding as ct,
    project_terminology::Catalogue,
    submission_values as values,
};

/// One binding: the column's own, or an entry's over the rows its where
/// clause selects.
struct Declaration<'a> {
    column: usize,
    identifier: &'a str,
    path: String,
    clause: Option<WhereClause>,
}
fn present(d: &Document, node: usize, name: &str) -> Option<usize> {
    d.field(node, name)
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
}
pub(super) fn apply(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    catalogue: &Catalogue,
    groups: &mut [verifications::Verifications],
) -> Result<(), PrepareError> {
    let mut declarations = Vec::new();
    for (column, &node) in columns.iter().enumerate() {
        let name = output.columns()[column].name.as_str();
        if let Some(identifier) =
            present(d, node, "submission").and_then(|id| present(d, id, "codelist"))
        {
            declarations.push(Declaration {
                column,
                identifier: text(d, identifier)?,
                path: format!("columns.{name}.submission.codelist"),
                clause: None,
            });
        }
        for entry in values::entries(d, node) {
            let Some(identifier) = present(d, entry.node, "codelist") else {
                continue;
            };
            // Value-level admission owns a clause no row could be selected by;
            // no partially selected scope is executable.
            let Some(clause) = super::value_level::clause(&entry, output, column) else {
                continue;
            };
            declarations.push(Declaration {
                column,
                identifier: text(d, identifier)?,
                path: format!("{}.codelist", values::path(name, entry.id)),
                clause: Some(clause),
            });
        }
    }
    // A column-level list of items states the column's value set, so an
    // `allowed_values` beside it is redundant. A value-level list governs only
    // its own rows and leaves the column's verification standing.
    let allowed = groups
        .iter()
        .map(|group| {
            group
                .checks
                .iter()
                .filter(|check| matches!(check.check, Check::AllowedValues(_)))
                .map(|check| check.path.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let bindings = declarations
        .iter()
        .map(|declaration| ct::Binding {
            column: declaration.column,
            name: &output.columns()[declaration.column].name,
            kind: output.columns()[declaration.column].kind,
            path: ct::Path::Authored(&declaration.path),
            codelist: declaration.identifier,
            allowed_values: if declaration.clause.is_none() {
                allowed[declaration.column].as_slice()
            } else {
                &[]
            },
        })
        .collect::<Vec<_>>();
    let checks = ct::bind(catalogue, &bindings).map_err(|findings| {
        PrepareError::Invalid(
            findings
                .into_iter()
                .map(|finding| PreflightFinding::ProjectCodelist(Box::new(finding)))
                .collect(),
        )
    })?;
    for (column, group) in groups.iter_mut().enumerate() {
        // The rows of every entry declaring a codelist leave the column's
        // binding, whether or not that codelist lists items.
        let overrides = declarations
            .iter()
            .filter(|declaration| declaration.column == column)
            .filter_map(|declaration| declaration.clause.clone())
            .collect::<Vec<_>>();
        if overrides.is_empty() {
            for check in checks.iter().filter(|c| c.column == column) {
                group.append_codelist(check.verification.clone());
            }
            continue;
        }
        let mut lists: Vec<ValueCodelist> = Vec::new();
        for declaration in declarations.iter().filter(|d| d.column == column) {
            let Some(check) = checks.iter().find(|check| {
                check.column == column && check.verification.path == declaration.path
            }) else {
                continue;
            };
            let Check::Codelist { id, values } = &check.verification.check else {
                unreachable!("terminology binder emits fixed lists")
            };
            let index = match lists.iter().position(|list| list.id == *id) {
                Some(index) => index,
                None => {
                    lists.push(ValueCodelist {
                        id: id.clone(),
                        values: values.clone(),
                        clauses: Vec::new(),
                        default: false,
                    });
                    lists.len() - 1
                }
            };
            match &declaration.clause {
                None => lists[index].default = true,
                Some(clause) => lists[index].clauses.push(clause.clone()),
            }
        }
        if !lists.is_empty() {
            group.append_codelist(Verification {
                path: format!(
                    "columns.{}.submission.codelist",
                    output.columns()[column].name
                ),
                check: Check::ValueCodelists {
                    overrides,
                    bindings: lists,
                },
            });
        }
    }
    Ok(())
}
