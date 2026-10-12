//! Bounded ordered case lowering. Unselected branches have no runtime effects.
use super::*;
use crate::{
    dataset::{CaseBranch, Intermediate, SecondarySource},
    predicate::Plan,
    predicate_compiler, project_call_binding,
};

#[derive(Clone, Debug)]
pub(super) struct Branch {
    when: Option<Result<Plan, InvalidPredicate>>,
    pub(super) path: String,
    pub(super) operation: alloc::boxed::Box<Operation>,
}
impl Branch {
    pub(super) fn value(operation: Operation, path: String) -> Self {
        Self {
            when: None,
            path,
            operation: alloc::boxed::Box::new(operation),
        }
    }
    pub(super) fn predicate(&self) -> Option<&Result<Plan, InvalidPredicate>> {
        self.when.as_ref()
    }
}
/// Keep the complete parser cause on the prepared branch; document reporting
/// projects it only after source ingestion reaches predicate binding.
#[derive(Clone, Debug)]
pub(super) struct InvalidPredicate {
    path: String,
    expression: String,
    error: crate::predicate_parser::ParseError,
}
impl InvalidPredicate {
    pub(super) fn diagnostic(&self) -> Result<crate::diagnostic::Diagnostic, BindError> {
        let crate::predicate_parser::ParseError::Grammar {
            position, failure, ..
        } = &self.error
        else {
            return Err(BindError::Internal);
        };
        failure
            .original_diagnostic(&self.path, &self.expression, position.character)
            .map_err(|_| BindError::PredicateDiagnosticLimit)
    }
}
fn unsupported(path: &str, operation: &str) -> PrepareError {
    PrepareError::Unsupported(vec![UnsupportedFeature {
        operation: operation.into(),
        path: path.into(),
    }])
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    d: &Document,
    node: usize,
    path: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    record: bool,
    depth: usize,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
) -> Result<Vec<Branch>, PrepareError> {
    if depth > 64 || path.len() > 65_536 {
        return Err(PrepareError::Limit("case_depth"));
    }
    let items = sequence(d, node)?;
    if items.is_empty() || items.len() > 1024 {
        return Err(unsupported(path, "case_shape"));
    }
    let mut branches = Vec::new();
    for (index, &item) in items.iter().enumerate() {
        let prefix = format!("{path}[{index}]");
        let (when, value, result_path) = if let Some(value) = d.field(item, "otherwise") {
            if index == 0
                || index + 1 != items.len()
                || d.field(item, "when").is_some()
                || d.field(item, "then").is_some()
            {
                return Err(unsupported(&prefix, "case_shape"));
            }
            (None, value, format!("{prefix}.otherwise"))
        } else {
            let when = text(d, field(d, item, "when")?)?;
            let compiled =
                predicate_compiler::compile(when, &format!("{prefix}.when"), Default::default());
            let compiled = match compiled {
                Ok(plan) => Ok(plan),
                Err(predicate_compiler::Error::Parse(
                    error @ crate::predicate_parser::ParseError::Grammar { .. },
                )) => Err(InvalidPredicate {
                    path: format!("{prefix}.when"),
                    expression: when.into(),
                    error,
                }),
                Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::UnsupportedRegex { .. },
                )) => return Err(unsupported(&format!("{prefix}.when"), "predicate_regex")),
                Err(predicate_compiler::Error::Internal) => return Err(PrepareError::Internal),
                Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
            };
            (
                Some(compiled),
                field(d, item, "then")?,
                format!("{prefix}.then"),
            )
        };
        let mut branch = derivations::prepare_value(
            d,
            value,
            &result_path,
            project,
            intermediates,
            record,
            depth,
            extraction,
            templates,
        )?;
        branch.when = when;
        branches.push(branch);
    }
    Ok(branches)
}

/// A width defect shares its derivation's authored declaration-validation order
/// with predicate and numeric grammar defects, before any reference binding.
/// Consumers without a width defect retain their existing binding checkpoint.
pub(super) fn padding_findings(
    operation: &Operation,
    findings: &mut Vec<BindFinding>,
) -> Result<bool, BindError> {
    fn has_width_error(operation: &Operation) -> bool {
        match operation {
            Operation::StrPad(operation) => operation.has_width_error(),
            Operation::Flag(operation) => operation.has_findings(),
            Operation::Case(branches) => branches
                .iter()
                .any(|branch| has_width_error(&branch.operation)),
            Operation::Scalar(operation) => match operation.as_ref() {
                scalar::Declaration::Concat { sources, .. } => sources
                    .iter()
                    .any(|branch| has_width_error(&branch.operation)),
                _ => false,
            },
            _ => false,
        }
    }
    fn append(
        operation: &Operation,
        path: &str,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        match operation {
            Operation::StrPad(operation) => str_pad::declaration_findings(operation, findings),
            Operation::Flag(operation) => operation.findings(findings),
            Operation::InvalidNumeric { expression, error } => {
                findings.push(BindFinding::Numeric {
                    path: format!("{path}.expr"),
                    expression: expression.clone(),
                    error: error.clone(),
                })
            }
            Operation::Case(branches) => {
                for branch in branches {
                    if let Some(Err(invalid)) = &branch.when {
                        findings.push(BindFinding::Predicate(invalid.diagnostic()?));
                    }
                    append(&branch.operation, &branch.path, findings)?;
                }
            }
            Operation::Scalar(operation) => {
                if let scalar::Declaration::Concat { sources, .. } = operation.as_ref() {
                    for branch in sources {
                        append(&branch.operation, &branch.path, findings)?;
                    }
                }
            }
            _ => (),
        }
        Ok(())
    }
    if !has_width_error(operation) {
        return Ok(false);
    }
    // A top-level numeric declaration cannot also contain padding. Its path is
    // only needed for the branch-owned InvalidNumeric cases visited by append.
    append(operation, "", findings)?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    spec: &PreparedSpecification,
    branches: &[Branch],
    column: usize,
    catalog: &Catalog,
    driver: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let mut result = Vec::new();
    for branch in branches {
        let when = branch
            .when
            .as_ref()
            .map(|compiled| {
                let plan = match compiled {
                    Ok(plan) => plan,
                    Err(invalid) => {
                        if !declarations_reported {
                            findings.push(BindFinding::Predicate(invalid.diagnostic()?));
                        }
                        return Ok(None);
                    }
                };
                case_predicates::bind(
                    spec,
                    plan,
                    column,
                    catalog,
                    secondary,
                    intermediates,
                    edges,
                    findings,
                )
            })
            .transpose()?
            .flatten();
        let expression = match branch.operation.as_ref() {
            Operation::InvalidNumeric { expression, error } => {
                if !declarations_reported {
                    findings.push(BindFinding::Numeric {
                        path: format!("{}.expr", branch.path),
                        expression: expression.clone(),
                        error: error.clone(),
                    });
                }
                None
            }
            Operation::Literal(value) => Some(Expression::Literal(value.clone())),
            Operation::Flag(operation) => flags::bind(
                operation,
                column,
                &spec.keys,
                &spec.source().name,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
                declarations_reported,
            )?,
            Operation::Case(branches) => bind(
                spec,
                branches,
                column,
                catalog,
                driver,
                secondary,
                intermediates,
                edges,
                findings,
                declarations_reported,
            )?,
            Operation::ProjectFunction(call) => {
                let calls = spec.project_calls.as_ref().ok_or(BindError::Internal)?;
                let record = spec.keys.contains(&column);
                let context = project_call_binding::Context {
                    source_dataset: 0,
                    source_mode: if record {
                        project_call_binding::SourceMode::Record
                    } else {
                        project_call_binding::SourceMode::Collect
                    },
                    available_outputs: None,
                    scope: crate::reference_scope::Scope {
                        drivers: &[spec.source().name.as_str()],
                        current_driver: true,
                        reach: if record {
                            crate::reference_scope::Reach::Record
                        } else {
                            crate::reference_scope::Reach::Relation
                        },
                        joined: false,
                        phase: crate::reference_scope::Phase::Column { groups: &[] },
                    },
                };
                match project_call_binding::bind(calls, *call, catalog, context) {
                    Ok(bound) => {
                        edges.extend(bound.dependencies);
                        Some(Expression::ProjectFunction(bound.function))
                    }
                    Err(project_call_binding::Error::Findings(errors)) => {
                        findings.extend(functions::bind_findings(calls, *call, errors, None)?);
                        None
                    }
                    Err(project_call_binding::Error::Reference(error)) => {
                        return Err(BindError::Catalog(error))
                    }
                    Err(_) => return Err(BindError::Internal),
                }
            }
            Operation::StrTemplate(operation) => str_template::bind(
                operation,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::Scalar(operation) => scalar::bind(
                spec,
                operation,
                column,
                catalog,
                driver,
                secondary,
                intermediates,
                edges,
                findings,
                declarations_reported,
            )?,
            Operation::StrContains(operation) => scalar::bind_contains(
                spec,
                operation,
                column,
                catalog,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::StrExtract(operation) => str_extract::bind(
                operation,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::MappingCut(operation) => mapping_cut::bind(
                spec,
                operation,
                column,
                catalog,
                driver,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::StrCase(operation) => str_case::bind(
                operation,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::StrPad(operation) => str_pad::bind(
                operation,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::Imputation(operation) => imputation::bind(
                operation,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::Temporal(operation) => temporal::bind(
                operation,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::Round(rounding) => rounding::bind(
                rounding,
                column,
                &spec.keys,
                catalog,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::Source(source) => source_expressions::bind(
                spec,
                source,
                column,
                &branch.path,
                catalog,
                driver,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            Operation::Aggregate(operation) => aggregates::bind(
                operation,
                catalog,
                &spec.source().name,
                None,
                crate::reference_scope::Phase::Column { groups: &[] },
                &spec.output,
                secondary,
                edges,
                findings,
            )?,
            Operation::Compute(compiled) => {
                let mut bindings = Vec::new();
                for name in compiled.identifiers() {
                    if let Some((index, field)) = spec.intermediates.reference(name) {
                        if let Some(item) = &intermediates[index] {
                            edges.extend(row_intermediates::dependencies(index, intermediates)?);
                            if let Some(column) = item.source_column(secondary, field) {
                                bindings.push(Binding {
                                    name: name.into(),
                                    read: Read::Intermediate { index, column },
                                });
                                continue;
                            }
                        }
                        findings.push(BindFinding::UnknownReference {
                            path: format!("{}.expr", branch.path),
                            name: name.into(),
                        });
                        continue;
                    }
                    match catalog.bind(name).map_err(BindError::Catalog)? {
                        Some(reference_binding::Binding::Output { column, .. }) => {
                            edges.push(column);
                            bindings.push(Binding {
                                name: name.into(),
                                read: Read::Column(column),
                            });
                        }
                        _ => findings.push(BindFinding::UnknownReference {
                            path: format!("{}.expr", branch.path),
                            name: name.into(),
                        }),
                    }
                }
                if findings.len() == start {
                    Some(Expression::Compute(
                        BoundNumeric::new(compiled.clone(), bindings)
                            .map_err(BindError::InvalidNumericBinding)?,
                    ))
                } else {
                    None
                }
            }
            Operation::FirstAvailable(selection) => derivations::bind_selection(
                selection,
                &branch.path,
                catalog,
                None,
                column,
                &spec.output,
                &spec.intermediates,
                secondary,
                intermediates,
                edges,
                findings,
            )?,
            _ => return Err(BindError::Internal),
        };
        if let Some(expression) = expression {
            result.push(CaseBranch {
                when,
                assignment: alloc::boxed::Box::new(Assignment {
                    column,
                    expression,
                    path: branch.path.clone(),
                }),
            });
        }
    }
    Ok((findings.len() == start).then_some(Expression::Case(result)))
}

/// Charge all nested formulas and case predicates before owning any branch plan.
pub(super) fn charge(
    d: &Document,
    value: usize,
    total: &mut usize,
    limit: usize,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("case_depth"));
    }
    if let Some(flag) = d.field(value, "flag") {
        let condition = if matches!(d.nodes()[flag], N::Text(_)) {
            flag
        } else {
            field(d, flag, "condition")?
        };
        *total = total
            .checked_add(text(d, condition)?.len())
            .filter(|&n| n <= limit)
            .ok_or(PrepareError::Limit("numeric_bytes"))?;
    }
    if let Some(payload) = d.field(value, "str_concat") {
        for &child in sequence(d, field(d, payload, "sources")?)? {
            if let Some(expr) = d.field(child, "compute").and_then(|id| d.field(id, "expr")) {
                *total = total
                    .checked_add(text(d, expr)?.len())
                    .filter(|&n| n <= limit)
                    .ok_or(PrepareError::Limit("numeric_bytes"))?;
            }
            charge(d, child, total, limit, depth + 1)?;
        }
    }
    let Some(case) = d.field(value, "case") else {
        return Ok(());
    };
    for &branch in sequence(d, case)? {
        let when = d.field(branch, "when");
        let child = d
            .field(branch, "then")
            .or_else(|| d.field(branch, "otherwise"));
        let compute = child
            .and_then(|id| d.field(id, "compute"))
            .and_then(|id| d.field(id, "expr"));
        let filter = child
            .and_then(|id| d.field(id, "source"))
            .and_then(|id| d.field(id, "filter"))
            .filter(|&id| !matches!(d.nodes()[id], N::Null));
        for id in when.into_iter().chain(compute).chain(filter) {
            *total = total
                .checked_add(text(d, id)?.len())
                .filter(|&count| count <= limit)
                .ok_or(PrepareError::Limit("numeric_bytes"))?;
        }
        if let Some(child) = child {
            charge(d, child, total, limit, depth + 1)?;
        }
    }
    Ok(())
}
