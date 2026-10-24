//! Original key expressions bind in current-row scope, without a target column.
use super::*;
use crate::{
    dataset::{MatchRead, MatchValue},
    result_type::ResultType,
};

#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub output: &'a TableSchema,
    pub driver: &'a str,
    pub row: Option<&'a str>,
    pub phase: crate::reference_scope::Phase<'a>,
    pub project: Option<&'a crate::project_calls::ProjectCalls>,
    pub tree: &'a types::Tree,
    pub named: &'a intermediates::Declarations,
    pub secondary: &'a [crate::dataset::SecondarySource],
    pub intermediates: &'a [Option<crate::dataset::Intermediate>],
}
/// A key's current-row value. A written value other than a name keeps its
/// node, which the tree types.
#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Variable(String),
    Source(String),
    Literal(crate::value::Value, ExprId),
    /// A verb that matches on one read of its source, transformed
    /// ([`ast::ScalarOp::key_read`]).
    Read(Box<crate::scalar_verb::Operation>, ExprId),
    Expression {
        expr: ExprId,
        references: Vec<String>,
    },
}

/// Whether a written match value at `value` is the operation a key's pair
/// at `pair` holds, rather than a name written there alone.
fn operation(spec: &ast::Spec, pair: ast::Meta, value: ExprId) -> bool {
    spec.expr(value).meta.path != pair.path
}

/// The operation a match value may not be: one whose records or branches
/// need a demand-aware key owner. Its keyword names it.
fn deferred_owner(kind: &ExprKind) -> Option<&str> {
    Some(match kind {
        ExprKind::Case(_) => "case",
        ExprKind::Call { .. } => "function",
        ExprKind::Aggregate(_) => "aggregate",
        ExprKind::Odm(_) => "odm",
        ExprKind::Flag { .. } => "flag",
        ExprKind::FirstAvailable { .. } => "first_available",
        ExprKind::Unsupported(operation) => operation,
        _ => return None,
    })
}

/// Admit a key's match value written at `value` in the pair at `pair`,
/// whose path is `path`. A scalar or window verb is admitted as any
/// derivation is; a structured source, a conditional imputation operand,
/// an operation outside the verbs and a read through a named intermediate
/// are unsupported keys, collected in `extra`.
pub(super) fn admit(
    spec: &ast::Spec,
    pair: ast::Meta,
    value: ExprId,
    path: &str,
    project: Option<&functions::Prepared>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<(), PrepareError> {
    let reference = admit_value(spec, pair, value, project, extra)?;
    if let Some((relation, _)) = reference.and_then(|name| name.split_once('.')) {
        if spec
            .intermediates
            .iter()
            .any(|item| item.id.name == relation)
        {
            reject(extra, "intermediate_key_expression", path.into());
        }
    }
    Ok(())
}

/// Admit the match value written at `value` in the pair at `pair`, as
/// [`admit`] does, except that it may read a named intermediate: a pair
/// relation's key value reads its left side's record (LOOKUP-27). Gives
/// the name its read operand reads, when it has one.
pub(super) fn admit_value<'s>(
    spec: &'s ast::Spec,
    pair: ast::Meta,
    value: ExprId,
    project: Option<&functions::Prepared>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Option<&'s str>, PrepareError> {
    let site = checks::path(spec, value);
    let mut reference = None;
    match &spec.expr(value).kind {
        ExprKind::Ref(name) => {
            if operation(spec, pair, value) {
                reference = Some(name.as_str());
            }
        }
        ExprKind::Source(read) => {
            for field in read.written.fields() {
                reject(extra, "intermediate_key_source", format!("{site}.{field}"));
            }
            reference = checks::name(spec, read.variable);
        }
        ExprKind::Literal(literal) => {
            constant(literal, &site)?;
        }
        ExprKind::Scalar { op, operands, .. } if op.key_read() => {
            crate::scalar_verb::Operation::from_tree(spec, value).map_err(refused_verb)?;
            reference = checks::operand_name(spec, operands, ast::Slot::Source);
        }
        ExprKind::Scalar { .. } | ExprKind::Formula(_) | ExprKind::Window(_) => {
            let scan = reads::Scan {
                relation: true,
                windows: true,
            };
            reads::names(spec, value, scan, project.is_some())?;
            let none = intermediates::Declarations::default();
            derivations::Admission {
                project,
                intermediates: &none,
            }
            .admit(spec, value, 0)?;
            // An operand the verb reads only when its source needs it, such as
            // an `impute`'s bounds and fallback, which a key's demand cannot
            // know.
            if let ExprKind::Scalar { op, operands, .. } = &spec.expr(value).kind {
                for operand in operands
                    .iter()
                    .filter(|operand| op.conditional(operand.slot))
                {
                    if let ast::Segment::Field(field) =
                        spec.paths.segment(spec.expr(operand.expr).meta.path)
                    {
                        reject(extra, field, format!("{site}.{field}"));
                    }
                }
            }
        }
        kind => {
            if let Some(owner) = deferred_owner(kind) {
                reject(extra, owner, site);
            }
        }
    }
    Ok(reference)
}

impl Declaration {
    /// The match value written at `value` in the pair at `pair`. `calls`
    /// says whether a project call's arguments are among the names an
    /// expression reads.
    pub(super) fn from_tree(
        spec: &ast::Spec,
        pair: ast::Meta,
        value: ExprId,
        calls: bool,
    ) -> Result<Self, PrepareError> {
        Ok(match &spec.expr(value).kind {
            ExprKind::Ref(name) if operation(spec, pair, value) => Self::Source(name.0.clone()),
            ExprKind::Ref(name) => Self::Variable(name.0.clone()),
            ExprKind::Source(read) => Self::Source(
                checks::name(spec, read.variable)
                    .ok_or(PrepareError::Internal)?
                    .into(),
            ),
            ExprKind::Literal(literal) => {
                Self::Literal(constant(literal, &checks::path(spec, value))?, value)
            }
            ExprKind::Scalar { op, .. } if op.key_read() => Self::Read(
                Box::new(
                    crate::scalar_verb::Operation::from_tree(spec, value).map_err(refused_verb)?,
                ),
                value,
            ),
            ExprKind::Scalar { .. } | ExprKind::Formula(_) | ExprKind::Window(_) => {
                let scan = reads::Scan {
                    relation: true,
                    windows: true,
                };
                Self::Expression {
                    expr: value,
                    references: reads::names(spec, value, scan, calls)?,
                }
            }
            // A refused operation never becomes an executable plan.
            _ => Self::Literal(crate::value::Value::Missing, value),
        })
    }
    pub(super) fn reference(&self) -> Option<&str> {
        match self {
            Self::Variable(name) | Self::Source(name) => Some(name),
            Self::Read(operation, _) => {
                operation
                    .operands()
                    .first()
                    .and_then(|operand| match &operand.read {
                        crate::scalar_verb::Read::Name(name) => Some(name.as_str()),
                        _ => None,
                    })
            }
            Self::Literal(..) | Self::Expression { .. } => None,
        }
    }
    pub(super) fn references(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Self::Expression { references, .. } => Box::new(references.iter().map(String::as_str)),
            _ => Box::new(self.reference().into_iter()),
        }
    }
    pub(super) fn display_name(&self, field: &str) -> String {
        match self {
            Self::Variable(name) => name.clone(),
            _ => format!("key[{field}]"),
        }
    }
    /// The match value's result type (EXPR-14) over the names the current
    /// row's catalog types: a read name's type, or the written value's.
    /// LOOKUP-6 compares it with the key column's type.
    pub(super) fn result_type(
        &self,
        tree: &types::Tree,
        catalog: &Catalog,
    ) -> Result<ResultType, BindError> {
        let names = |name: &str| ResultType::declared(catalog.type_of(name));
        match self {
            Self::Variable(name) | Self::Source(name) => Ok(names(name)),
            Self::Literal(_, expr) | Self::Read(_, expr) | Self::Expression { expr, .. } => {
                Ok(tree.result_type(*expr, ast::Scope::Row, &names))
            }
        }
    }
    pub(super) fn bind(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
        context: Context<'_>,
    ) -> Result<Option<MatchValue>, BindError> {
        self.bind_in_context(path, intermediate, catalog, findings, false, context)
    }
    pub(super) fn bind_deferred(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
        context: Context<'_>,
    ) -> Result<Option<MatchValue>, BindError> {
        self.bind_in_context(path, intermediate, catalog, findings, true, context)
    }
    #[allow(clippy::too_many_arguments)]
    fn bind_in_context(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
        defer_unknown: bool,
        context: Context<'_>,
    ) -> Result<Option<MatchValue>, BindError> {
        if let Self::Expression { expr, references } = self {
            if defer_unknown {
                for name in references {
                    if catalog.bind(name).is_none() {
                        return Ok(Some(MatchValue::Contextual(
                            crate::dataset::ContextualMatch::Expression {
                                identifier: name.clone(),
                            },
                        )));
                    }
                }
            }
            let before = findings.len();
            let mut reads = Vec::new();
            let mut fields = Vec::new();
            for name in references {
                let Some(bound) = catalog.bind(name) else {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        path,
                        name,
                        Some(intermediate),
                        ReferenceCause::OutputKey,
                        None,
                    )));
                    continue;
                };
                let (read, kind) = match bound {
                    reference_binding::Binding::Output {
                        column,
                        column_type,
                    } => (
                        MatchRead::Output {
                            column,
                            identifier: name.clone(),
                        },
                        column_type,
                    ),
                    reference_binding::Binding::Dataset {
                        dataset: 0,
                        field,
                        column_type,
                    } => (
                        MatchRead::Record {
                            column: field,
                            identifier: name.clone(),
                        },
                        column_type,
                    ),
                    reference_binding::Binding::Dataset {
                        dataset,
                        field,
                        column_type,
                    } => (
                        MatchRead::SecondaryRecord {
                            source: dataset - 1,
                            column: field,
                            identifier: name.clone(),
                        },
                        column_type,
                    ),
                };
                if !reads
                    .iter()
                    .any(|prior: &MatchRead| prior.identifier() == name)
                {
                    reads.push(read);
                    fields.push(Column {
                        name: name.clone(),
                        kind,
                    });
                }
            }
            if findings.len() != before {
                return Ok(None);
            }
            let mut edges = Vec::new();
            let expression = BindContext {
                scope: binder::Scope {
                    template: context
                        .row
                        .map_or(binder::Template::Any, binder::Template::Row),
                    phase: context.phase,
                    driver: context.driver,
                    available: None,
                },
                donors: binder::Donors {
                    named: context.named,
                    secondary: context.secondary,
                    items: context.intermediates,
                },
                catalog,
                output: context.output,
                project: context.project,
                tree: context.tree,
                edges: &mut edges,
                findings: &mut *findings,
                reported: false,
            }
            .bind(*expr, context.output.columns().len())?;
            return expression
                .map(|expression| {
                    crate::dataset::MatchExpression::new(
                        Assignment {
                            column: context.output.columns().len(),
                            expression,
                            path: checks::path(context.tree.spec(), *expr),
                        },
                        reads,
                        TableSchema::new(fields).map_err(|_| BindError::Internal)?,
                        self.result_type(context.tree, catalog)?,
                    )
                    .map(|expression| MatchValue::Expression(Box::new(expression)))
                    .map_err(BindError::InvalidPlan)
                })
                .transpose();
        }
        if let Self::Read(operation, _) = self {
            findings.extend(operation.defect().cloned().map(BindFinding::Scalar));
        }
        let Some(name) = self.reference() else {
            let Self::Literal(value, _) = self else {
                return Err(BindError::Internal);
            };
            return Ok(Some(MatchValue::Literal(value.clone())));
        };
        // The catalog's own record is the primary input, or the donor record
        // in a donor context: a named intermediate's catalog lists the primary
        // first, an aggregate key binds against `aggregates::record_catalog`,
        // and a donor context leaves its donor out of `secondary`.
        let read = match catalog.bind(name) {
            Some(reference_binding::Binding::Output { column, .. }) => MatchRead::Output {
                column,
                identifier: name.into(),
            },
            Some(reference_binding::Binding::Dataset {
                dataset: 0, field, ..
            }) => MatchRead::Record {
                column: field,
                identifier: name.into(),
            },
            Some(reference_binding::Binding::Dataset { dataset, field, .. }) => {
                MatchRead::SecondaryRecord {
                    source: dataset - 1,
                    column: field,
                    identifier: name.into(),
                }
            }
            None if defer_unknown => {
                use crate::dataset::ContextualMatch;
                let declaration = match self {
                    Self::Variable(_) => ContextualMatch::Variable(name.into()),
                    Self::Source(_) => ContextualMatch::Source(name.into()),
                    Self::Read(operation, _) => operation
                        .contextual_match(name.into())
                        .ok_or(BindError::Internal)?,
                    Self::Literal(..) | Self::Expression { .. } => return Err(BindError::Internal),
                };
                return Ok(Some(MatchValue::Contextual(declaration)));
            }
            None => {
                let suggestion = name
                    .strip_prefix("SELF.")
                    .filter(|bare| {
                        catalog.bind(bare).is_some_and(|item| {
                            matches!(item, reference_binding::Binding::Output { .. })
                        })
                    })
                    .map(String::from);
                findings.push(BindFinding::Lookup(LookupFinding::reference(
                    path,
                    name,
                    Some(intermediate),
                    ReferenceCause::OutputKey,
                    suggestion,
                )));
                return Ok(None);
            }
        };
        Ok(Some(match self {
            Self::Variable(_) => MatchValue::Variable(read),
            Self::Source(_) => MatchValue::Source(read),
            Self::Read(operation, _) => operation.match_value(read).ok_or(BindError::Internal)?,
            Self::Literal(..) | Self::Expression { .. } => return Err(BindError::Internal),
        }))
    }
}

/// Reject intermediate and key counts before compiling their authored
/// expressions.
pub(super) fn declaration_admission(spec: &ast::Spec) -> Result<(), PrepareError> {
    if spec.intermediates.len() > crate::limits::Limits::COLUMNS {
        return Err(PrepareError::limit(
            "intermediates",
            crate::limits::Limits::COLUMNS,
            Some(spec.intermediates.len()),
            None,
        ));
    }
    for item in &spec.intermediates {
        if let Some(ast::JoinKey::Pairs { pairs, meta }) = &item.key {
            if pairs.len() > crate::limits::Limits::KEYS {
                return Err(PrepareError::limit(
                    "intermediate_keys",
                    crate::limits::Limits::KEYS,
                    Some(pairs.len()),
                    Some(&checks::render(spec, *meta)),
                ));
            }
        }
    }
    Ok(())
}
