//! Lower inherited and per-value terminology against one admitted environment.
use super::*;
use crate::{
    dataset::{Check, ValueCodelist, Verification},
    project_codelist_binding as ct,
    project_terminology::Catalogue,
};

struct Declaration<'a> {
    column: usize,
    identifier: &'a str,
    path: String,
    code: Option<&'a str>,
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
    limits: crate::project_limits::Limits,
    groups: &mut [verifications::Verifications],
) -> Result<(), PrepareError> {
    // Reserve scope text before any code/path copy. Bound-codelist values and
    // findings keep their existing cumulative binder admission below.
    let mut budget = crate::project_limits::Budget::new(limits);
    for node in d.nodes() {
        if let N::Text(value) = node {
            budget
                .text(value)
                .map_err(|_| PrepareError::Limit("terminology_bindings"))?;
        }
    }
    let mut declarations = Vec::new();
    for (column, &node) in columns.iter().enumerate() {
        if let Some(identifier) =
            present(d, node, "submission").and_then(|id| present(d, id, "codelist"))
        {
            declarations.push(Declaration {
                column,
                identifier: text(d, identifier)?,
                path: format!(
                    "columns.{}.submission.codelist",
                    output.columns()[column].name
                ),
                code: None,
            });
        }
    }
    let discriminator_name = format!("{}TESTCD", text(d, field(d, d.root(), "domain")?)?);
    let discriminator = output
        .columns()
        .iter()
        .position(|c| c.name == discriminator_name);
    if let Some(rows) = present(d, d.root(), "rows") {
        for &row in sequence(d, rows)? {
            let Some(submission) = present(d, row, "submission") else {
                continue;
            };
            let code = present(d, row, "derivations")
                .and_then(|id| present(d, id, &discriminator_name))
                .and_then(|id| present(d, id, "value"))
                .and_then(|id| present(d, id, "literal"))
                .and_then(|id| {
                    if let N::Text(text) = &d.nodes()[id] {
                        Some(text.as_str())
                    } else {
                        None
                    }
                });
            // Submission admission owns the absent/dynamic discriminator and
            // unknown-column findings. No partially selected scope is executable.
            let (Some(code), Some(_)) = (code, discriminator) else {
                continue;
            };
            for &(name, metadata) in mapping(d, submission)? {
                let name = text(d, name)?;
                let Some(column) = output.columns().iter().position(|c| c.name == name) else {
                    continue;
                };
                let Some(identifier) = present(d, metadata, "codelist") else {
                    continue;
                };
                for value in [text(d, field(d, row, "id")?)?, name, code] {
                    budget
                        .text(value)
                        .map_err(|_| PrepareError::Limit("terminology_bindings"))?;
                }
                declarations.push(Declaration {
                    column,
                    identifier: text(d, identifier)?,
                    path: format!(
                        "rows.{}.submission.{name}.codelist",
                        text(d, field(d, row, "id")?)?
                    ),
                    code: Some(code),
                });
            }
        }
    }
    let allowed = groups
        .iter()
        .map(|group| {
            group
                .checks
                .iter()
                .filter_map(|check| match &check.check {
                    Check::AllowedValues(values) => Some(ct::AllowedValues {
                        path: &check.path,
                        values,
                    }),
                    _ => None,
                })
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
            allowed_values: &allowed[declaration.column],
        })
        .collect::<Vec<_>>();
    let checks = ct::bind(catalogue, &bindings, limits).map_err(|error| match error {
        ct::Error::Limit(_) => PrepareError::Limit("terminology_bindings"),
        ct::Error::Findings(findings) => PrepareError::Invalid(
            findings
                .into_iter()
                .map(|finding| PreflightFinding::ProjectCodelist(alloc::boxed::Box::new(finding)))
                .collect(),
        ),
    })?;
    for (column, group) in groups.iter_mut().enumerate() {
        let overrides = declarations
            .iter()
            .filter(|d| d.column == column)
            .filter_map(|d| d.code)
            .collect::<alloc::collections::BTreeSet<_>>();
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
                        codes: Vec::new(),
                        default: false,
                    });
                    lists.len() - 1
                }
            };
            match declaration.code {
                None => lists[index].default = true,
                Some(code) if !lists[index].codes.iter().any(|prior| prior == code) => {
                    lists[index].codes.push(code.into())
                }
                _ => (),
            }
        }
        if !lists.is_empty() {
            group.append_codelist(Verification {
                path: format!(
                    "columns.{}.submission.codelist",
                    output.columns()[column].name
                ),
                check: Check::ValueCodelists {
                    discriminator: discriminator.expect("admitted literal owner"),
                    overrides: overrides.into_iter().map(String::from).collect(),
                    bindings: lists,
                },
            });
        }
    }
    Ok(())
}
