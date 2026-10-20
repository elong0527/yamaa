//! Authored ODM declarations select fixed source admission before study capture.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{AggregatePhase, BoundOdm, Expression, OdmRead, OdmScope},
    reference_scope::Phase,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    declared: bool,
    dataset: String,
    item: String,
    item_oid: String,
    path: String,
    /// Whether the read is a row template's own derivation.
    row: bool,
    selectors: [Option<Vec<String>>; 3],
    filter: Option<(String, Result<CompiledExpression, CompileError>)>,
}

/// `record_rows` makes each row of the implicit template one base record,
/// whose item group the read takes its scope from, as an ungrouped row
/// template does.
pub(super) fn bind(
    declaration: &Declaration,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: Phase<'_>,
    record_rows: bool,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    if !declaration.declared {
        findings.push(BindFinding::UnknownReference {
            path: format!("{}.item", declaration.path),
            name: declaration.item.clone(),
        });
        return Ok(None);
    }
    if declaration.dataset != driver && !matches!(phase, Phase::Column { .. }) {
        findings.push(BindFinding::Odm(
            crate::diagnostic::OdmFinding::InvalidContext {
                path: &declaration.path,
                dataset: &declaration.dataset,
                location: if declaration.row {
                    crate::diagnostic::OdmLocation::Row
                } else {
                    crate::diagnostic::OdmLocation::Column
                },
            }
            .diagnostic(),
        ));
        return Ok(None);
    }
    let before = findings.len();
    let filter = if let Some((expression, compiled)) = &declaration.filter {
        let plan = match compiled {
            Ok(plan) => plan,
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Predicate(failure.diagnostic(
                    ExpressionContext::Predicate,
                    &format!("{}.filter", declaration.path),
                    expression,
                    position.character,
                )));
                return Ok(None);
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
        };
        let mut bindings = Vec::new();
        let mut seen = BTreeSet::new();
        for identifier in plan.identifiers() {
            if !seen.insert(identifier) {
                continue;
            }
            if identifier
                .split_once('.')
                .is_none_or(|(dataset, _)| dataset != declaration.dataset)
            {
                findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                    crate::diagnostic::ConditionCode::SourceFilterReference,
                    plan.spec_path(),
                    &[
                        ("identifier", identifier),
                        ("dataset", &declaration.dataset),
                    ],
                )));
                continue;
            }
            match catalog.bind(identifier) {
                Some(reference_binding::Binding::Dataset { field, .. }) => bindings.push(Binding {
                    name: identifier.into(),
                    read: Read::Source(field),
                }),
                _ => findings.push(BindFinding::UnknownReference {
                    path: plan.spec_path().into(),
                    name: identifier.into(),
                }),
            }
        }
        if findings.len() == before {
            let typed =
                plan.typed(|name| crate::result_type::ResultType::declared(catalog.type_of(name)));
            Some(BoundPredicate::new(typed, bindings).map_err(BindError::InvalidPredicateBinding)?)
        } else {
            None
        }
    } else {
        None
    };
    if findings.len() != before {
        return Ok(None);
    }
    let scope = match phase {
        Phase::Row {
            group_by: Some(groups),
        } => OdmScope::Groups(
            crate::odm_source::HIERARCHY
                .into_iter()
                .filter(|&column| {
                    groups.iter().any(|name| {
                        *name
                            == format!(
                                "{}.{}",
                                declaration.dataset,
                                crate::odm_source::FIELDS[column]
                            )
                    })
                })
                .collect(),
        ),
        Phase::Row { group_by: None } => OdmScope::Records,
        Phase::Column { .. } if record_rows => OdmScope::Records,
        Phase::Column { .. } => OdmScope::Keys,
    };
    let read = OdmRead {
        dataset: declaration.dataset.clone(),
        item: declaration.item.clone(),
        item_oid: declaration.item_oid.clone(),
        path: declaration.path.clone(),
        row: row.map(String::from),
        phase: if declaration.row {
            AggregatePhase::RowConstruction
        } else {
            AggregatePhase::Derivation
        },
        scope,
        source: if matches!(phase, Phase::Column { .. }) {
            let reference = format!("{}.ItemOID", declaration.dataset);
            let Some(reference_binding::Binding::Dataset { dataset, .. }) =
                catalog.bind(&reference)
            else {
                return Err(BindError::Internal);
            };
            Some(if dataset == 0 {
                crate::dataset::AggregateSource::Primary
            } else {
                crate::dataset::AggregateSource::Secondary(dataset - 1)
            })
        } else {
            None
        },
        selectors: declaration.selectors.clone(),
        filter,
    };
    BoundOdm::new(read)
        .map(|bound| Some(Expression::Odm(std::boxed::Box::new(bound))))
        .map_err(BindError::InvalidPlan)
}

impl Declaration {
    /// The `odm` read at `e`: its item, whether an input declares the item's
    /// dataset, its selectors and filter.
    pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Self, PrepareError> {
        let ExprKind::Odm(read) = &spec.expr(e).kind else {
            return Err(PrepareError::Internal);
        };
        let item = checks::name(spec, read.item).ok_or(PrepareError::Internal)?;
        let (dataset, item_oid) = item.split_once('.').ok_or(PrepareError::Internal)?;
        let selector = |list: &Option<Vec<ast::Text>>| {
            list.as_ref()
                .map(|list| list.iter().map(|value| value.text.clone()).collect())
        };
        let filter = read
            .filter
            .map(|filter| match &spec.expr(filter).kind {
                ExprKind::Formula(formula) => {
                    Ok((formula.text.text.clone(), formula.compiled.clone()))
                }
                _ => Err(PrepareError::Internal),
            })
            .transpose()?;
        Ok(Self {
            declared: spec.inputs.iter().any(|input| input.name.name == dataset),
            dataset: dataset.into(),
            item: item.into(),
            item_oid: item_oid.into(),
            path: checks::path(spec, e),
            row: checks::under(spec, spec.expr(e).meta, "rows"),
            selectors: [
                selector(&read.event),
                selector(&read.form),
                selector(&read.item_group),
            ],
            filter,
        })
    }
}
