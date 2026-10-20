//! Original flag declaration and post-capture binding; no predicate text is invented.
use super::*;
use crate::json::Value as V;
use crate::{
    dataset::{Flag, FlagBinding, FlagRead},
    diagnostic::{ConditionCode as C, Diagnostic},
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    condition: Result<CompiledExpression, Diagnostic>,
    path: String,
    true_value: crate::value::Value,
    false_value: Option<crate::value::Value>,
    missing: Option<crate::value::Value>,
}
fn finding(code: C, path: &str, context: crate::diagnostic::Context) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context,
        source_span: None,
        operand_route: None,
    }
}
fn value_text(value: &str) -> V {
    V::String(value.into())
}
impl Declaration {
    pub(super) fn has_findings(&self) -> bool {
        self.condition.is_err() || self.false_value.is_some() && self.missing.is_none()
    }
    pub(super) fn findings(&self, findings: &mut Vec<BindFinding>) {
        if let Err(diagnostic) = &self.condition {
            findings.push(BindFinding::Predicate(diagnostic.clone()));
        }
        if let Some(false_value) = self.false_value.as_ref().filter(|_| self.missing.is_none()) {
            findings.push(BindFinding::Predicate(finding(
                C::FlagMissingValueRequired,
                &format!("{}.missing", self.path),
                [("false_value".into(), V::from(false_value.clone()))].into(),
            )));
        }
    }
}
impl Declaration {
    /// The flag at `e`: its condition as compiled, or the grammar finding
    /// that keeps it from binding, and its three values.
    pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Self, PrepareError> {
        let ExprKind::Flag {
            condition,
            true_value,
            false_value,
            missing,
        } = &spec.expr(e).kind
        else {
            return Err(PrepareError::Internal);
        };
        let ExprKind::Formula(formula) = &spec.expr(*condition).kind else {
            return Err(PrepareError::Internal);
        };
        let condition_path = checks::render(spec, formula.text.meta);
        let condition = match &formula.compiled {
            Ok(plan) => Ok(plan.clone()),
            // The flag's public context carries the original predicate and its
            // character position, as every predicate field's does.
            Err(CompileError::Invalid {
                position, failure, ..
            }) => Err(failure.diagnostic(
                ExpressionContext::Predicate,
                &condition_path,
                &formula.text.text,
                position.character,
            )),
            Err(error) => {
                return Err(PrepareError::expression(
                    error.clone(),
                    Some(&condition_path),
                ))
            }
        };
        Ok(Self {
            condition,
            path: checks::path(spec, e),
            true_value: constant(&true_value.value, &checks::render(spec, true_value.meta))?,
            false_value: checks::handler(spec, false_value.as_ref())?,
            missing: checks::handler(spec, missing.as_ref())?,
        })
    }
}
fn unknown(path: &str, name: &str) -> BindFinding {
    BindFinding::Predicate(finding(
        C::PredicateUnknownField,
        path,
        [("identifier".into(), value_text(name))].into(),
    ))
}
#[allow(clippy::too_many_arguments)]
fn output_binding(
    name: &str,
    path: &str,
    catalog: &Catalog,
    driver: &str,
    available: Option<&[usize]>,
    row: Option<&str>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<FlagRead>, BindError> {
    if let Some(cause) = catalog
        .validate_output(name, None, available, &[0])
        .map_err(BindError::Catalog)?
    {
        match cause {
            reference_binding::Diagnostic::UnknownField => findings.push(unknown(path, name)),
            reference_binding::Diagnostic::UnresolvableName { .. } => {
                findings.push(BindFinding::Predicate(finding(
                    C::PredicateUnresolvableName,
                    path,
                    [
                        ("identifier".into(), value_text(name)),
                        ("suggestion".into(), value_text(&format!("{driver}.{name}"))),
                    ]
                    .into(),
                )))
            }
            _ => findings.push(BindFinding::OutputReference {
                path: path.into(),
                name: name.into(),
                row: row.map(String::from),
                finding: cause,
            }),
        }
        return Ok(None);
    }
    let Some(reference_binding::Binding::Output { column, .. }) = catalog.bind(name) else {
        return Err(BindError::Internal);
    };
    edges.push(column);
    Ok(Some(FlagRead::Read(Read::Column(column))))
}
/// The bound flag over its condition as the site types it (EXPR-14).
fn finish(
    operation: &Declaration,
    typed: impl FnOnce(&CompiledExpression) -> CompiledExpression,
    bindings: Vec<FlagBinding>,
) -> Result<Option<Expression>, BindError> {
    let Ok(condition) = &operation.condition else {
        return Ok(None);
    };
    if operation.has_findings() {
        return Ok(None);
    }
    Flag::new(
        typed(condition),
        bindings,
        operation.true_value.clone(),
        operation.false_value.clone(),
        operation.missing.clone(),
    )
    .map(|flag| Some(Expression::Flag(flag)))
    .map_err(|_| BindError::Internal)
}
/// Each name of the condition binds once, in this scope. The implicit
/// template's condition may read a lookup, and a driver field of its
/// non-key column collects the distinct readings of the row's records
/// unless record rows make the row one record.
pub(super) fn bind(
    operation: &Declaration,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    if !cx.reported {
        operation.findings(cx.findings);
    }
    let Ok(condition) = &operation.condition else {
        return Ok(None);
    };
    let implicit = cx.implicit();
    let path = condition.spec_path();
    let start = cx.findings.len();
    let mut bindings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for name in condition.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        let joined = name
            .split_once('.')
            .is_some_and(|(dataset, _)| cx.catalog.joined(dataset).is_some());
        let read = if let Some((index, field)) = implicit.and(cx.donors.named.reference(name)) {
            let Some(item) = &cx.donors.items[index] else {
                cx.findings.push(unknown(path, name));
                continue;
            };
            cx.edges
                .extend(row_intermediates::dependencies(index, cx.donors.items)?);
            match item.source_column(cx.donors.secondary, field) {
                Some(column) => Some(FlagRead::Read(Read::Intermediate { index, column })),
                None => {
                    cx.findings
                        .push(BindFinding::Lookup(LookupFinding::reference(
                            path,
                            name,
                            None,
                            ReferenceCause::Value,
                            None,
                        )));
                    continue;
                }
            }
        } else if let (crate::reference_scope::Phase::Row { group_by }, true) =
            (cx.scope.phase, joined)
        {
            let mut finding = LookupFinding::new(
                path,
                if group_by.is_some() {
                    C::GroupedRowNumericSourceScope
                } else {
                    C::RowNumericSourceScope
                },
            );
            finding.text("identifier", name);
            finding.texts("drivers", &[cx.scope.driver]);
            cx.findings.push(BindFinding::Lookup(finding));
            continue;
        } else if joined {
            let read = source_expressions::bind_catalog_join(
                name,
                path,
                cx.catalog,
                cx.row(),
                cx.scope.available,
                cx.edges,
                cx.findings,
            )?
            .and_then(|expression| match expression {
                Expression::Joined(read) => Some(FlagRead::Read(Read::Joined(read))),
                _ => None,
            });
            // The implicit template also names a refused join read an
            // unknown predicate field.
            if read.is_none() && implicit.is_some() {
                cx.findings.push(unknown(path, name));
            }
            read
        } else if !name.contains('.') {
            output_binding(
                name,
                path,
                cx.catalog,
                cx.scope.driver,
                cx.scope.available,
                cx.row(),
                cx.edges,
                cx.findings,
            )?
        } else {
            use crate::reference_scope::{self, Reach, Scope};
            let causes = reference_scope::validate(
                cx.catalog,
                name,
                None,
                Scope {
                    drivers: &[cx.scope.driver],
                    current_driver: false,
                    reach: Reach::Scalar,
                    joined: false,
                    phase: cx.scope.phase,
                },
            )
            .map_err(BindError::Catalog)?;
            if !causes.is_empty() {
                for cause in causes {
                    if cause == reference_scope::Finding::UnknownField {
                        cx.findings.push(unknown(path, name));
                    } else {
                        cx.findings.push(BindFinding::QualifiedReference {
                            path: path.into(),
                            name: name.into(),
                            row: cx.row().map(String::from),
                            finding: cause,
                        });
                    }
                }
                None
            } else {
                match cx.catalog.bind(name) {
                    Some(reference_binding::Binding::Dataset { field, .. }) => {
                        Some(match implicit {
                            Some(implicit) if !implicit.record() => FlagRead::Collect {
                                column: field,
                                identifier: name.into(),
                            },
                            _ => FlagRead::Read(Read::Source(field)),
                        })
                    }
                    _ => None,
                }
            }
        };
        if let Some(read) = read {
            bindings.push(FlagBinding {
                name: name.into(),
                read,
            });
        }
    }
    if implicit.is_some() {
        // The implicit template types a lookup read by its donor's capture.
        let captured = cx.captured();
        if cx.findings.len() == start {
            captured.predicate(condition, cx.findings);
        }
        if cx.findings.len() != start {
            return Ok(None);
        }
        return finish(operation, |condition| captured.typed(condition), bindings);
    }
    let catalog = cx.catalog;
    if cx.findings.len() == start {
        cx.findings.extend(
            types::predicate_kinds(condition, |name| catalog.type_of(name).map(Into::into))
                .into_iter()
                .map(BindFinding::Predicate),
        );
    }
    if cx.findings.len() != start {
        Ok(None)
    } else {
        finish(
            operation,
            |condition| {
                condition
                    .typed(|name| crate::result_type::ResultType::declared(catalog.type_of(name)))
            },
            bindings,
        )
    }
}
