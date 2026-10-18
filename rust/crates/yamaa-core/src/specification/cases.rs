//! Bounded ordered case lowering. Unselected branches have no runtime effects.
use super::*;
use crate::{
    dataset::{CaseBranch, Intermediate, SecondarySource},
    expression::Failure,
    project_call_binding,
};

#[derive(Clone, Debug)]
pub(super) struct Branch {
    when: Option<std::boxed::Box<Result<CompiledExpression, InvalidPredicate>>>,
    pub(super) path: String,
    pub(super) operation: std::boxed::Box<Operation>,
}
impl Branch {
    pub(super) fn value(operation: Operation, path: String) -> Self {
        Self {
            when: None,
            path,
            operation: std::boxed::Box::new(operation),
        }
    }
    pub(super) fn predicate(&self) -> Option<&Result<CompiledExpression, InvalidPredicate>> {
        self.when.as_deref()
    }
}
/// Keep the complete parser cause on the prepared branch; document reporting
/// projects it only after source ingestion reaches predicate binding.
#[derive(Clone, Debug)]
pub(super) struct InvalidPredicate {
    path: String,
    expression: String,
    position: usize,
    failure: Failure,
}
impl InvalidPredicate {
    pub(super) fn diagnostic(&self) -> Result<crate::diagnostic::Diagnostic, BindError> {
        Ok(self.failure.diagnostic(
            ExpressionContext::Predicate,
            &self.path,
            &self.expression,
            self.position,
        ))
    }
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
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            Some(path),
        ));
    }

    let items = sequence(d, node)?;

    // The schema owns REQ-0339 shape failures. These branches defend the
    // normalized-document contract rather than declare a language subset.
    if items.is_empty() {
        return Err(PrepareError::Internal);
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
                return Err(PrepareError::Internal);
            }
            (None, value, format!("{prefix}.otherwise"))
        } else {
            let when = text(d, field(d, item, "when")?)?;
            let compiled = compile(
                when,
                ExpressionContext::Predicate,
                &format!("{prefix}.when"),
            );
            let compiled = match compiled {
                Ok(plan) => Ok(plan),
                Err(CompileError::Invalid {
                    position, failure, ..
                }) => Err(InvalidPredicate {
                    path: format!("{prefix}.when"),
                    expression: when.into(),
                    position: position.character,
                    failure,
                }),
                Err(error) => {
                    return Err(PrepareError::expression(
                        error,
                        Some(&format!("{prefix}.when")),
                    ))
                }
            };
            (
                Some(std::boxed::Box::new(compiled)),
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
/// with the expression defects of its predicates and formulas, before any
/// reference binding. Consumers without a width defect retain their existing
/// binding checkpoint.
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
                    if let Some(Err(invalid)) = branch.when.as_deref() {
                        findings.push(BindFinding::Predicate(invalid.diagnostic()?));
                    }
                    append(&branch.operation, &branch.path, findings)?;
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
fn bind_when(
    spec: &PreparedSpecification,
    branch: &Branch,
    column: usize,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
) -> Result<Option<crate::bound_expression::BoundPredicate>, BindError> {
    let Some(compiled) = branch.when.as_deref() else {
        return Ok(None);
    };
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
}

fn bind_project_operation(
    spec: &PreparedSpecification,
    call: &usize,
    column: usize,
    catalog: &Catalog,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let calls = spec.project_calls.as_ref().ok_or(BindError::Internal)?;
    let record = spec.reads_one_record(column);
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
    Ok(
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
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn bind_compute_operation(
    spec: &PreparedSpecification,
    branch: &Branch,
    compiled: &CompiledExpression,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    start: usize,
) -> Result<Option<Expression>, BindError> {
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
        match catalog.bind(name) {
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
    static_types::numeric(
        compiled,
        &bindings,
        &spec.intermediates,
        catalog,
        secondary,
        intermediates,
        findings,
    )?;
    Ok(if findings.len() == start {
        Some(Expression::Compute(
            BoundNumeric::new(
                static_types::typed(
                    compiled,
                    &spec.intermediates,
                    catalog,
                    secondary,
                    intermediates,
                ),
                bindings,
            )
            .map_err(BindError::InvalidNumericBinding)?,
        ))
    } else {
        None
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_operation(
    spec: &PreparedSpecification,
    branch: &Branch,
    column: usize,
    catalog: &Catalog,
    driver: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
    start: usize,
) -> Result<Option<Expression>, BindError> {
    Ok(match branch.operation.as_ref() {
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
        Operation::Window(window) => window.bind(catalog, edges, findings)?,
        Operation::Flag(operation) => flags::bind(
            operation,
            spec.reads_one_record(column),
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
        Operation::ProjectFunction { call, .. } => {
            bind_project_operation(spec, call, column, catalog, edges, findings)?
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
        Operation::StrReplace(operation) => str_extract::bind_replace(
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
            &spec.intermediates,
            secondary,
            intermediates,
            spec.project_calls.as_ref(),
            edges,
            findings,
        )?,
        Operation::Compute(compiled) => bind_compute_operation(
            spec,
            branch,
            compiled,
            catalog,
            secondary,
            intermediates,
            edges,
            findings,
            start,
        )?,
        Operation::Odm(operation) => odm::bind(
            operation,
            catalog,
            &spec.source().name,
            None,
            spec.odm_phase(column),
            spec.record_rows(),
            findings,
        )?,
        Operation::FirstAvailable(selection) => derivations::bind_selection(
            selection,
            &branch.path,
            catalog,
            None,
            spec.record_rows(),
            column,
            &spec.output,
            &spec.intermediates,
            secondary,
            intermediates,
            edges,
            findings,
        )?,
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_branch(
    spec: &PreparedSpecification,
    branch: &Branch,
    column: usize,
    catalog: &Catalog,
    driver: &TableSchema,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
    start: usize,
) -> Result<Option<CaseBranch>, BindError> {
    let when = bind_when(
        spec,
        branch,
        column,
        catalog,
        secondary,
        intermediates,
        edges,
        findings,
        declarations_reported,
    )?;
    let expression = bind_operation(
        spec,
        branch,
        column,
        catalog,
        driver,
        secondary,
        intermediates,
        edges,
        findings,
        declarations_reported,
        start,
    )?;
    Ok(expression.map(|expression| CaseBranch {
        when,
        assignment: std::boxed::Box::new(Assignment {
            column,
            expression,
            path: branch.path.clone(),
        }),
    }))
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
        if let Some(bound) = bind_branch(
            spec,
            branch,
            column,
            catalog,
            driver,
            secondary,
            intermediates,
            edges,
            findings,
            declarations_reported,
            start,
        )? {
            result.push(bound);
        }
    }
    Ok((findings.len() == start).then_some(Expression::Case(result)))
}
