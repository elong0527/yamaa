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
/// document node, which the tree types.
#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Variable(String),
    Source(String),
    Literal(crate::value::Value, usize),
    StrPad(str_pad::Declaration, usize),
    Expression {
        branch: cases::Branch,
        references: Vec<String>,
    },
}
impl Declaration {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        d: &Document,
        value: usize,
        path: &str,
        _sources: &[SourceDeclaration],
        _driver: usize,
        project: Option<&functions::Prepared>,
        extraction: &str_extract::Prepared,
        templates: &str_template::Prepared,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        if let N::Text(name) = &d.nodes()[value] {
            return Ok(Self::Variable(name.clone()));
        }
        let &[(operation, payload)] = mapping(d, value)? else {
            return Err(PrepareError::Internal);
        };
        let op = text(d, operation)?;
        let operation_path = format!("{path}.{op}");
        let declaration = match op {
            "literal" => Self::Literal(literal(d, payload, &operation_path)?, value),
            "source" => {
                let name = if let N::Text(name) = &d.nodes()[payload] {
                    name.as_str()
                } else {
                    for &(key, value) in mapping(d, payload)? {
                        let key = text(d, key)?;
                        if key != "variable" && !matches!(d.nodes()[value], N::Null) {
                            reject(
                                extra,
                                "intermediate_key_source",
                                format!("{operation_path}.{key}"),
                            );
                        }
                    }
                    text(d, field(d, payload, "variable")?)?
                };
                Self::Source(name.into())
            }
            "str_pad" => Self::StrPad(
                str_pad::prepare(d, payload, &operation_path, None, None, extra)?,
                value,
            ),
            "str_case"
            | "impute"
            | "to_date"
            | "precision"
            | "imputation_flag"
            | "round_half_away_from_zero"
            | "diff"
            | "study_day"
            | "to_epoch_day"
            | "mapping"
            | "cut"
            | "greatest"
            | "least"
            | "str_extract"
            | "str_replace"
            | "str_template"
            | "compute"
            | "row_number"
            | "rank"
            | "baseline_flag"
            | "row_value"
            | "locf"
            | "shift"
            | "duration_value"
            | "dtc"
            | "time_of_day" => {
                let mut references = Vec::new();
                derivations::references(d, value, project, &mut references, 0)?;
                self_relations::template_references(d, value, templates, &mut references, 0)?;
                let branch = derivations::prepare_value(
                    d,
                    value,
                    path,
                    project,
                    &intermediates::Declarations::default(),
                    false,
                    0,
                    extraction,
                    templates,
                )?;
                reject_deferred_owners(&branch, extra);
                Self::Expression { branch, references }
            }
            _ => {
                reject(extra, op, operation_path);
                // A rejected operation never becomes an executable plan.
                Self::Literal(crate::value::Value::Missing, value)
            }
        };
        if let Some(name) = declaration.expression_reference() {
            if let Some((relation, _)) = name.split_once('.') {
                if d.field(d.root(), "intermediates").is_some_and(|items| {
                    sequence(d, items).is_ok_and(|items| {
                        items.iter().any(|&item| {
                            d.field(item, "id")
                                .is_some_and(|id| text(d, id).is_ok_and(|name| name == relation))
                        })
                    })
                }) {
                    reject(extra, "intermediate_key_expression", path.into());
                }
            }
        }
        Ok(declaration)
    }
    fn expression_reference(&self) -> Option<&str> {
        match self {
            Self::Source(name) => Some(name),
            Self::StrPad(pad, _) => Some(pad.source()),
            _ => None,
        }
    }
    pub(super) fn reference(&self) -> Option<&str> {
        match self {
            Self::Variable(name) | Self::Source(name) => Some(name),
            Self::StrPad(pad, _) => Some(pad.source()),
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
            Self::Literal(_, value) | Self::StrPad(_, value) => {
                tree.result_type(*value, ast::Scope::Row, &names)
            }
            Self::Expression { branch, .. } => {
                tree.result_type(branch.value, ast::Scope::Row, &names)
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
        if let Self::Expression {
            branch, references, ..
        } = self
        {
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
            let expression = derivations::bind_row(
                &branch.operation,
                &branch.path,
                context.output.columns().len(),
                catalog,
                context.driver,
                context.row,
                context.phase,
                None,
                &mut edges,
                findings,
                false,
                context.output,
                context.project,
                context.tree,
                context.named,
                context.secondary,
                context.intermediates,
            )?;
            return expression
                .map(|expression| {
                    crate::dataset::MatchExpression::new(
                        Assignment {
                            column: context.output.columns().len(),
                            expression,
                            path: branch.path.clone(),
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
        if let Self::StrPad(operation, _) = self {
            str_pad::declaration_findings(operation, findings);
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
                    Self::StrPad(operation, _) => ContextualMatch::StrPad {
                        identifier: name.into(),
                        operation: match operation {
                            str_pad::Declaration::Valid(operation) => Some(operation.clone()),
                            str_pad::Declaration::WidthError { .. } => None,
                        },
                    },
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
            Self::StrPad(operation, _) => MatchValue::StrPad {
                read,
                operation: match operation {
                    str_pad::Declaration::Valid(operation) => Some(operation.clone()),
                    str_pad::Declaration::WidthError { .. } => None,
                },
            },
            Self::Literal(..) | Self::Expression { .. } => return Err(BindError::Internal),
        }))
    }
}

/// Conditional and callback relation contexts need a demand-aware key owner.
/// Keep them outside this bounded scalar/window expansion rather than
/// pre-reading their unselected branches.
fn reject_deferred_owners(branch: &cases::Branch, extra: &mut Vec<UnsupportedFeature>) {
    let operation = match branch.operation.as_ref() {
        Operation::Case(_) => Some("case"),
        Operation::ProjectFunction { .. } => Some("function"),
        Operation::Aggregate(_) => Some("aggregate"),
        Operation::Odm(_) => Some("odm"),
        Operation::Flag(_) => Some("flag"),
        Operation::FirstAvailable(_) => Some("first_available"),
        Operation::Imputation(operation) if operation.has_conditional_operands() => {
            for (field, _) in operation.operands().skip(1) {
                reject(extra, field, format!("{}.{field}", branch.path));
            }
            None
        }
        _ => None,
    };
    if let Some(operation) = operation {
        reject(extra, operation, branch.path.clone());
    }
}

/// Reject aggregate key counts before compiling their authored expressions.
pub(super) fn declaration_admission(d: &Document) -> Result<(), PrepareError> {
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(());
    };
    let items = sequence(d, items)?;
    if items.len() > crate::limits::Limits::COLUMNS {
        return Err(PrepareError::limit(
            "intermediates",
            crate::limits::Limits::COLUMNS,
            Some(items.len()),
            None,
        ));
    }
    for (index, &item) in items.iter().enumerate() {
        if let Some(key) = d.field(item, "key") {
            if let N::Mapping(fields) = &d.nodes()[key] {
                if fields.len() > crate::limits::Limits::KEYS {
                    return Err(PrepareError::limit(
                        "intermediate_keys",
                        crate::limits::Limits::KEYS,
                        Some(fields.len()),
                        Some(&format!("intermediates[{index}].key")),
                    ));
                }
            }
        }
    }
    Ok(())
}
