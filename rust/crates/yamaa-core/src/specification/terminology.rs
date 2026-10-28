//! Lower column and value-level terminology against one admitted environment.
use super::*;
use crate::{
    ast,
    dataset::{Check, ValueCodelist, Verification, WhereClause},
    project_codelist_binding as ct,
    project_terminology::Catalogue,
};

/// What a 1.x specification checked or run without a project file binds that
/// only a project supplies, in column order: the column's codelist and
/// inventory vocabulary, then each of its value-level entries'. Each fails
/// validation as `project_binding_missing` at the field that binds it, with
/// the column and, for a codelist, the codelist (CT-13).
pub(super) fn unbound(spec: &ast::Spec) -> Vec<PreflightFinding> {
    use crate::diagnostic::{ConditionCode, Context, Diagnostic};
    use crate::json::Value;
    let missing = |path: String, binding: &str, column: &str, codelist: Option<&str>| {
        let mut context = Context::new();
        context.insert("binding".into(), Value::String(binding.into()));
        context.insert("column".into(), Value::String(column.into()));
        if let Some(codelist) = codelist {
            context.insert("codelist".into(), Value::String(codelist.into()));
        }
        PreflightFinding::Scalar(Diagnostic {
            code: ConditionCode::ProjectBindingMissing,
            spec_paths: vec![path],
            context,
            source_span: None,
            operand_route: None,
        })
    };
    let mut findings = Vec::new();
    for column in &spec.columns {
        let Some(submission) = &column.submission else {
            continue;
        };
        let declarations = std::iter::once((&submission.metadata, submission.meta)).chain(
            submission
                .value_level
                .iter()
                .map(|entry| (&entry.metadata, entry.meta)),
        );
        for (metadata, meta) in declarations {
            if let Some(codelist) = &metadata.codelist {
                findings.push(missing(
                    spec.cite(codelist.meta),
                    "codelist",
                    &column.name.name,
                    Some(&codelist.name),
                ));
            }
            if metadata.inventory_vocabulary == Some(true) {
                findings.push(missing(
                    format!("{}.inventory_vocabulary", spec.cite(meta)),
                    "inventory_vocabulary",
                    &column.name.name,
                    None,
                ));
            }
        }
    }
    findings
}

/// The project codelists a specification's `decode` expressions read, and
/// what refuses each read (EXPR-15).
pub(super) struct Decodes {
    /// Each codelist a decode reads, once, as the project declares it.
    pub(super) codelists: Vec<ast::Codelist>,
    pub(super) findings: Vec<PreflightFinding>,
}

/// Bind the codelist of each `decode`, in the order of the columns, row
/// templates and intermediates that hold them. Without a project file each
/// fails as `project_binding_missing` (CT-13); with one, a codelist the
/// project does not declare fails as `unknown_codelist`, and one that
/// declares no decode, whether its items have none or it defers to an
/// external dictionary, as `codelist_without_decodes`, each at the decode's
/// `codelist` with the codelist (EXPR-15).
pub(super) fn decodes(
    spec: &ast::Spec,
    catalogue: Option<&Catalogue>,
    projectless: bool,
) -> Decodes {
    use crate::diagnostic::{ConditionCode, Context, Diagnostic};
    use crate::json::Value;
    let finding = |code, path: String, binding: bool, codelist: &str| {
        let mut context = Context::new();
        if binding {
            context.insert("binding".into(), Value::String("codelist".into()));
        }
        context.insert("codelist".into(), Value::String(codelist.into()));
        PreflightFinding::Scalar(Diagnostic {
            code,
            spec_paths: vec![path],
            context,
            source_span: None,
            operand_route: None,
        })
    };
    let mut decodes = Decodes {
        codelists: Vec::new(),
        findings: Vec::new(),
    };
    for expr in &spec.exprs {
        let ast::ExprKind::Scalar {
            op: ast::ScalarOp::Decode { codelist },
            ..
        } = &expr.kind
        else {
            continue;
        };
        let path = format!("{}.codelist", spec.cite(expr.meta));
        if projectless {
            decodes.findings.push(finding(
                ConditionCode::ProjectBindingMissing,
                path,
                true,
                codelist,
            ));
            continue;
        }
        let Some(list) = catalogue.and_then(|catalogue| catalogue.get(codelist)) else {
            decodes.findings.push(finding(
                ConditionCode::DecodeUnknownCodelist,
                path,
                false,
                codelist,
            ));
            continue;
        };
        // A codelist declares a decode for every item or for none (CT-5).
        let decoded = list.items.as_deref().is_some_and(|items| {
            !items.is_empty() && items.iter().all(|item| item.decode.is_some())
        });
        if !decoded {
            decodes.findings.push(finding(
                ConditionCode::DecodeWithoutDecodes,
                path,
                false,
                codelist,
            ));
        } else if !decodes
            .codelists
            .iter()
            .any(|kept| kept.id.name == list.id.name)
        {
            decodes.codelists.push(list.clone());
        }
    }
    decodes
}

/// One binding: the column's own, or an entry's over the rows its where
/// clause selects.
struct Declaration<'a> {
    column: usize,
    identifier: &'a str,
    path: String,
    clause: Option<WhereClause>,
}
pub(super) fn apply(
    spec: &ast::Spec,
    output: &TableSchema,
    catalogue: &Catalogue,
    groups: &mut [verifications::Verifications],
) -> Result<(), PrepareError> {
    let mut declarations = Vec::new();
    for (column, declared) in spec.columns.iter().enumerate() {
        let Some(submission) = &declared.submission else {
            continue;
        };
        if let Some(codelist) = &submission.metadata.codelist {
            declarations.push(Declaration {
                column,
                identifier: &codelist.name,
                path: spec.cite(codelist.meta),
                clause: None,
            });
        }
        for entry in &submission.value_level {
            let Some(codelist) = &entry.metadata.codelist else {
                continue;
            };
            // Value-level admission owns a clause no row could be selected by;
            // no partially selected scope is executable.
            let Some(clause) = super::value_level::clause(entry, output, column) else {
                continue;
            };
            declarations.push(Declaration {
                column,
                identifier: &codelist.name,
                path: spec.cite(codelist.meta),
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
                        path: declaration.path.clone(),
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
            let submission = spec.columns[column]
                .submission
                .as_ref()
                .ok_or(PrepareError::Internal)?;
            group.append_codelist(Verification {
                path: format!("{}.codelist", spec.cite(submission.meta)),
                check: Check::ValueCodelists {
                    overrides,
                    bindings: lists,
                },
            });
        }
    }
    Ok(())
}
