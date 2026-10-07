//! Closed row lowering with per-template defaults and shared group-scope validation.
//! Row-output dependencies and broader expressions remain explicit follow-up work.
use super::*;
use crate::dataset::Verification;
use yamaa_core::{
    aggregate_parser::{parse_aggregate, ParsedKind, Reducer},
    reduction::NumericReducer,
    reference_scope::{self, Phase, Reach, Scope},
    value::Value,
};

#[derive(Debug)]
enum RowOperation {
    Source(String),
    InvalidAggregate {
        expression: String,
        error: yamaa_core::aggregate_parser::GrammarFailure,
    },
    Literal(Value),
    Sum {
        name: String,
        expression: String,
    },
}
#[derive(Debug)]
struct RowDeclaration {
    column: usize,
    path: String,
    operation: RowOperation,
}
#[derive(Debug)]
struct Template {
    id: String,
    groups: Option<Vec<String>>,
    declarations: Vec<RowDeclaration>,
}
#[derive(Debug)]
pub(super) struct Rows {
    templates: Vec<Template>,
    columns: Vec<RowDeclaration>,
}
fn unsupported(operation: &str, path: &str) -> PrepareError {
    PrepareError::Unsupported(vec![UnsupportedFeature {
        operation: operation.into(),
        path: path.into(),
    }])
}
fn closed_fields(
    d: &Document,
    id: usize,
    allowed: &[&str],
    path: &str,
) -> Result<(), PrepareError> {
    for &(name, _) in mapping(d, id)? {
        let name = text(d, name)?;
        if !allowed.contains(&name) {
            return Err(unsupported(name, &format!("{path}.{name}")));
        }
    }
    Ok(())
}
fn declaration(
    d: &Document,
    id: usize,
    column: usize,
    prefix: String,
    grouped: bool,
) -> Result<RowDeclaration, PrepareError> {
    closed_fields(d, id, &["value"], &prefix)?;
    let &[(op, payload)] = mapping(d, field(d, id, "value")?)? else {
        return Err(PrepareError::Internal);
    };
    let op = text(d, op)?;
    let path = format!("{prefix}.{op}");
    let operation = match op {
        "source" => {
            closed_fields(d, payload, &["variable"], &path)?;
            let name = text(d, field(d, payload, "variable")?)?;
            if !name.contains('.') {
                return Err(unsupported("row_output_reference", &path));
            }
            RowOperation::Source(name.into())
        }
        "literal" => RowOperation::Literal(match &d.nodes()[payload] {
            N::Null => Value::Missing,
            N::Text(value) => Value::Str(value.clone()),
            N::Boolean(value) => Value::Bool(*value),
            N::Float(value) => Value::float(*value),
            N::Integer(value) => Value::Int(
                value
                    .parse()
                    .map_err(|_| unsupported("wide_integer_literal", &path))?,
            ),
            _ => return Err(unsupported("literal", &path)),
        }),
        "aggregate" if grouped => {
            closed_fields(d, payload, &["expr"], &path)?;
            let expression = text(d, field(d, payload, "expr")?)?;
            let parsed = match parse_aggregate(expression, Default::default()) {
                Ok(parsed) => parsed,
                Err(yamaa_core::aggregate_parser::ParseError::Grammar { failure, .. }) => {
                    return Ok(RowDeclaration {
                        column,
                        path,
                        operation: RowOperation::InvalidAggregate {
                            expression: expression.into(),
                            error: failure,
                        },
                    })
                }
                Err(yamaa_core::aggregate_parser::ParseError::Limit { .. }) => {
                    return Err(PrepareError::Limit("aggregate_parser"))
                }
            };
            let ParsedKind::Reduction {
                reducer: Reducer::Sum,
                operand,
                ..
            } = parsed.nodes()[parsed.root()].kind
            else {
                return Err(unsupported("aggregate_expression", &path));
            };
            if !matches!(parsed.nodes()[operand].kind, ParsedKind::Identifier) {
                return Err(unsupported("aggregate_argument", &path));
            }
            let span = parsed.nodes()[operand].span;
            RowOperation::Sum {
                name: expression[span.start..span.end].into(),
                expression: expression.into(),
            }
        }
        _ => return Err(unsupported(op, &path)),
    };
    Ok(RowDeclaration {
        column,
        path,
        operation,
    })
}
impl Rows {
    pub(super) fn prepare(
        d: &Document,
        output: &TableSchema,
        driver: &str,
        limits: CompilationLimits,
    ) -> Result<Self, PrepareError> {
        let root = d.root();
        let raw_rows = sequence(d, field(d, root, "rows")?)?;
        if raw_rows.is_empty() || raw_rows.len() > 16 {
            return Err(PrepareError::Limit("row_templates"));
        }
        let columns = sequence(d, field(d, root, "columns")?)?;
        let mut templates = Vec::new();
        let mut selected = Vec::new();
        for (column, metadata) in output.columns().iter().enumerate() {
            if raw_rows.iter().any(|&row| {
                d.field(row, "derivations")
                    .is_some_and(|id| d.field(id, &metadata.name).is_some())
            }) {
                selected.push(column);
                // Admit every written default, even when all templates override
                // it. Actual inherited defaults are lowered in each row's scope.
                if let Some(id) = d
                    .field(columns[column], "derivation")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    declaration(
                        d,
                        id,
                        column,
                        format!("columns.{}.derivation", metadata.name),
                        false,
                    )?;
                }
            }
        }
        for (index, &row) in raw_rows.iter().enumerate() {
            let path = format!("rows[{index}]");
            closed_fields(d, row, &["id", "dataset", "group_by", "derivations"], &path)?;
            if let Some(id) = d
                .field(row, "dataset")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                if text(d, id)? != driver {
                    return Err(unsupported("row_driver", &format!("{path}.dataset")));
                }
            }
            let groups = d
                .field(row, "group_by")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| {
                    let values = sequence(d, id)?;
                    if values.len() > limits.keys {
                        return Err(PrepareError::Limit("group_keys"));
                    }
                    values
                        .iter()
                        .map(|&id| text(d, id).map(String::from))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let entries = mapping(d, field(d, row, "derivations")?)?;
            if entries.len() > limits.columns {
                return Err(PrepareError::Limit("row_columns"));
            }
            for &(name, _) in entries {
                if !output
                    .columns()
                    .iter()
                    .any(|c| text(d, name).is_ok_and(|name| name == c.name))
                {
                    return Err(unsupported("undeclared_row_column", &path));
                }
            }
            let mut declarations = Vec::new();
            for &column in &selected {
                let metadata = &output.columns()[column];
                let (id, prefix) = if let Some(&(_, id)) = entries
                    .iter()
                    .find(|&&(name, _)| text(d, name).is_ok_and(|name| name == metadata.name))
                {
                    (id, format!("{path}.derivations.{}", metadata.name))
                } else {
                    (
                        field(d, columns[column], "derivation")?,
                        format!("columns.{}.derivation", metadata.name),
                    )
                };
                declarations.push(declaration(d, id, column, prefix, groups.is_some())?);
            }
            templates.push(Template {
                id: text(d, field(d, row, "id")?)?.into(),
                groups,
                declarations,
            });
        }
        let mut lowered_columns = Vec::new();
        for (column, &id) in columns.iter().enumerate() {
            let prefix = format!("columns.{}", output.columns()[column].name);
            for name in ["verifications", "submission", "metadata"] {
                if present(d, id, name) {
                    return Err(unsupported(name, &format!("{prefix}.{name}")));
                }
            }
            if !selected.contains(&column) {
                let Some(derivation) = d
                    .field(id, "derivation")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                else {
                    return Err(unsupported("missing_column", &prefix));
                };
                lowered_columns.push(declaration(
                    d,
                    derivation,
                    column,
                    format!("{prefix}.derivation"),
                    false,
                )?);
            }
        }
        Ok(Self {
            templates,
            columns: lowered_columns,
        })
    }
    pub(super) fn bind(
        &self,
        source: &TableSchema,
        name: &str,
        output: &TableSchema,
        keys: &[usize],
        verifications: &[Verification],
    ) -> Result<DatasetPlan, BindError> {
        let output_fields = output
            .columns()
            .iter()
            .map(|c| reference_binding::Field {
                name: &c.name,
                column_type: c.kind,
            })
            .collect::<Vec<_>>();
        let source_fields = source
            .columns()
            .iter()
            .map(|c| reference_binding::Field {
                name: &c.name,
                column_type: c.kind,
            })
            .collect::<Vec<_>>();
        let catalog = Catalog::compile(
            &output_fields,
            &[reference_binding::Dataset {
                name,
                fields: &source_fields,
            }],
            Default::default(),
        )
        .map_err(BindError::Catalog)?;
        let groups = self
            .templates
            .iter()
            .filter_map(|t| t.groups.as_ref())
            .map(|g| g.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let column_groups = groups.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let mut findings = Vec::new();
        let resolve = |reference: &str, path: &str, findings: &mut Vec<BindFinding>| {
            let selected = reference
                .split_once('.')
                .filter(|(dataset, _)| *dataset == name)
                .and_then(|(_, field)| source.columns().iter().position(|c| c.name == field));
            if selected.is_none() {
                findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: reference.into(),
                });
            }
            selected
        };
        let assignment = |declaration: &RowDeclaration,
                          phase: Phase<'_>,
                          row: Option<&str>,
                          findings: &mut Vec<BindFinding>|
         -> Result<Option<Assignment>, BindError> {
            let expression = match &declaration.operation {
                RowOperation::Literal(value) => Some(Expression::Literal(value.clone())),
                RowOperation::InvalidAggregate { expression, error } => {
                    findings.push(BindFinding::Aggregate {
                        path: declaration.path.clone(),
                        expression: expression.clone(),
                        error: error.clone(),
                    });
                    None
                }
                RowOperation::Source(reference) => {
                    if let Some(column) = resolve(reference, &declaration.path, findings) {
                        let scoped = reference_scope::validate(
                            &catalog,
                            reference,
                            None,
                            Scope {
                                drivers: &[name],
                                current_driver: false,
                                reach: Reach::Scalar,
                                joined: false,
                                phase,
                            },
                            Default::default(),
                        )
                        .map_err(BindError::Catalog)?;
                        let valid = scoped.is_empty();
                        findings.extend(scoped.into_iter().map(|finding| {
                            BindFinding::QualifiedReference {
                                path: declaration.path.clone(),
                                name: reference.clone(),
                                row: row.map(String::from),
                                finding,
                            }
                        }));
                        valid.then_some(Expression::Source(column))
                    } else {
                        None
                    }
                }
                RowOperation::Sum { name, expression } => {
                    resolve(name, &declaration.path, findings).map(|column| Expression::Reduce {
                        identifier: Some(name.clone()),
                        column,
                        reducer: NumericReducer::Sum,
                        text: expression.clone(),
                    })
                }
            };
            Ok(expression.map(|expression| Assignment {
                column: declaration.column,
                expression,
                path: declaration.path.clone(),
            }))
        };
        let mut templates = Vec::new();
        for (index, template) in self.templates.iter().enumerate() {
            let mode = match &template.groups {
                None => RowMode::Records,
                Some(groups) => RowMode::Groups(
                    groups
                        .iter()
                        .enumerate()
                        .filter_map(|(position, name)| {
                            resolve(
                                name,
                                &format!("rows[{index}].group_by[{position}]"),
                                &mut findings,
                            )
                        })
                        .collect(),
                ),
            };
            let group_names = template
                .groups
                .as_ref()
                .map(|g| g.iter().map(String::as_str).collect::<Vec<_>>());
            let phase = Phase::Row {
                group_by: group_names.as_deref(),
            };
            let mut assignments = Vec::new();
            for declaration in &template.declarations {
                if let Some(value) =
                    assignment(declaration, phase, Some(&template.id), &mut findings)?
                {
                    assignments.push(value);
                }
            }
            templates.push(RowTemplate {
                mode,
                assignments,
                filter: None,
            });
        }
        let mut columns = Vec::new();
        for declaration in &self.columns {
            if let Some(value) = assignment(
                declaration,
                Phase::Column {
                    groups: &column_groups,
                },
                None,
                &mut findings,
            )? {
                columns.push(value);
            }
        }
        if !findings.is_empty() {
            return Err(BindError::Invalid(findings));
        }
        DatasetPlan::new(
            source.clone(),
            output.clone(),
            templates,
            columns,
            keys.to_vec(),
            verifications.to_vec(),
        )
        .map_err(BindError::InvalidPlan)
    }
}
