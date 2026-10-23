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

/// One aggregate as binding reads it from the tree.
#[derive(Clone, Debug)]
pub(super) struct Declaration {
    expression: String,
    expression_path: String,
    path: String,
    /// The aggregate's node, which the tree types.
    expr: ExprId,
    compiled: Result<CompiledExpression, CompileError>,
    groups: Vec<String>,
    keys: Option<Vec<(String, match_values::Declaration)>>,
    /// A written `key` that names no pair (LOOKUP-5).
    empty_key: bool,
    /// An aggregate without `key` of its own, which matches on its input's
    /// declared `key` (LOOKUP-5, D63).
    from_input: bool,
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
    /// The binding's derivation.
    expr: ExprId,
    handler: Option<crate::conversion::LiteralHandler>,
    path: String,
    intermediate_names: Vec<String>,
}
impl Declaration {
    /// The result type of each derive binding's derivation over `names`, in
    /// declaration order, typed within the aggregate, where every binding
    /// names its declared type.
    fn derive_types(
        &self,
        tree: &types::Tree,
        names: &dyn ast::TypeEnv,
    ) -> Result<Vec<crate::result_type::ResultType>, BindError> {
        let env = tree.env(names);
        let table = ast::Table::new(tree.spec(), self.expr, ast::Scope::Row, &env);
        Ok(self
            .derive
            .iter()
            .flatten()
            .map(|binding| table.of(binding.expr))
            .collect())
    }
    /// The aggregate at `e` as binding reads it. `named` holds the named
    /// intermediates; a derive step inside an intermediate's own derivation
    /// classifies the names it reads against the intermediates derived
    /// before that one, as preparation did. `calls` says whether a project
    /// call's arguments are among the names a binding reads.
    pub(super) fn from_tree(
        spec: &ast::Spec,
        e: ExprId,
        named: &intermediates::Declarations,
        calls: bool,
    ) -> Result<Self, PrepareError> {
        let ExprKind::Aggregate(aggregate) = &spec.expr(e).kind else {
            return Err(PrepareError::Internal);
        };
        let ExprKind::Formula(formula) = &spec.expr(aggregate.expr).kind else {
            return Err(PrepareError::Internal);
        };
        let contains = |name: &str| named.earlier_contains(name);
        let kept = |name: &str| named.earlier_kept(name);
        let derive = derive(spec, e, aggregate, &contains, &kept, calls, None)?;
        let filter = aggregate
            .filter
            .map(|filter| match &spec.expr(filter).kind {
                ExprKind::Formula(formula) => {
                    Ok((formula.text.text.clone(), formula.compiled.clone()))
                }
                _ => Err(PrepareError::Internal),
            })
            .transpose()?;
        let (derive, derive_relations, derive_references) = match derive {
            Some(Derive {
                bindings,
                mut relations,
                references,
            }) => {
                relations.extend(filter_relations(&filter, &contains));
                relations.sort_unstable();
                relations.dedup();
                (Some(bindings), relations, references)
            }
            None => (None, Vec::new(), Vec::new()),
        };
        let keys = keys(spec, aggregate, calls, None)?;
        Ok(Self {
            expression: formula.text.text.clone(),
            expression_path: checks::render(spec, formula.text.meta),
            path: checks::path(spec, e),
            expr: e,
            compiled: formula.compiled.clone(),
            groups: aggregate
                .group_by
                .iter()
                .map(|&group| {
                    checks::name(spec, group)
                        .map(String::from)
                        .ok_or(PrepareError::Internal)
                })
                .collect::<Result<_, _>>()?,
            empty_key: keys.as_ref().is_some_and(Vec::is_empty),
            keys,
            from_input: matches!(aggregate.key, Some(ast::JoinKey::FromInput)),
            filter,
            derive,
            derive_relations,
            derive_references,
            between: aggregate
                .between
                .as_ref()
                .map(|between| lookup_range::Declaration::from_tree(spec, between))
                .transpose()?,
        })
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

/// A derive step's bindings, with the relations and the qualified names
/// they read.
struct Derive {
    bindings: Vec<DeriveDeclaration>,
    relations: Vec<String>,
    references: Vec<String>,
}

/// The relations a derive step's filter names besides the named
/// intermediates.
fn filter_relations<'a>(
    filter: &'a Option<(String, Result<CompiledExpression, CompileError>)>,
    contains: &'a dyn Fn(&str) -> bool,
) -> impl Iterator<Item = String> + 'a {
    filter
        .iter()
        .filter_map(|(_, plan)| plan.as_ref().ok())
        .flat_map(|plan| plan.identifiers())
        .filter_map(|name| {
            name.split_once('.')
                .map(|(relation, _)| String::from(relation))
        })
        .filter(move |relation| !contains(relation))
}

/// The derive step of the aggregate at `e` (LOOKUP-22): each binding with
/// what it reads, or the refusal preparation reports for it, in the order
/// it has always reported them. `contains` and `kept` say whether a name is
/// a named intermediate and whether it declares `keep`. With `admission`,
/// each binding's derivation is admitted too, after its scope checks and
/// before its handler.
fn derive(
    spec: &ast::Spec,
    e: ExprId,
    aggregate: &ast::Aggregate,
    contains: &dyn Fn(&str) -> bool,
    kept: &dyn Fn(&str) -> bool,
    calls: bool,
    admission: Option<(&derivations::Admission<'_>, usize)>,
) -> Result<Option<Derive>, PrepareError> {
    let Some(&field) = aggregate
        .written
        .fields()
        .iter()
        .find(|&&field| matches!(field, "derive" | "derivations"))
    else {
        return Ok(None);
    };
    let path = checks::path(spec, e);
    let list = format!("{path}.{field}");
    let invalid = |path: &str, reason: &str| {
        PrepareError::Invalid(vec![PreflightFinding::Scalar(diagnostic(
            crate::diagnostic::ConditionCode::AggregateDeriveDeclaration,
            path,
            &[("reason", reason)],
        ))])
    };
    if aggregate.bindings.is_empty() {
        return Err(invalid(&list, "a derive step is a non-empty binding list"));
    }
    let mut relations = Vec::new();
    let mut references = Vec::new();
    let mut bindings = Vec::<DeriveDeclaration>::new();
    for binding in &aggregate.bindings {
        let binding_path = checks::render(spec, binding.meta);
        let name = binding.name.name.as_str();
        if bindings.iter().any(|prior| prior.name == name) {
            return Err(invalid(
                &binding_path,
                "derive binding names are unique identifiers",
            ));
        }
        let kind = binding.declared.ok_or(PrepareError::Internal)?;
        let value = binding.derivation.expr;
        if let Some(window) = checks::positions(spec, value)
            .into_iter()
            .find(|&e| matches!(spec.expr(e).kind, ExprKind::Window(_)))
        {
            return Err(invalid(
                &checks::path(spec, window),
                "a derive binding cannot use a window operation",
            ));
        }
        let mut intermediate_names = Vec::<String>::new();
        let scan = reads::Scan {
            relation: false,
            windows: true,
        };
        for reference in reads::names(spec, value, scan, calls)? {
            if let Some((relation, _)) = reference.split_once('.') {
                if !contains(relation) {
                    relations.push(relation.into());
                    references.push(reference);
                } else {
                    intermediate_names.push(relation.into());
                }
            } else if !bindings.iter().any(|binding| binding.name == reference) {
                let mut finding = LookupFinding::new(
                    &binding_path,
                    crate::diagnostic::ConditionCode::AggregateDeriveVariable,
                );
                finding.text("variable", &reference);
                finding.text("binding", name);
                // The binding's expression, when its value is one.
                match &spec.expr(value).kind {
                    ExprKind::Formula(formula) => finding.text("expr", &formula.text.text),
                    _ => finding.null("expr"),
                }
                return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                    finding.diagnostic(),
                )]));
            }
        }
        if let Some(qualifier) = intermediate_names.iter().find(|name| !kept(name)) {
            return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                diagnostic(
                    crate::diagnostic::ConditionCode::AggregateDeriveIntermediate,
                    &list,
                    &[
                        (
                            "reason",
                            "a derive step reads only named intermediates that declare a selection (keep or nearest)",
                        ),
                        ("intermediate", qualifier),
                    ],
                ),
            )]));
        }
        if let Some((admission, depth)) = admission {
            admission.admit(spec, value, depth + 1)?;
        }
        let handler = binding
            .derivation
            .unconvertible
            .as_ref()
            .map(|handler| {
                let spec_path = checks::render(spec, handler.meta);
                Ok(crate::conversion::LiteralHandler {
                    value: constant(&handler.value, &spec_path)?,
                    spec_path,
                })
            })
            .transpose()?;
        bindings.push(DeriveDeclaration {
            name: name.into(),
            kind,
            expr: value,
            handler,
            path: binding_path,
            intermediate_names,
        });
    }
    Ok(Some(Derive {
        bindings,
        relations,
        references,
    }))
}

/// An aggregate's written `key`: donor columns, each paired once with its
/// same-named value, or each donor column with its written value. With
/// `admission`, each written value is admitted as a key's match value, an
/// operation outside the scalar and window verbs being an unsupported key.
fn keys(
    spec: &ast::Spec,
    aggregate: &ast::Aggregate,
    calls: bool,
    mut admission: Option<(&derivations::Admission<'_>, &mut Vec<UnsupportedFeature>)>,
) -> Result<Option<Vec<(String, match_values::Declaration)>>, PrepareError> {
    match &aggregate.key {
        Some(ast::JoinKey::Columns { columns, .. }) => {
            if columns.len() > 64 {
                return Err(PrepareError::limit("aggregate_keys", 64, None, None));
            }
            let mut pairs = Vec::<(String, match_values::Declaration)>::new();
            for column in columns {
                if !pairs.iter().any(|(prior, _)| *prior == column.name) {
                    pairs.push((
                        column.name.clone(),
                        match_values::Declaration::Variable(column.name.clone()),
                    ));
                }
            }
            Ok(Some(pairs))
        }
        Some(ast::JoinKey::Pairs { pairs, .. }) => {
            if pairs.len() > 64 {
                return Err(PrepareError::limit("aggregate_keys", 64, None, None));
            }
            let mut declared = Vec::new();
            for (name, value) in pairs {
                if let Some((admission, extra)) = admission.as_mut() {
                    match_values::admit(
                        spec,
                        name.meta,
                        *value,
                        &checks::render(spec, name.meta),
                        admission.project,
                        extra,
                    )?;
                }
                declared.push((
                    name.name.clone(),
                    match_values::Declaration::from_tree(spec, name.meta, *value, calls)?,
                ));
            }
            Ok(Some(declared))
        }
        _ => Ok(None),
    }
}

/// Admit the aggregate at `e`, `depth` operations deep: its expression
/// within the compiler's limits, its derive step, its key, and the one
/// relation a derive step names (LOOKUP-22).
pub(super) fn admit(
    spec: &ast::Spec,
    e: ExprId,
    admission: &derivations::Admission<'_>,
    depth: usize,
) -> Result<(), PrepareError> {
    let ExprKind::Aggregate(aggregate) = &spec.expr(e).kind else {
        return Err(PrepareError::Internal);
    };
    let ExprKind::Formula(formula) = &spec.expr(aggregate.expr).kind else {
        return Err(PrepareError::Internal);
    };
    if let Err(error @ CompileError::Limit { .. }) = &formula.compiled {
        return Err(PrepareError::expression(
            error.clone(),
            Some(&checks::render(spec, formula.text.meta)),
        ));
    }
    let named = admission.intermediates;
    let contains = |name: &str| named.contains(name);
    let kept = |name: &str| named.kept(name);
    let calls = admission.project.is_some();
    let derive = derive(
        spec,
        e,
        aggregate,
        &contains,
        &kept,
        calls,
        Some((admission, depth)),
    )?;
    let mut extra = Vec::new();
    keys(spec, aggregate, calls, Some((admission, &mut extra)))?;
    if let Some(mut derive) = derive {
        let filter = aggregate
            .filter
            .and_then(|filter| match &spec.expr(filter).kind {
                ExprKind::Formula(formula) => {
                    Some((formula.text.text.clone(), formula.compiled.clone()))
                }
                _ => None,
            });
        derive
            .relations
            .extend(filter_relations(&filter, &contains));
        derive.relations.sort_unstable();
        derive.relations.dedup();
        if derive.relations.len() != 1 {
            let field = aggregate
                .written
                .fields()
                .iter()
                .find(|&&field| matches!(field, "derive" | "derivations"))
                .ok_or(PrepareError::Internal)?;
            let mut value = diagnostic(
                crate::diagnostic::ConditionCode::AggregateDeriveRelation,
                &format!("{}.{field}", checks::path(spec, e)),
                &[("reason", "a derive step names exactly one relation")],
            );
            value.context.insert(
                "relations".into(),
                crate::json::Value::Array(
                    derive
                        .relations
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
    Ok(())
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
                // The aggregate's own expression reads it, so no binding does.
                let mut finding = LookupFinding::new(
                    &declaration.expression_path,
                    crate::diagnostic::ConditionCode::AggregateDeriveVariable,
                );
                finding.text("variable", name);
                finding.text("expr", &declaration.expression);
                finding.null("binding");
                findings.push(BindFinding::AggregateDiagnostic(finding.diagnostic()));
            }
        }
        for binding in derive {
            for qualifier in &binding.intermediate_names {
                let item = intermediates
                    .iter()
                    .flatten()
                    .find(|item| item.identifier == *qualifier);
                if item.is_none_or(|item| item.selection.is_none() && item.nearest.is_none()) {
                    findings.push(BindFinding::AggregateDiagnostic(diagnostic(
                        crate::diagnostic::ConditionCode::AggregateDeriveIntermediate,
                        &format!("{}.derive", declaration.path),
                        &[
                            (
                                "reason",
                                "a derive step reads only named intermediates that declare a selection (keep or nearest)",
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
    declared: Option<&[String]>,
    relation: Option<&str>,
    catalog: &Catalog,
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
    // Baseline for the LOOKUP-19 guard below: findings already reported
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
    // LOOKUP-19: a qualified aggregate's group_by columns must also be
    // output keys: the keys the reduction joins on, either its explicit
    // `key` pairs or its input's declared `key`. The Matched executor
    // matches donor records on those keys only, so a group_by column
    // outside them would silently merge distinct groups.
    if matches!(phase, Phase::Column { .. }) && findings.len() == group_columns_before {
        if let Some(relation_name) = relation {
            let applicable: Vec<&str> = match &declaration.keys {
                Some(pairs) => pairs.iter().map(|(field, _)| field.as_str()).collect(),
                None => declared
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str)
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
        types::aggregate_kinds(compiled, kind, matches!(phase, Phase::Row { .. }))?
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

/// The output columns and every input's record at its own port: the primary
/// input, `driver` in a column-phase catalog, then each secondary input. None
/// when the catalog holds no record of the primary input.
fn record_catalog(
    catalog: &Catalog,
    driver: &str,
    output: &TableSchema,
    secondary: &[SecondarySource],
) -> Result<Option<Catalog>, BindError> {
    let Some(primary) = catalog.fields(driver) else {
        return Ok(None);
    };
    fn field(column: &crate::table::Column) -> reference_binding::Field<'_> {
        reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        }
    }
    let outputs = output.columns().iter().map(field).collect::<Vec<_>>();
    let inputs = secondary
        .iter()
        .map(|input| input.schema.columns().iter().map(field).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let datasets = std::iter::once(reference_binding::Dataset {
        name: driver,
        fields: &primary,
    })
    .chain(
        secondary
            .iter()
            .zip(&inputs)
            .map(|(input, fields)| reference_binding::Dataset {
                name: &input.name,
                fields,
            }),
    )
    .collect::<Vec<_>>();
    Catalog::compile(&outputs, &datasets)
        .map(Some)
        .map_err(BindError::Catalog)
}

#[allow(clippy::too_many_arguments)]
fn bound_relation(
    phase: Phase<'_>,
    relation: Option<&str>,
    donor: AggregateSource,
    group_columns: Vec<usize>,
    declaration: &Declaration,
    declared: Option<&[String]>,
    compiled: &CompiledExpression,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    output: &TableSchema,
    driver: &str,
    row: Option<&str>,
    project: Option<&crate::project_calls::ProjectCalls>,
    tree: &types::Tree,
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
                        plan.identifiers().any(|name| {
                            name.split_once('.')
                                .is_some_and(|(owner, _)| owner == relation)
                        })
                    })
                    .map_or(declaration.expression_path.as_str(), |plan| {
                        plan.spec_path()
                    })
            };
            // Without `key`: the input's declared key, each column paired with
            // the current row's same-named value.
            let defaults = declared
                .unwrap_or_default()
                .iter()
                .map(|name| {
                    (
                        name.clone(),
                        match_values::Declaration::Variable(name.clone()),
                    )
                })
                .collect::<Vec<_>>();
            let pairs = declaration.keys.as_ref().unwrap_or(&defaults);
            if pairs.is_empty() {
                finding(
                    if declaration.empty_key {
                        Finding::MissingKeys {
                            path: &declaration.path,
                            dataset: relation,
                        }
                    } else {
                        Finding::UndeclaredInputKey {
                            path: &declaration.path,
                            dataset: relation,
                        }
                    },
                    findings,
                )?;
                return Ok(None);
            }
            // A match value reads the current row: an output column, or the
            // record of any input at that input's own port, as a named
            // intermediate's key does (LOOKUP-5). Whether the rows that
            // evaluate the aggregate have that record is admitted for each
            // reading context (`admit_record_keys`). A written `key` owns
            // its refusals, as a named intermediate's does (LOOKUP-5): a key
            // column the relation lacks, or a pair whose types cannot
            // compare, at `key`; a match value that reads no current-row
            // value at its pair, `key.<column>`, where a row context that
            // lacks its record refuses it too. The key an input declares
            // reports at the read.
            let owned = declaration
                .keys
                .is_some()
                .then(|| format!("{}.key", declaration.path));
            let records = record_catalog(catalog, driver, output, secondary)?;
            let values = records.as_ref().unwrap_or(catalog);
            let mut keys = Vec::new();
            for (field, value) in pairs {
                let pair = format!("{}.key.{field}", declaration.path);
                let (key_path, value_path) = match &owned {
                    Some(key) => (key.as_str(), pair.as_str()),
                    None => (key_path, key_path),
                };
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
                match donor_field {
                    Some((_, actual)) => {
                        let expected = value.result_type(tree, values)?;
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
                    }
                    None => {
                        finding(
                            Finding::DonorKeyReference {
                                path: key_path,
                                identifier: &written,
                            },
                            findings,
                        )?;
                        // A written key still reads its match value, as a
                        // named intermediate's does; the key an input
                        // declares reads nothing more of the pair.
                        if owned.is_none() {
                            continue;
                        }
                    }
                }
                let before_references = findings.len();
                for reference in value.references() {
                    if values.bind(reference).is_none() {
                        finding(
                            Finding::MatchValueReference {
                                path: value_path,
                                identifier: reference,
                            },
                            findings,
                        )?;
                    }
                }
                let Some((source_column, _)) =
                    donor_field.filter(|_| findings.len() == before_references)
                else {
                    continue;
                };
                let bound = value.bind(
                    &pair,
                    relation,
                    values,
                    findings,
                    match_values::Context {
                        output,
                        driver,
                        row,
                        phase,
                        project,
                        tree,
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
                        path: pair,
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
    tree: &types::Tree,
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
    // An aggregate without `key` of its own matches on its input's declared
    // `key`, none when the input declares none (LOOKUP-5, D63).
    let declared = declaration.from_input.then(|| {
        relation
            .and_then(|relation| tree.spec().input_key(relation))
            .unwrap_or_default()
            .iter()
            .map(|column| column.name.clone())
            .collect::<Vec<_>>()
    });
    let Some((group_columns, bindings)) = bind_groups_and_operands(
        compiled,
        declaration,
        declared.as_deref(),
        relation,
        catalog,
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
        declared.as_deref(),
        compiled,
        catalog,
        secondary,
        output,
        driver,
        row,
        project,
        tree,
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
        tree,
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

/// Visit every qualified aggregate a row evaluates directly, through lazy and
/// nested alternatives. A derive binding evaluates for a donor record instead.
fn visit(expression: &Expression, visitor: &mut impl FnMut(&crate::dataset::BoundAggregate)) {
    match expression {
        Expression::Aggregate(aggregate) => visitor(aggregate),
        Expression::SourceBinding { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::Round { source, .. } => visit(&source.expression, visitor),
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let crate::dataset::SelectionRead::Expression(source) = &source.read {
                    visit(&source.expression, visitor);
                }
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => {
            for source in operands {
                visit(&source.expression, visitor);
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                visit(&branch.assignment.expression, visitor);
            }
        }
        _ => (),
    }
}

/// A matched aggregate's match value read from an input's record names the
/// driver of every row that evaluates it (LOOKUP-5). Each context pairs the
/// expressions a row evaluates with the record ports of the drivers of those
/// rows, as for a range value (`lookup_range::admit`). A value read from
/// another input's record names no value of those rows and fails as
/// `unknown_field` at its key, once however many contexts read it.
pub(super) fn admit_record_keys(
    contexts: &[(Vec<&Expression>, Vec<lookup_range::RowDriver>)],
    findings: &mut Vec<BindFinding>,
) {
    let mut reported = BTreeSet::new();
    for (expressions, drivers) in contexts {
        for expression in expressions {
            visit(expression, &mut |aggregate| {
                let crate::dataset::AggregateRelation::Matched { keys, .. } = aggregate.relation()
                else {
                    return;
                };
                for key in keys {
                    for read in key.value.reads() {
                        let port = match read {
                            crate::dataset::MatchRead::Record { .. } => 0,
                            crate::dataset::MatchRead::SecondaryRecord { source, .. } => source + 1,
                            crate::dataset::MatchRead::Output { .. } => continue,
                        };
                        if drivers.iter().any(|driver| driver.port != port)
                            && reported.insert((key.path.clone(), read.identifier().to_owned()))
                        {
                            findings.push(BindFinding::Scalar(
                                crate::diagnostic::lookup::match_value_reference(
                                    &key.path,
                                    crate::diagnostic::lookup::Owner::Unnamed,
                                    read.identifier(),
                                ),
                            ));
                        }
                    }
                }
            });
        }
    }
}

/// A row template's derivations complete in dependency order (WINDOW-3), but
/// the stage split moves a matched aggregate to the template's window pass only
/// when a key, or a `case` condition around it, reads a window result. A range
/// value or a derive binding that reads one, in an assignment the split leaves
/// before the windows, would read it before the window completes it, as a key
/// computed beside a window key can. Neither is supported yet. A deferred
/// assignment reads completed results, and a keyed template evaluates every
/// assignment before its windows, as resolution does.
pub(super) fn late_reads(plan: &crate::dataset::DatasetPlan) -> Vec<BindFinding> {
    let width = plan.output().columns().len();
    let intermediates = plan.intermediates();
    let mut findings = Vec::new();
    let mut reported = BTreeSet::new();
    let mut refuse = |operation: &str, path: String, findings: &mut Vec<BindFinding>| {
        if reported.insert(path.clone()) {
            findings.push(BindFinding::Scalar(crate::application_issue::unsupported(
                operation,
                Some(&path),
            )));
        }
    };
    for template in plan.templates() {
        if matches!(template.mode, crate::dataset::RowMode::Keys) {
            continue;
        }
        let deferred = crate::dataset::window_stages(&template.assignments, width, intermediates);
        let late =
            |read: Read| crate::dataset::read_requires_window(&read, &deferred, intermediates);
        for assignment in &template.assignments {
            if deferred.get(assignment.column).copied().unwrap_or(false) {
                continue;
            }
            visit(&assignment.expression, &mut |aggregate| {
                if !matches!(
                    aggregate.relation(),
                    crate::dataset::AggregateRelation::Matched { .. }
                ) {
                    return;
                }
                if let Some(range) = aggregate.range() {
                    let read = match range.current().read {
                        crate::dataset::CurrentRead::Output(column) => Some(Read::Column(column)),
                        crate::dataset::CurrentRead::Intermediate { index, column } => {
                            Some(Read::Intermediate { index, column })
                        }
                        crate::dataset::CurrentRead::Record { .. }
                        | crate::dataset::CurrentRead::Contextual => None,
                    };
                    if read.is_some_and(late) {
                        refuse(
                            "aggregate_range_window_read",
                            format!("{}.value", range.path()),
                            &mut findings,
                        );
                    }
                }
                for binding in aggregate.derivations() {
                    let mut reads = false;
                    crate::dataset::relation_predicates::visit(
                        &binding.assignment.expression,
                        &mut |index| reads |= late(Read::Intermediate { index, column: 0 }),
                    );
                    if reads {
                        refuse(
                            "aggregate_derive_window_read",
                            binding.assignment.path.clone(),
                            &mut findings,
                        );
                    }
                }
            });
        }
    }
    findings
}

/// A qualified aggregate's range (LOOKUP-9): bounds are fields of its
/// relation and the value a value of the current row, admitted for every row
/// the column is derived for ([`lookup_range::admit`]). A bare value that no
/// column takes fails whatever the bounds bind.
#[allow(clippy::too_many_arguments)]
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
        if range.names_no_column(catalog) {
            findings.push(BindFinding::AggregateDiagnostic(
                range.unknown_value(Owner::Unnamed),
            ));
        }
        return Ok(None);
    };
    let Some((read, kind)) =
        range.current_value(catalog, secondary, intermediates, None, None, edges)?
    else {
        findings.push(BindFinding::AggregateDiagnostic(
            range.unknown_value(Owner::Unnamed),
        ));
        return Ok(None);
    };
    Ok(match range.finish(read, kind, bounds, Owner::Unnamed)? {
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
    tree: &types::Tree,
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
    let derived_types = declaration.derive_types(
        tree,
        &types::CapturedTypes::new(named, &catalog, secondary, intermediates),
    )?;
    let mut bound = Vec::new();
    for (index, binding) in derive.iter().enumerate() {
        let available = (0..width + index).collect::<Vec<_>>();
        let mut dependencies = Vec::new();
        let start = findings.len();
        let expression = BindContext {
            scope: binder::Scope {
                template: binder::Template::Any,
                phase: Phase::Row { group_by: None },
                driver: relation,
                available: Some(&available),
            },
            donors: binder::Donors {
                named,
                secondary,
                items: intermediates,
            },
            catalog: &catalog,
            output: &derive_output,
            project,
            tree,
            edges: &mut dependencies,
            findings: &mut *findings,
            reported: false,
        }
        .bind(binding.expr, width + index)?;
        // VALUE-24: a binding converts to its declared type like a column.
        if expression.is_some() && findings.len() == start && binding.handler.is_none() {
            findings.extend(
                types::conversion_finding(&binding.path, derived_types[index], binding.kind)
                    .map(BindFinding::Scalar),
            );
        }
        if let Some(expression) = expression {
            edges.extend(dependencies.into_iter().filter(|&column| column < width));
            bound.push(crate::dataset::AggregateDerivation {
                assignment: crate::dataset::Assignment {
                    column: width + index,
                    expression,
                    path: checks::path(tree.spec(), binding.expr),
                },
                kind: binding.kind,
                handler: binding.handler.clone(),
                path: binding.path.clone(),
            });
        }
    }
    Ok((bound, Some(derived_schema)))
}
