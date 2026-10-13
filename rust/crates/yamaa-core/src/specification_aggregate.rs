//! R013 declarations select one of the three admitted relation contexts.
use super::*;
use crate::value::Value;
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
use alloc::collections::BTreeSet;
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
    derive: Option<Vec<DeriveDeclaration>>,
    derive_relations: Vec<String>,
    between: Option<RangeDeclaration>,
}
#[derive(Clone, Debug)]
struct DeriveDeclaration {
    name: String,
    kind: ColumnType,
    branch: cases::Branch,
    handler: Option<crate::conversion::LiteralHandler>,
    path: String,
    intermediate_names: Vec<String>,
}
#[derive(Clone, Debug)]
struct RangeDeclaration {
    value: String,
    lower: Option<String>,
    upper: Option<String>,
}
impl Declaration {
    pub(super) fn calls(&self, result: &mut Vec<usize>) {
        for derive in self.derive.iter().flatten() {
            derivations::calls(&derive.branch.operation, result);
        }
    }
}
fn diagnostic(
    code: crate::diagnostic::ConditionCode,
    path: &str,
    fields: &[(&str, &str)],
) -> crate::diagnostic::Diagnostic {
    let mut value = LookupFinding::new(path, code);
    for (field, text) in fields {
        value.text(field, text);
    }
    value.diagnostic()
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    project: Option<&functions::Prepared>,
    named: &intermediates::Declarations,
    depth: usize,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
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
    let mut derive_relations = Vec::<String>::new();
    let derive = if scalar {
        None
    } else {
        d.field(payload, "derive")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| {
                let items = sequence(d, id)?;
                if items.len() > 4096 {
                    return Err(PrepareError::Limit("aggregate_derive_bindings"));
                }
                let mut bindings = Vec::<DeriveDeclaration>::new();
                let invalid = |path: &str, reason: &str| {
                    PrepareError::Invalid(vec![PreflightFinding::Scalar(diagnostic(
                        crate::diagnostic::ConditionCode::AggregateDeriveDeclaration,
                        path,
                        &[("reason", reason)],
                    ))])
                };
                if items.is_empty() {
                    return Err(invalid(
                        &format!("{path}.derive"),
                        "a derive step is a non-empty binding list",
                    ));
                }
                for (index, &item) in items.iter().enumerate() {
                    let binding_path = format!("{path}.derive[{index}]");
                    let name = text(d, field(d, item, "name")?)?;
                    if bindings.iter().any(|binding| binding.name == name) {
                        return Err(invalid(
                            &binding_path,
                            "derive binding names are unique identifiers",
                        ));
                    }
                    let kind = match text(d, field(d, item, "type")?)? {
                        "str" => ColumnType::Str,
                        "int" => ColumnType::Int,
                        "float" => ColumnType::Float,
                        "date" => ColumnType::Date,
                        "datetime" => ColumnType::DateTime,
                        _ => return Err(PrepareError::Internal),
                    };
                    let derivation = field(d, item, "derivation")?;
                    let value = field(d, derivation, "value")?;
                    let mut references = Vec::new();
                    let mut intermediate_names = Vec::<String>::new();
                    derivations::current_references(d, value, project, &mut references, 0)?;
                    for reference in references {
                        if let Some((relation, _)) = reference.split_once('.') {
                            if !named.contains(relation) {
                                derive_relations.push(relation.into());
                            } else {
                                intermediate_names.push(relation.into());
                            }
                        } else if !bindings.iter().any(|binding| binding.name == reference) {
                            return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                                diagnostic(
                                    crate::diagnostic::ConditionCode::AggregateDeriveVariable,
                                    &binding_path,
                                    &[("variable", &reference), ("binding", name)],
                                ),
                            )]));
                        }
                    }
                    if let Some(qualifier) = intermediate_names.iter().find(|name| !named.kept(name)) {
                        return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(diagnostic(
                            crate::diagnostic::ConditionCode::AggregateDeriveIntermediate,
                            &format!("{path}.derive"),
                            &[("reason", "a derive step reads only keep-declared named intermediates"), ("intermediate", qualifier)],
                        ))]));
                    }
                    let branch = derivations::prepare_value(
                        d,
                        value,
                        &format!("{binding_path}.derivation"),
                        project,
                        named,
                        true,
                        depth + 1,
                        extraction,
                        templates,
                    )?;
                    bindings.push(DeriveDeclaration {
                        name: name.into(),
                        kind,
                        branch,
                        handler: literal_handler(
                            d,
                            derivation,
                            &format!("{binding_path}.derivation"),
                        )?,
                        path: binding_path,
                        intermediate_names,
                    });
                }
                Ok(bindings)
            })
            .transpose()?
    };
    let between = if scalar {
        None
    } else {
        d.field(payload, "between")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| {
                let bound = |name| {
                    d.field(id, name)
                        .filter(|&id| !matches!(d.nodes()[id], N::Null))
                        .map(|id| text(d, id).map(String::from))
                        .transpose()
                };
                Ok::<_, PrepareError>(RangeDeclaration {
                    value: text(d, field(d, id, "value")?)?.into(),
                    lower: bound("lower")?,
                    upper: bound("upper")?,
                })
            })
            .transpose()?
    };
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
    if derive.is_some() {
        if let Some((_, Ok(filter))) = &filter {
            derive_relations.extend(
                filter
                    .identifiers()
                    .into_iter()
                    .filter_map(|name| {
                        name.split_once('.')
                            .map(|(relation, _)| String::from(relation))
                    })
                    .filter(|relation| !named.contains(relation)),
            );
        }
        derive_relations.sort_unstable();
        derive_relations.dedup();
        if derive_relations.len() != 1 {
            let mut value = diagnostic(
                crate::diagnostic::ConditionCode::AggregateDeriveRelation,
                &format!("{path}.derive"),
                &[("reason", "a derive step names exactly one relation")],
            );
            value.context.insert(
                "relations".into(),
                crate::diagnostic::ContextValue::Sequence(
                    derive_relations
                        .iter()
                        .map(|relation| {
                            crate::diagnostic::ContextValue::Scalar(Value::Str(relation.clone()))
                        })
                        .collect(),
                ),
            );
            return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(value)]));
        }
    }
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
        derive,
        derive_relations,
        between,
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
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    project: Option<&crate::project_calls::ProjectCalls>,
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
    let relation = if declaration.derive.is_some() {
        declaration.derive_relations.first().map(String::as_str)
    } else {
        relations.first().map(String::as_str)
    };
    if let Some(derive) = &declaration.derive {
        let before_derive = findings.len();
        for name in compiled.identifiers() {
            if !name.contains('.') && !derive.iter().any(|binding| binding.name == name) {
                findings.push(BindFinding::AggregateDiagnostic(diagnostic(
                    crate::diagnostic::ConditionCode::AggregateDeriveVariable,
                    &declaration.expression_path,
                    &[("variable", name), ("expr", &declaration.expression)],
                )));
            }
        }
        for binding in derive {
            for qualifier in &binding.intermediate_names {
                let item = intermediates
                    .iter()
                    .flatten()
                    .find(|item| item.identifier == *qualifier);
                if item.is_none_or(|item| item.selection.is_none()) {
                    findings.push(BindFinding::AggregateDiagnostic(diagnostic(
                        crate::diagnostic::ConditionCode::AggregateDeriveIntermediate,
                        &format!("{}.derive", declaration.path),
                        &[
                            (
                                "reason",
                                "a derive step reads only keep-declared named intermediates",
                            ),
                            ("intermediate", qualifier),
                        ],
                    )));
                }
            }
        }
        if matches!(phase, Phase::Row { .. }) {
            findings.push(BindFinding::AggregateDiagnostic(diagnostic(
                crate::diagnostic::ConditionCode::AggregateDeriveDeclaration,
                &format!("{}.derive", declaration.path),
                &[(
                    "reason",
                    "derive binds records of a qualified right side only",
                )],
            )));
        }
        if findings.len() != before_derive {
            return Ok(None);
        }
    }
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
            if declaration.between.is_some() {
                Some((
                    Cause::RowNoRightSide,
                    format!("{}.between", declaration.path),
                ))
            } else if !declaration.groups.is_empty() {
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
            if declaration.between.is_some() {
                Some((
                    Cause::OutputBetween,
                    format!("{}.between", declaration.path),
                ))
            } else if declaration.groups.is_empty() {
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
    let donor_schema = match (relation, donor) {
        (Some(_), AggregateSource::Secondary(index)) => secondary[index].schema.clone(),
        (Some(relation), AggregateSource::Primary) => TableSchema::new(
            catalog
                .fields(relation)
                .ok_or(BindError::Internal)?
                .iter()
                .map(|field| crate::table::Column {
                    name: field.name.into(),
                    kind: field.column_type,
                })
                .collect(),
        )
        .map_err(|_| BindError::Internal)?,
        (None, _) => output.clone(),
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
    let mut seen = alloc::collections::BTreeSet::new();
    for name in compiled.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        if let Some(index) = declaration
            .derive
            .as_ref()
            .and_then(|derive| derive.iter().position(|binding| binding.name == name))
        {
            bindings.push(AggregateBinding {
                name: name.into(),
                column: donor_schema.columns().len() + index,
            });
            continue;
        }
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
            let mut seen = alloc::collections::BTreeSet::new();
            for name in plan.identifiers() {
                if !seen.insert(name) {
                    continue;
                }
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
    let range = bind_range(
        declaration,
        relation,
        catalog,
        &donor_schema,
        secondary,
        intermediates,
        edges,
        findings,
    )?;
    let (derivations, derived_schema) = bind_derivations(
        declaration,
        relation,
        &donor_schema,
        output,
        named,
        secondary,
        intermediates,
        project,
        edges,
        findings,
    )?;
    if findings.len() != before {
        return Ok(None);
    }
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
    .map_err(|_| BindError::Internal)?
    .with_record_operations(range, derivations, derived_schema);
    let _ = output;
    Ok(Some(Expression::Aggregate(alloc::boxed::Box::new(bound))))
}

#[allow(clippy::too_many_arguments)]
fn bind_range(
    declaration: &Declaration,
    relation: Option<&str>,
    catalog: &Catalog,
    donor: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<crate::dataset::RelationRange>, BindError> {
    use crate::dataset::{
        CurrentField, CurrentRead, RelationRange, RelationRangeBound, RelationRangeError,
    };
    use crate::diagnostic::ConditionCode as C;
    let Some(range) = &declaration.between else {
        return Ok(None);
    };
    let path = format!("{}.between", declaration.path);
    if range.lower.is_none() && range.upper.is_none() {
        findings.push(BindFinding::AggregateDiagnostic(diagnostic(
            C::AggregateDriverScope,
            &path,
            &[
                ("expr", &declaration.expression),
                ("reason", "between declares at least one bound"),
            ],
        )));
        return Ok(None);
    }
    let before = findings.len();
    let mut bound = |name: &Option<String>| {
        name.as_ref().and_then(|name| {
            donor
                .columns()
                .iter()
                .enumerate()
                .find(|(_, column)| column.name == *name)
                .map(|(column, field)| RelationRangeBound {
                    column,
                    identifier: field.name.clone(),
                    kind: Some(field.kind.into()),
                })
                .or_else(|| {
                    findings.push(BindFinding::AggregateDiagnostic(diagnostic(
                        C::IntermediateRangeReference,
                        &path,
                        &[("identifier", name)],
                    )));
                    None
                })
        })
    };
    let lower = bound(&range.lower);
    let upper = bound(&range.upper);
    if findings.len() != before {
        return Ok(None);
    }
    let current = match catalog.bind(&range.value).map_err(BindError::Catalog)? {
        Some(reference_binding::Binding::Output {
            column,
            column_type,
        }) => {
            edges.push(column);
            Some((CurrentRead::Output(column), column_type))
        }
        Some(reference_binding::Binding::Dataset {
            field, column_type, ..
        }) => Some((
            CurrentRead::Record {
                source: 0,
                column: field,
            },
            column_type,
        )),
        None => range.value.split_once('.').and_then(|(name, field)| {
            secondary
                .iter()
                .enumerate()
                .find(|(_, source)| source.name == name)
                .and_then(|(source, item)| {
                    item.schema
                        .columns()
                        .iter()
                        .enumerate()
                        .find(|(_, column)| column.name == field)
                        .map(|(column, field)| {
                            (
                                CurrentRead::Record {
                                    source: source + 1,
                                    column,
                                },
                                field.kind,
                            )
                        })
                })
                .or_else(|| {
                    intermediates.iter().enumerate().find_map(|(index, item)| {
                        let item = item.as_ref()?;
                        if item.identifier != name {
                            return None;
                        }
                        let column = item.source_column(secondary, field)?;
                        edges.extend(row_intermediates::dependencies(index, intermediates).ok()?);
                        Some((
                            CurrentRead::Intermediate { index, column },
                            item.source_schema(secondary)?.columns()[column].kind,
                        ))
                    })
                })
        }),
    };
    let Some((read, kind)) = current else {
        findings.push(BindFinding::AggregateDiagnostic(diagnostic(
            C::IntermediateRangeReference,
            &format!("{path}.value"),
            &[("identifier", &range.value)],
        )));
        return Ok(None);
    };
    let _ = relation;
    match RelationRange::new(
        path.clone(),
        CurrentField {
            read,
            identifier: range.value.clone(),
        },
        Some(kind.into()),
        lower,
        upper,
        16_777_216,
    ) {
        Ok(range) => Ok(Some(range)),
        Err(RelationRangeError::Incomparable(types)) => {
            findings.push(BindFinding::AggregateDiagnostic(
                types.diagnostic_for(&path, None),
            ));
            Ok(None)
        }
        Err(RelationRangeError::Limit) => {
            Err(BindError::Catalog(reference_binding::Error::Limit {
                resource: "aggregate_range_metadata",
                limit: 16_777_216,
                required: usize::MAX,
            }))
        }
        Err(_) => Err(BindError::Internal),
    }
}
#[allow(clippy::too_many_arguments)]
fn bind_derivations(
    declaration: &Declaration,
    relation: Option<&str>,
    donor: &TableSchema,
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    project: Option<&crate::project_calls::ProjectCalls>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<
    (
        Vec<crate::dataset::AggregateDerivation>,
        Option<TableSchema>,
    ),
    BindError,
> {
    let Some(derive) = &declaration.derive else {
        return Ok((Vec::new(), None));
    };
    let relation = relation.ok_or(BindError::Internal)?;
    let width = output.columns().len();
    let mut derived_columns = donor.columns().to_vec();
    let mut output_columns = output.columns().to_vec();
    for (index, binding) in derive.iter().enumerate() {
        let mut name = format!("{}.derive[{index}].{}", declaration.path, binding.name);
        while derived_columns
            .iter()
            .chain(&output_columns)
            .any(|column| column.name == name)
        {
            name.push('$');
        }
        let column = crate::table::Column {
            name,
            kind: binding.kind,
        };
        derived_columns.push(column.clone());
        output_columns.push(column);
    }
    let derived_schema = TableSchema::new(derived_columns).map_err(|_| BindError::Internal)?;
    let derive_output = TableSchema::new(output_columns).map_err(|_| BindError::Internal)?;
    let mut output_fields = output
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            (
                if derive.iter().any(|binding| binding.name == column.name) {
                    format!("$current-output[{index}]")
                } else {
                    column.name.clone()
                },
                column.kind,
            )
        })
        .collect::<Vec<_>>();
    output_fields.extend(
        derive
            .iter()
            .map(|binding| (binding.name.clone(), binding.kind)),
    );
    let output_fields = output_fields
        .iter()
        .map(|(name, column_type)| reference_binding::Field {
            name,
            column_type: *column_type,
        })
        .collect::<Vec<_>>();
    let source_fields = donor
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    let catalog = Catalog::compile(
        &output_fields,
        &[reference_binding::Dataset {
            name: relation,
            fields: &source_fields,
        }],
        Default::default(),
    )
    .map_err(BindError::Catalog)?;
    let references = derive
        .iter()
        .flat_map(|binding| &binding.intermediate_names)
        .collect::<BTreeSet<_>>();
    let mut selected = Vec::new();
    for (index, item) in intermediates.iter().enumerate() {
        let Some(item) = item else {
            continue;
        };
        if !references.contains(&item.identifier) {
            continue;
        }
        let schema = item.source_schema(secondary).ok_or(BindError::Internal)?;
        let fields = schema
            .columns()
            .iter()
            .enumerate()
            .filter(|(_, field)| item.source_column(secondary, &field.name).is_some())
            .map(|(column, field)| (field.name.clone(), (column, field.kind)))
            .collect();
        selected.push((
            item.identifier.clone(),
            reference_binding::SelectedRelation {
                index,
                fields,
                dependencies: row_intermediates::dependencies(index, intermediates)?,
            },
        ));
    }
    let catalog = catalog
        .with_selected(selected)
        .map_err(BindError::Catalog)?;
    let mut bound = Vec::new();
    for (index, binding) in derive.iter().enumerate() {
        let available = (0..width + index).collect::<Vec<_>>();
        let mut dependencies = Vec::new();
        if let Some(expression) = derivations::bind_row(
            &binding.branch.operation,
            &binding.branch.path,
            width + index,
            &catalog,
            relation,
            None,
            Phase::Row { group_by: None },
            Some(&available),
            &mut dependencies,
            findings,
            false,
            &derive_output,
            project,
            named,
            secondary,
            intermediates,
        )? {
            edges.extend(dependencies.into_iter().filter(|&column| column < width));
            bound.push(crate::dataset::AggregateDerivation {
                assignment: crate::dataset::Assignment {
                    column: width + index,
                    expression,
                    path: binding.branch.path.clone(),
                },
                kind: binding.kind,
                handler: binding.handler.clone(),
                path: binding.path.clone(),
            });
        }
    }
    Ok((bound, Some(derived_schema)))
}

impl Declaration {
    pub(super) fn check_findings(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        match &self.compiled {
            Ok(_) => (),
            Err(AggregateCompileError::Parse(crate::aggregate_parser::ParseError::Grammar {
                failure,
                ..
            })) => {
                findings.push(BindFinding::Aggregate {
                    path: self.expression_path.clone(),
                    expression: self.expression.clone(),
                    error: failure.clone(),
                });
            }
            Err(AggregateCompileError::Numeric(error)) => {
                findings.push(BindFinding::Numeric {
                    path: self.expression_path.clone(),
                    expression: self.expression.clone(),
                    error: error.clone(),
                });
            }
            Err(_) => return Err(BindError::Internal),
        }
        if let Some((expression, compiled)) = &self.filter {
            static_checks::predicate(
                compiled,
                expression,
                &format!("{}.filter", self.path),
                findings,
            )?;
        }
        for declaration in self.derive.iter().flatten() {
            static_checks::branch(&declaration.branch, findings)?;
        }
        Ok(())
    }
}
