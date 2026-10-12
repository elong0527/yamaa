//! R013 declarations select one of the three admitted relation contexts.
use super::*;
use crate::{
    aggregate_compiler::{compile_aggregate, AggregateCompileError, CompiledAggregate},
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        AggregateBinding, AggregatePhase, AggregateRelation, AggregateRowContext, AggregateSource,
        BoundAggregate, IntermediateKey, SecondarySource,
    },
    diagnostic::{AggregateContextCause as Cause, AggregateEmptyKey, AggregateFinding as Finding},
    reference_scope::Phase,
};
use alloc::string::ToString;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    expression: String,
    expression_path: String,
    path: String,
    compiled: Result<CompiledAggregate, AggregateCompileError>,
    groups: Vec<String>,
    keys: Option<Vec<(String, match_values::Declaration)>>,
    empty_key: Option<AggregateEmptyKey>,
    output_keys: Vec<String>,
    filter: Option<(
        String,
        Result<crate::predicate::Plan, crate::predicate_compiler::Error>,
    )>,
}
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
) -> Result<Declaration, PrepareError> {
    let scalar = matches!(d.nodes()[payload], N::Text(_));
    let expr = if scalar {
        text(d, payload)?
    } else {
        text(d, field(d, payload, "expr")?)?
    };
    let simple = scalar || mapping(d, payload)?.len() == 1;
    let expression_path = if simple {
        path.into()
    } else {
        format!("{path}.expr")
    };
    let compiled = compile_aggregate(expr, &expression_path, Default::default());
    if matches!(
        compiled,
        Err(AggregateCompileError::Parse(
            crate::aggregate_parser::ParseError::Limit { .. }
        ))
    ) {
        return Err(PrepareError::Limit("aggregate_parser"));
    }
    if matches!(compiled, Err(AggregateCompileError::MetadataLimit { .. })) {
        return Err(PrepareError::Limit("aggregate_metadata"));
    }
    let mut extra = Vec::new();
    if !scalar {
        optional_features(d, payload, &["derive", "between"], path, &mut extra);
    }
    let groups = if scalar {
        Vec::new()
    } else {
        d.field(payload, "group_by")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| {
                sequence(d, id)?
                    .iter()
                    .map(|&id| Ok(text(d, id)?.into()))
                    .collect()
            })
            .transpose()?
            .unwrap_or_default()
    };
    let output_keys = sequence(d, field(d, d.root(), "keys")?)?
        .iter()
        .map(|&id| Ok(text(d, id)?.into()))
        .collect::<Result<_, PrepareError>>()?;
    let keys = if scalar {
        None
    } else {
        d.field(payload, "key")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| {
                if matches!(&d.nodes()[id],N::Sequence(pairs) if pairs.len()>64)
                    || matches!(&d.nodes()[id],N::Mapping(pairs) if pairs.len()>64)
                {
                    return Err(PrepareError::Limit("aggregate_keys"));
                }
                let names = match &d.nodes()[id] {
                    N::Text(name) => Some(vec![name.as_str()]),
                    N::Sequence(ids) => Some(
                        ids.iter()
                            .map(|&id| text(d, id))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    _ => None,
                };
                if let Some(names) = names {
                    let mut pairs = Vec::new();
                    for name in names {
                        if !pairs.iter().any(|(prior, _)| prior == name) {
                            pairs.push((
                                name.into(),
                                match_values::Declaration::Variable(name.into()),
                            ));
                        }
                    }
                    return Ok(pairs);
                }
                mapping(d, id)?
                    .iter()
                    .map(|&(key, value)| {
                        let name = text(d, key)?;
                        Ok((
                            name.into(),
                            match_values::Declaration::prepare(
                                d,
                                value,
                                &format!("{path}.key.{name}"),
                                &[],
                                0,
                                &mut extra,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>, PrepareError>>()
            })
            .transpose()?
    };
    let empty_key = if keys.as_ref().is_some_and(Vec::is_empty) {
        Some(
            if matches!(
                d.nodes()[d.field(payload, "key").expect("retained key declaration")],
                N::Sequence(_)
            ) {
                AggregateEmptyKey::List
            } else {
                AggregateEmptyKey::Mapping
            },
        )
    } else {
        None
    };
    let filter = if scalar {
        None
    } else {
        d.field(payload, "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| {
                let expr = text(d, id)?;
                Ok((
                    expr.into(),
                    crate::predicate_compiler::compile(
                        expr,
                        &format!("{path}.filter"),
                        Default::default(),
                    ),
                ))
            })
            .transpose()?
    };
    if !extra.is_empty() {
        return Err(PrepareError::Unsupported(extra));
    }
    Ok(Declaration {
        expression: expr.into(),
        expression_path,
        path: path.into(),
        compiled,
        groups,
        keys,
        empty_key,
        output_keys,
        filter,
    })
}
fn finding(value: Finding<'_>, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
    findings.push(BindFinding::AggregateDiagnostic(
        value.diagnostic().map_err(|_| BindError::Internal)?,
    ));
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    declaration: &Declaration,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: Phase<'_>,
    output: &TableSchema,
    secondary: &[SecondarySource],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let compiled = match &declaration.compiled {
        Ok(compiled) => compiled,
        Err(AggregateCompileError::Parse(crate::aggregate_parser::ParseError::Grammar {
            failure,
            ..
        })) => {
            findings.push(BindFinding::Aggregate {
                path: declaration.expression_path.clone(),
                expression: declaration.expression.clone(),
                error: failure.clone(),
            });
            return Ok(None);
        }
        Err(AggregateCompileError::Numeric(error)) => {
            findings.push(BindFinding::Numeric {
                path: declaration.expression_path.clone(),
                expression: declaration.expression.clone(),
                error: error.clone(),
            });
            return Ok(None);
        }
        Err(_) => return Err(BindError::Internal),
    };
    let mut relations = compiled
        .identifiers()
        .filter_map(|name| {
            name.split_once('.')
                .map(|(relation, _)| relation.to_string())
        })
        .chain(compiled.record_relations().map(String::from))
        .collect::<Vec<_>>();
    relations.sort_unstable();
    relations.dedup();
    let bare = compiled.identifiers().any(|name| !name.contains('.'));
    if relations.len() > 1 || (!relations.is_empty() && bare) {
        if bare {
            relations.push("<output>".into());
        }
        finding(
            Finding::MixedRelations {
                path: &declaration.expression_path,
                expression: &declaration.expression,
                relations: &relations,
            },
            findings,
        )?;
        return Ok(None);
    }
    let relation = relations.first().map(String::as_str);
    if matches!(phase, Phase::Column { .. }) {
        if let Some(relation) = relation {
            if declaration
                .groups
                .iter()
                .any(|name| name.split_once('.').map(|(prefix, _)| prefix) != Some(relation))
            {
                findings.push(BindFinding::AggregateGroupScope {
                    path: format!("{}.group_by", declaration.path),
                    expression: declaration.expression.clone(),
                    relation: relation.into(),
                });
                return Ok(None);
            }
        }
    }
    let cause = match phase {
        Phase::Row { group_by: None } => Some((Cause::UngroupedRow, declaration.path.clone())),
        Phase::Row { group_by: Some(_) } => {
            if relation != Some(driver) {
                findings.push(BindFinding::AggregateScope {
                    path: declaration.path.clone(),
                    expression: declaration.expression.clone(),
                    relation: relation.map(String::from),
                    driver: driver.into(),
                });
                return Ok(None);
            }
            if !declaration.groups.is_empty() {
                Some((
                    Cause::RowOwnsGroups,
                    format!("{}.group_by", declaration.path),
                ))
            } else if declaration.keys.is_some() {
                Some((Cause::RowNoKeys, format!("{}.key", declaration.path)))
            } else {
                None
            }
        }
        Phase::Column { .. } if relation.is_none() => {
            if declaration.groups.is_empty() {
                Some((Cause::OutputNeedsGroups, declaration.path.clone()))
            } else if declaration.groups.iter().any(|name| name.contains('.')) {
                Some((
                    Cause::OutputGroupsQualified,
                    format!("{}.group_by", declaration.path),
                ))
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some((cause, path)) = cause {
        finding(
            Finding::InvalidContext {
                path: &path,
                expression: &declaration.expression,
                cause,
            },
            findings,
        )?;
        return Ok(None);
    }
    let before = findings.len();
    let donor = if let Some(relation) = relation {
        if relation == driver {
            AggregateSource::Primary
        } else if let Some(index) = secondary.iter().position(|source| source.name == relation) {
            AggregateSource::Secondary(index)
        } else {
            findings.push(BindFinding::UnknownReference {
                path: declaration.expression_path.clone(),
                name: format!("{relation}.*"),
            });
            return Ok(None);
        }
    } else {
        AggregateSource::Primary
    };
    let resolve = |name: &str,
                   path: &str,
                   findings: &mut Vec<BindFinding>|
     -> Result<Option<(usize, ColumnType)>, BindError> {
        let bound = match (relation, donor) {
            (None, _) => match catalog.bind(name).map_err(BindError::Catalog)? {
                Some(reference_binding::Binding::Output {
                    column,
                    column_type,
                }) => Some((column, column_type)),
                _ => None,
            },
            (Some(relation), AggregateSource::Secondary(index)) => name
                .strip_prefix(relation)
                .and_then(|tail| tail.strip_prefix('.'))
                .and_then(|field| {
                    secondary[index]
                        .schema
                        .columns()
                        .iter()
                        .enumerate()
                        .find(|(_, column)| column.name == field)
                        .map(|(index, column)| (index, column.kind))
                }),
            (Some(_), AggregateSource::Primary) => {
                match catalog.bind(name).map_err(BindError::Catalog)? {
                    Some(reference_binding::Binding::Dataset {
                        field, column_type, ..
                    }) => Some((field, column_type)),
                    _ => None,
                }
            }
        };
        if bound.is_none() {
            findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.into(),
            });
        }
        Ok(bound)
    };
    let grain = match phase {
        Phase::Row {
            group_by: Some(groups),
        } => groups.to_vec(),
        _ => declaration.groups.iter().map(String::as_str).collect(),
    };
    // The enclosing row already reports invalid group fields at its authored
    // declaration. Its failed grain cannot grant an aggregate relation, and
    // must not be reported again under an invented aggregate.group_by path.
    if matches!(phase, Phase::Row { .. })
        && grain.iter().any(|name| {
            !catalog.bind(name).is_ok_and(|binding| {
                matches!(binding, Some(reference_binding::Binding::Dataset { .. }))
            })
        })
    {
        return Ok(None);
    }
    for identifier in compiled.ungrouped_identifiers() {
        if !grain.contains(&identifier) {
            finding(
                Finding::IdentifierNotGrouped {
                    path: &declaration.expression_path,
                    expression: &declaration.expression,
                    dataset: relation,
                    identifier,
                },
                findings,
            )?;
        }
    }
    let mut group_columns = Vec::new();
    for (index, name) in grain.iter().enumerate() {
        if let Some((column, _)) = resolve(
            name,
            &format!("{}.group_by[{index}]", declaration.path),
            findings,
        )? {
            group_columns.push(column);
            if relation.is_none() {
                edges.push(column);
            }
        }
    }
    let mut bindings = Vec::new();
    for name in compiled.identifiers() {
        if let Some((column, _)) = resolve(name, &declaration.expression_path, findings)? {
            bindings.push(AggregateBinding {
                name: name.into(),
                column,
            });
            if relation.is_none() {
                edges.push(column);
            }
        }
    }
    let filter = match &declaration.filter {
        Some((_, Ok(plan))) => {
            let mut bound = Vec::new();
            for name in plan.identifiers() {
                if let Some((column, _)) = resolve(name, plan.spec_path(), findings)? {
                    bound.push(Binding {
                        name: name.into(),
                        read: Read::Source(column),
                    });
                    if relation.is_none() {
                        edges.push(column);
                    }
                }
            }
            if findings.len() == before {
                Some(
                    BoundPredicate::new(plan.clone(), bound)
                        .map_err(BindError::InvalidPredicateBinding)?,
                )
            } else {
                None
            }
        }
        Some((
            expr,
            Err(crate::predicate_compiler::Error::Parse(
                crate::predicate_parser::ParseError::Grammar {
                    position, failure, ..
                },
            )),
        )) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                &format!("{}.filter", declaration.path),
                expr,
                position.character,
                failure,
            )?));
            None
        }
        Some((_, Err(error))) => return Err(BindError::PredicatePolicy(error.clone())),
        None => None,
    };
    let bound_relation = match phase {
        Phase::Row { .. } => AggregateRelation::CurrentGroup,
        Phase::Column { .. } if relation.is_none() => AggregateRelation::Output {
            group_by: group_columns,
        },
        Phase::Column { .. } => {
            let relation = relation.expect("qualified context");
            let inferred = declaration.keys.is_none();
            let defaults = declaration
                .output_keys
                .iter()
                .filter(|name| {
                    let written = format!("{relation}.{name}");
                    match donor {
                        AggregateSource::Primary => {
                            catalog.bind(&written).is_ok_and(|value| value.is_some())
                        }
                        AggregateSource::Secondary(index) => secondary[index]
                            .schema
                            .columns()
                            .iter()
                            .any(|column| column.name == **name),
                    }
                })
                .map(|name| {
                    (
                        name.clone(),
                        match_values::Declaration::Variable(name.clone()),
                    )
                })
                .collect::<Vec<_>>();
            let pairs = declaration.keys.as_ref().unwrap_or(&defaults);
            if pairs.is_empty() {
                if let Some(key) = declaration.empty_key {
                    finding(
                        Finding::MissingKeys {
                            path: &declaration.path,
                            dataset: relation,
                            key,
                        },
                        findings,
                    )?;
                    return Ok(None);
                }
                finding(
                    Finding::NoApplicableKeys {
                        path: &declaration.path,
                        dataset: relation,
                        keys: &declaration.output_keys,
                    },
                    findings,
                )?;
                return Ok(None);
            }
            let mut keys = Vec::new();
            for (field, value) in pairs {
                let written = format!("{relation}.{field}");
                let donor_field = match donor {
                    AggregateSource::Primary => match catalog
                        .bind(&written)
                        .map_err(BindError::Catalog)?
                    {
                        Some(reference_binding::Binding::Dataset {
                            field, column_type, ..
                        }) => Some((field, column_type)),
                        _ => None,
                    },
                    AggregateSource::Secondary(index) => secondary[index]
                        .schema
                        .columns()
                        .iter()
                        .enumerate()
                        .find(|(_, column)| column.name == *field)
                        .map(|(index, column)| (index, column.kind)),
                };
                let Some((source_column, actual)) = donor_field else {
                    finding(
                        Finding::KeyReference {
                            path: &declaration.expression_path,
                            identifier: &written,
                        },
                        findings,
                    )?;
                    continue;
                };
                if let Some(expected) = value.result_type(catalog)? {
                    if !crate::key_relation::comparable(expected, actual.into()) {
                        finding(
                            Finding::KeyType {
                                path: &declaration.expression_path,
                                source: &value.display_name(field),
                                expected,
                                actual,
                                inferred,
                            },
                            findings,
                        )?;
                    }
                }
                if let Some(reference) = value.reference() {
                    if catalog
                        .bind(reference)
                        .map_err(BindError::Catalog)?
                        .is_none()
                    {
                        finding(
                            Finding::KeyReference {
                                path: &declaration.expression_path,
                                identifier: reference,
                            },
                            findings,
                        )?;
                        continue;
                    }
                }
                let bound = value.bind(
                    &format!("{}.key.{field}", declaration.path),
                    relation,
                    catalog,
                    findings,
                )?;
                if let Some(value) = bound {
                    if let Some(column) = value
                        .read()
                        .and_then(crate::dataset::MatchRead::output_column)
                    {
                        edges.push(column);
                    }
                    keys.push(IntermediateKey {
                        source_column,
                        path: format!("{}.key.{field}", declaration.path),
                        value,
                    });
                }
            }
            AggregateRelation::Matched {
                source: donor,
                keys,
                group_by: group_columns,
            }
        }
    };
    if findings.len() != before {
        return Ok(None);
    }
    let row_context = if let Phase::Row {
        group_by: Some(names),
    } = phase
    {
        Some(AggregateRowContext {
            row: row.ok_or(BindError::Internal)?.into(),
            groups: names
                .iter()
                .zip(grain.iter())
                .map(|(name, _)| {
                    resolve(name, &declaration.path, findings).map(|value| {
                        (
                            name.split_once('.')
                                .map_or(*name, |(_, field)| field)
                                .to_string(),
                            value.expect("bound enclosing group").0,
                        )
                    })
                })
                .collect::<Result<_, _>>()?,
        })
    } else {
        None
    };
    let bound = BoundAggregate::new(
        compiled.clone(),
        bound_relation,
        bindings,
        filter,
        if matches!(phase, Phase::Row { .. }) {
            AggregatePhase::RowConstruction
        } else {
            AggregatePhase::Derivation
        },
        row_context,
        &declaration.path,
    )
    .map_err(|_| BindError::Internal)?;
    let _ = output;
    Ok(Some(Expression::Aggregate(alloc::boxed::Box::new(bound))))
}
