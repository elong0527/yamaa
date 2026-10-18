//! R013 declarations select one of the three admitted relation contexts.
use super::*;
use crate::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        AggregateBinding, AggregatePhase, AggregateRelation, AggregateRowContext, AggregateSource,
        BoundAggregate, IntermediateKey, SecondarySource,
    },
    diagnostic::{AggregateContextCause as Cause, AggregateFinding as Finding},
    reference_scope::Phase,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    expression: String,
    expression_path: String,
    path: String,
    compiled: Result<CompiledExpression, CompileError>,
    groups: Vec<String>,
    keys: Option<Vec<(String, match_values::Declaration)>>,
    /// A written `key` that names no pair (LOOKUP-5).
    empty_key: bool,
    output_keys: Vec<String>,
    filter: Option<(String, Result<CompiledExpression, CompileError>)>,
    derive: Option<Vec<DeriveDeclaration>>,
    derive_relations: Vec<String>,
    derive_references: Vec<String>,
    between: Option<lookup_range::Declaration>,
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
impl Declaration {
    pub(super) fn calls(&self, result: &mut Vec<usize>) {
        for derive in self.derive.iter().flatten() {
            derivations::calls(&derive.branch.operation, result);
        }
    }
    /// VALUE-24 for each derive binding over the types `input` declares, an
    /// earlier binding naming its declared type, as binding checks them.
    pub(super) fn derive_conversion_findings(
        &self,
        input: &dyn Fn(&str) -> crate::result_type::ResultType,
        findings: &mut Vec<BindFinding>,
    ) {
        let Some(derive) = &self.derive else {
            return;
        };
        for (index, binding) in derive.iter().enumerate() {
            if binding.handler.is_some() {
                continue;
            }
            let earlier = &derive[..index];
            let binding_input = |name: &str| {
                earlier
                    .iter()
                    .find(|earlier| earlier.name == name)
                    .map_or_else(|| input(name), |earlier| earlier.kind.into())
            };
            findings.extend(
                result_types::conversion_finding(
                    &binding.path,
                    binding.branch.operation.result_type(&binding_input),
                    binding.kind,
                )
                .map(BindFinding::Scalar),
            );
        }
    }
    /// EXPR-14 by FORMULA-23 over the relation's fields, a derive binding
    /// naming its declared type.
    pub(super) fn result_type(
        &self,
        input: &dyn Fn(&str) -> crate::result_type::ResultType,
    ) -> crate::result_type::ResultType {
        let Ok(compiled) = &self.compiled else {
            return crate::result_type::ResultType::UNKNOWN;
        };
        compiled.result_type(|name| {
            self.derive
                .iter()
                .flatten()
                .find(|binding| binding.name == name)
                .map_or_else(|| input(name), |binding| binding.kind.into())
        })
    }
    /// The range whose value binding reads for the current row: a qualified
    /// aggregate's range that states a bound. Binding reports every other
    /// form of `between` before it reads the value (LOOKUP-9).
    pub(super) fn current_range(&self) -> Option<&lookup_range::Declaration> {
        let range = self.between.as_ref()?;
        let qualified = self.compiled.as_ref().is_ok_and(|compiled| {
            if self.derive.is_some() {
                !self.derive_relations.is_empty()
            } else {
                compiled.identifiers().any(|name| name.contains('.'))
                    || compiled.record_relations().next().is_some()
            }
        });
        (qualified && range.donor_fields().next().is_some()).then_some(range)
    }
    /// The derivation of each derive binding, in declaration order.
    pub(super) fn derive_operations(&self) -> impl Iterator<Item = &Operation> {
        self.derive
            .iter()
            .flatten()
            .map(|binding| binding.branch.operation.as_ref())
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
/// Per-record aggregate bindings have no relation-wide window context.
fn window_path(
    d: &Document,
    value: usize,
    path: &str,
    depth: usize,
) -> Result<Option<String>, PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            Some(path),
        ));
    }

    let &[(operation, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let operation = text(d, operation)?;
    let path = format!("{path}.{operation}");
    match operation {
        "row_number" | "rank" | "row_value" | "locf" | "baseline_flag" => {
            return Ok(Some(path));
        }
        "case" => {
            for (index, &branch) in sequence(d, payload)?.iter().enumerate() {
                for name in ["then", "otherwise"] {
                    if let Some(child) = d.field(branch, name) {
                        if let Some(path) =
                            window_path(d, child, &format!("{path}[{index}].{name}"), depth + 1)?
                        {
                            return Ok(Some(path));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(None)
}
#[allow(clippy::too_many_arguments)]
fn prepare_keys(
    d: &Document,
    payload: usize,
    path: &str,
    project: Option<&functions::Prepared>,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Option<Vec<(String, match_values::Declaration)>>, PrepareError> {
    d.field(payload, "key")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| {
            if matches!(&d.nodes()[id],N::Sequence(pairs) if pairs.len()>64)
                || matches!(&d.nodes()[id],N::Mapping(pairs) if pairs.len()>64)
            {
                return Err(PrepareError::limit("aggregate_keys", 64, None, None));
            }
            let names = match &d.nodes()[id] {
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
                            project,
                            extraction,
                            templates,
                            extra,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, PrepareError>>()
        })
        .transpose()
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
    let expr = text(d, field(d, payload, "expr")?)?;
    // A written long form never holds only `expr` (SCHEMA-15), so such a
    // mapping is the expanded shorthand and its findings name the operation.
    let simple = mapping(d, payload)?.len() == 1;
    let expression_path = if simple {
        path.into()
    } else {
        format!("{path}.expr")
    };
    let compiled = compile(expr, ExpressionContext::Aggregate, &expression_path);
    if let Err(error @ CompileError::Limit { .. }) = &compiled {
        return Err(PrepareError::expression(
            error.clone(),
            Some(&expression_path),
        ));
    }
    let mut extra = Vec::new();
    let mut derive_relations = Vec::<String>::new();
    let mut derive_references = Vec::<String>::new();
    let derive = d
        .field(payload, "derive")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| {
            let items = sequence(d, id)?;

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
                if let Some(window) =
                    window_path(d, value, &format!("{binding_path}.derivation"), depth + 1)?
                {
                    return Err(invalid(
                        &window,
                        "a derive binding cannot use a window operation",
                    ));
                }
                let mut references = Vec::new();
                let mut intermediate_names = Vec::<String>::new();
                derivations::current_references(d, value, project, &mut references, 0)?;
                self_relations::template_references(d, value, templates, &mut references, 0)?;
                for reference in references {
                    if let Some((relation, _)) = reference.split_once('.') {
                        if !named.contains(relation) {
                            derive_relations.push(relation.into());
                            derive_references.push(reference);
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
                    return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                        diagnostic(
                            crate::diagnostic::ConditionCode::AggregateDeriveIntermediate,
                            &format!("{path}.derive"),
                            &[
                                (
                                    "reason",
                                    "a derive step reads only keep-declared named intermediates",
                                ),
                                ("intermediate", qualifier),
                            ],
                        ),
                    )]));
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
                    handler: literal_handler(d, derivation, &format!("{binding_path}.derivation"))?,
                    path: binding_path,
                    intermediate_names,
                });
            }
            Ok(bindings)
        })
        .transpose()?;
    let between = lookup_range::Declaration::prepare(d, payload, path)?;
    let groups = d
        .field(payload, "group_by")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| {
            sequence(d, id)?
                .iter()
                .map(|&id| Ok(text(d, id)?.into()))
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    let output_keys = sequence(d, field(d, d.root(), "keys")?)?
        .iter()
        .map(|&id| Ok(text(d, id)?.into()))
        .collect::<Result<_, PrepareError>>()?;
    let keys = prepare_keys(d, payload, path, project, extraction, templates, &mut extra)?;
    let empty_key = keys.as_ref().is_some_and(Vec::is_empty);
    let filter = d
        .field(payload, "filter")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| {
            let expr = text(d, id)?;
            Ok((
                expr.into(),
                compile(
                    expr,
                    ExpressionContext::Predicate,
                    &format!("{path}.filter"),
                ),
            ))
        })
        .transpose()?;
    if derive.is_some() {
        if let Some((_, Ok(filter))) = &filter {
            derive_relations.extend(
                filter
                    .identifiers()
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
                crate::json::Value::Array(
                    derive_relations
                        .iter()
                        .map(|relation| crate::json::Value::String(relation.clone()))
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
        derive_references,
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
fn validate_header<'a>(
    declaration: &'a Declaration,
    phase: Phase<'_>,
    driver: &str,
    intermediates: &[Option<crate::dataset::Intermediate>],
    findings: &mut Vec<BindFinding>,
) -> Result<Option<(&'a CompiledExpression, Vec<String>)>, BindError> {
    let compiled = match &declaration.compiled {
        Ok(compiled) => compiled,
        Err(CompileError::Invalid { failure, .. }) => {
            findings.push(BindFinding::Aggregate {
                path: declaration.expression_path.clone(),
                expression: declaration.expression.clone(),
                error: failure.clone(),
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
    Ok(Some((compiled, relations)))
}

type ResolveRef<'a> = &'a (dyn Fn(&str, &str, &mut Vec<BindFinding>) -> Result<Option<(usize, ColumnType)>, BindError>
         + 'a);
type GroupOperands = (Vec<usize>, Vec<AggregateBinding>);

#[allow(clippy::too_many_arguments)]
fn bind_groups_and_operands(
    compiled: &CompiledExpression,
    declaration: &Declaration,
    relation: Option<&str>,
    donor: AggregateSource,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    donor_schema: &TableSchema,
    grain: &[&str],
    phase: Phase<'_>,
    resolve: ResolveRef<'_>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<GroupOperands>, BindError> {
    if matches!(phase, Phase::Row { .. })
        && grain.iter().any(|name| {
            !catalog.bind(name).is_some_and(|binding| {
                matches!(binding, reference_binding::Binding::Dataset { .. })
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
    // Baseline for the REQ-0470 guard below: findings already reported
    // (for example an ungrouped identifier) must not suppress it, while
    // findings from resolving the group_by columns themselves must.
    let group_columns_before = findings.len();
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
    // REQ-0470: a qualified aggregate's group_by columns must also be
    // output keys: the keys the reduction joins on, either its explicit
    // `key` pairs or the defaults inferred from the output keys. The
    // Matched executor matches donor records on those keys only, so a
    // group_by column outside them would silently merge distinct groups.
    if matches!(phase, Phase::Column { .. }) && findings.len() == group_columns_before {
        if let Some(relation_name) = relation {
            let applicable: Vec<&str> = match &declaration.keys {
                Some(pairs) => pairs.iter().map(|(field, _)| field.as_str()).collect(),
                None => declaration
                    .output_keys
                    .iter()
                    .map(String::as_str)
                    .filter(|name| {
                        let written = format!("{relation_name}.{name}");
                        match donor {
                            AggregateSource::Primary => catalog.bind(&written).is_some(),
                            AggregateSource::Secondary(index) => secondary[index]
                                .schema
                                .columns()
                                .iter()
                                .any(|column| column.name == *name),
                        }
                    })
                    .collect(),
            };
            if declaration.groups.iter().any(|name| {
                name.strip_prefix(relation_name)
                    .and_then(|tail| tail.strip_prefix('.'))
                    .is_some_and(|field| !applicable.contains(&field))
            }) {
                let path = format!("{}.group_by", declaration.path);
                finding(
                    Finding::InvalidContext {
                        path: &path,
                        expression: &declaration.expression,
                        cause: Cause::QualifiedGroupNotOutputKey,
                    },
                    findings,
                )?;
                return Ok(None);
            }
        }
    }
    let mut bindings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
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
    let kind = |name: &str| binding_kind(name, &bindings, donor_schema, declaration);
    findings.extend(
        static_types::aggregate_kinds(compiled, kind, matches!(phase, Phase::Row { .. }))?
            .into_iter()
            .map(BindFinding::AggregateDiagnostic),
    );
    Ok(Some((group_columns, bindings)))
}

/// The type of a name the aggregate expression binds: a relation field's
/// captured type, or a derive binding's declared type.
fn binding_kind(
    name: &str,
    bindings: &[AggregateBinding],
    donor_schema: &TableSchema,
    declaration: &Declaration,
) -> Option<crate::value::ValueType> {
    bindings
        .iter()
        .find(|binding| binding.name == name)
        .and_then(|binding| {
            donor_schema
                .columns()
                .get(binding.column)
                .map(|column| column.kind.into())
                .or_else(|| {
                    declaration
                        .derive
                        .as_ref()
                        .and_then(|derive| {
                            derive.get(binding.column.saturating_sub(donor_schema.columns().len()))
                        })
                        .map(|derive| derive.kind.into())
                })
        })
}

#[allow(clippy::too_many_arguments)]
fn bind_filter(
    declaration: &Declaration,
    resolve: ResolveRef<'_>,
    relation: Option<&str>,
    before: usize,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<BoundPredicate>, BindError> {
    Ok(match &declaration.filter {
        Some((_, Ok(plan))) => {
            let mut bound = Vec::new();
            let mut kinds = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for name in plan.identifiers() {
                if !seen.insert(name) {
                    continue;
                }
                if let Some((column, kind)) = resolve(name, plan.spec_path(), findings)? {
                    bound.push(Binding {
                        name: name.into(),
                        read: Read::Source(column),
                    });
                    kinds.push((name, kind));
                    if relation.is_none() {
                        edges.push(column);
                    }
                }
            }
            if findings.len() == before {
                let typed = plan.typed(|name| {
                    crate::result_type::ResultType::declared(
                        kinds
                            .iter()
                            .find(|(bound, _)| *bound == name)
                            .map(|&(_, kind)| kind),
                    )
                });
                Some(
                    BoundPredicate::new(typed, bound)
                        .map_err(BindError::InvalidPredicateBinding)?,
                )
            } else {
                None
            }
        }
        Some((
            expr,
            Err(CompileError::Invalid {
                position, failure, ..
            }),
        )) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                &format!("{}.filter", declaration.path),
                expr,
                position.character,
                failure,
            )));
            None
        }
        Some((_, Err(error))) => return Err(BindError::ExpressionPolicy(error.clone())),
        None => None,
    })
}

#[allow(clippy::too_many_arguments)]
fn bound_relation(
    phase: Phase<'_>,
    relation: Option<&str>,
    donor: AggregateSource,
    group_columns: Vec<usize>,
    declaration: &Declaration,
    compiled: &CompiledExpression,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    output: &TableSchema,
    driver: &str,
    row: Option<&str>,
    project: Option<&crate::project_calls::ProjectCalls>,
    named: &intermediates::Declarations,
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<AggregateRelation>, BindError> {
    Ok(Some(match phase {
        Phase::Row { .. } => AggregateRelation::CurrentGroup,
        Phase::Column { .. } if relation.is_none() => AggregateRelation::Output {
            group_by: group_columns,
        },
        Phase::Column { .. } => {
            let relation = relation.expect("qualified context");
            // A fieldless reduction uses an existing donor-filter reference
            // for its join. Keep that authored owner; a payload field in the
            // expression still owns its own pairing.
            let key_path = if compiled.identifiers().any(|name| name.contains('.'))
                || !declaration.derive_references.is_empty()
                || !declaration.groups.is_empty()
            {
                declaration.expression_path.as_str()
            } else {
                declaration
                    .filter
                    .as_ref()
                    .and_then(|(_, plan)| plan.as_ref().ok())
                    .filter(|plan| {
                        plan.identifiers().into_iter().any(|name| {
                            name.split_once('.')
                                .is_some_and(|(owner, _)| owner == relation)
                        })
                    })
                    .map_or(declaration.expression_path.as_str(), |plan| {
                        plan.spec_path()
                    })
            };
            let defaults = declaration
                .output_keys
                .iter()
                .filter(|name| {
                    let written = format!("{relation}.{name}");
                    match donor {
                        AggregateSource::Primary => catalog.bind(&written).is_some(),
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
                if declaration.empty_key {
                    finding(
                        Finding::MissingKeys {
                            path: &declaration.path,
                            dataset: relation,
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
                    AggregateSource::Primary => match catalog.bind(&written) {
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
                        Finding::DonorKeyReference {
                            path: key_path,
                            identifier: &written,
                        },
                        findings,
                    )?;
                    continue;
                };
                let expected = value.result_type(catalog);
                if !expected.comparable(actual.into()) {
                    finding(
                        Finding::KeyType {
                            path: key_path,
                            source: &value.display_name(field),
                            expected,
                            actual,
                        },
                        findings,
                    )?;
                }
                let before_references = findings.len();
                for reference in value.references() {
                    if catalog.bind(reference).is_none() {
                        finding(
                            Finding::MatchValueReference {
                                path: key_path,
                                identifier: reference,
                            },
                            findings,
                        )?;
                    }
                }
                if findings.len() != before_references {
                    continue;
                }
                let bound = value.bind(
                    &format!("{}.key.{field}", declaration.path),
                    relation,
                    catalog,
                    findings,
                    match_values::Context {
                        output,
                        driver,
                        row,
                        phase,
                        project,
                        named,
                        secondary,
                        intermediates,
                    },
                )?;
                if let Some(value) = bound {
                    for column in value
                        .reads()
                        .filter_map(crate::dataset::MatchRead::output_column)
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
    }))
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
    let Some((compiled, relations)) =
        validate_header(declaration, phase, driver, intermediates, findings)?
    else {
        return Ok(None);
    };
    let relation = if declaration.derive.is_some() {
        declaration.derive_relations.first().map(String::as_str)
    } else {
        relations.first().map(String::as_str)
    };
    let before = findings.len();
    let donor = if let Some(relation) = relation {
        if relation == driver {
            AggregateSource::Primary
        } else if let Some(index) = secondary.iter().position(|source| source.name == relation) {
            AggregateSource::Secondary(index)
        } else {
            let reference = declaration
                .derive_references
                .iter()
                .map(String::as_str)
                .chain(
                    declaration
                        .filter
                        .iter()
                        .filter(|_| declaration.derive.is_some())
                        .filter_map(|(_, plan)| plan.as_ref().ok())
                        .flat_map(|plan| plan.identifiers()),
                )
                .chain(compiled.identifiers())
                .find(|name| {
                    name.split_once('.')
                        .is_some_and(|(owner, _)| owner == relation)
                });
            findings.push(BindFinding::UnknownReference {
                path: declaration.expression_path.clone(),
                name: reference.map_or_else(|| format!("{relation}.*"), String::from),
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
            (None, _) => match catalog.bind(name) {
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
            (Some(_), AggregateSource::Primary) => match catalog.bind(name) {
                Some(reference_binding::Binding::Dataset {
                    field, column_type, ..
                }) => Some((field, column_type)),
                _ => None,
            },
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
    let Some((group_columns, bindings)) = bind_groups_and_operands(
        compiled,
        declaration,
        relation,
        donor,
        catalog,
        secondary,
        &donor_schema,
        &grain,
        phase,
        &resolve,
        edges,
        findings,
    )?
    else {
        return Ok(None);
    };
    let filter = bind_filter(declaration, &resolve, relation, before, edges, findings)?;
    let Some(bound_relation) = bound_relation(
        phase,
        relation,
        donor,
        group_columns,
        declaration,
        compiled,
        catalog,
        secondary,
        output,
        driver,
        row,
        project,
        named,
        intermediates,
        edges,
        findings,
    )?
    else {
        return Ok(None);
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
    let typed = compiled.typed(|name| {
        crate::result_type::ResultType::known(binding_kind(
            name,
            &bindings,
            &donor_schema,
            declaration,
        ))
    });
    let bound = BoundAggregate::new(
        typed,
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
    Ok(Some(Expression::Aggregate(std::boxed::Box::new(bound))))
}

/// A qualified aggregate's range (LOOKUP-9): bounds are fields of its
/// relation and the value a value of the current row, admitted for every row
/// the column is derived for ([`lookup_range::admit`]).
fn bind_range(
    declaration: &Declaration,
    catalog: &Catalog,
    donor: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<crate::dataset::RelationRange>, BindError> {
    use crate::diagnostic::lookup::Owner;
    let Some(range) = &declaration.between else {
        return Ok(None);
    };
    if let Some(finding) = range.bounds_finding(Owner::Unnamed) {
        findings.push(BindFinding::AggregateDiagnostic(finding));
        return Ok(None);
    }
    let mut diagnostics = Vec::new();
    let bounds = range.bind_bounds(
        Owner::Unnamed,
        donor,
        &|column| donor.columns().get(column).map(|field| field.kind),
        &|_| true,
        &mut diagnostics,
    );
    findings.extend(
        diagnostics
            .into_iter()
            .map(BindFinding::AggregateDiagnostic),
    );
    let Some(bounds) = bounds else {
        return Ok(None);
    };
    let Some((read, kind)) = range.current_value(catalog, secondary, intermediates, None, edges)?
    else {
        findings.push(BindFinding::AggregateDiagnostic(
            range.unknown_value(Owner::Unnamed),
        ));
        return Ok(None);
    };
    Ok(match range.finish(read, kind, bounds, None)? {
        Ok(range) => Some(range),
        Err(diagnostic) => {
            findings.push(BindFinding::AggregateDiagnostic(diagnostic));
            None
        }
    })
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
            .map(|(column, field)| {
                (
                    field.name.clone(),
                    (column, item.source_kind(secondary, column)),
                )
            })
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
        let start = findings.len();
        let expression = derivations::bind_row(
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
        )?;
        // VALUE-24: a binding converts to its declared type like a column.
        if expression.is_some() && findings.len() == start && binding.handler.is_none() {
            let input = |name: &str| {
                static_types::result_type(name, named, &catalog, secondary, intermediates)
            };
            findings.extend(
                result_types::conversion_finding(
                    &binding.path,
                    binding.branch.operation.result_type(&input),
                    binding.kind,
                )
                .map(BindFinding::Scalar),
            );
        }
        if let Some(expression) = expression {
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
            Err(CompileError::Invalid { failure, .. }) => {
                findings.push(BindFinding::Aggregate {
                    path: self.expression_path.clone(),
                    expression: self.expression.clone(),
                    error: failure.clone(),
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
