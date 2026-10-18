//! Shared expression lowering for column, case and row declarations.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{Intermediate, SecondarySource},
    reference_scope::{Phase, Reach, Scope},
};
fn unsupported(path: &str, operation: &str) -> PrepareError {
    PrepareError::Unsupported(vec![UnsupportedFeature {
        operation: operation.into(),
        path: path.into(),
    }])
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_value(
    d: &Document,
    value: usize,
    result_path: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    record: bool,
    depth: usize,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
) -> Result<cases::Branch, PrepareError> {
    prepare_value_with_driver(
        d,
        value,
        result_path,
        project,
        intermediates,
        record,
        depth,
        extraction,
        templates,
        None,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_value_with_driver(
    d: &Document,
    value: usize,
    result_path: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    record: bool,
    depth: usize,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
    driver: Option<&str>,
) -> Result<cases::Branch, PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            Some(result_path),
        ));
    }
    let &[(op, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let op = text(d, op)?;
    let result_path = format!("{result_path}.{op}");
    let operation = match op {
        "odm" => Operation::Odm(std::boxed::Box::new(odm::prepare(
            d,
            payload,
            &result_path,
        )?)),
        "aggregate" => Operation::Aggregate(std::boxed::Box::new(aggregates::prepare(
            d,
            payload,
            &result_path,
            project,
            intermediates,
            depth,
            extraction,
            templates,
        )?)),
        "flag" => Operation::Flag(flags::prepare(d, payload, &result_path)?),
        "literal" => Operation::Literal(literal(d, payload, &result_path)?),
        "function" => Operation::ProjectFunction(
            *project
                .and_then(|p| p.nodes.get(&payload))
                .ok_or(PrepareError::Internal)?,
        ),
        "case" => Operation::Case(cases::prepare(
            d,
            payload,
            &result_path,
            project,
            intermediates,
            record,
            depth + 1,
            extraction,
            templates,
        )?),
        "str_template" => Operation::StrTemplate(templates.get(payload)?),
        "str_extract" => Operation::StrExtract(extraction.get(payload)?),
        "str_replace" => Operation::StrReplace(extraction.replacement(payload)?),
        "greatest" | "least" => Operation::Scalar(std::boxed::Box::new(scalar::prepare(
            d,
            payload,
            op,
            &result_path,
            depth + 1,
        )?)),
        "mapping" | "cut" => {
            let mut features = Vec::new();
            let declaration =
                mapping_cut::prepare(d, payload, op, &result_path, intermediates, &mut features)?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::MappingCut(std::boxed::Box::new(declaration))
        }
        "str_case" => {
            let mut features = Vec::new();
            let operation = str_case::prepare(
                d,
                payload,
                &result_path,
                driver,
                Some(intermediates),
                &mut features,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::StrCase(operation)
        }
        "str_pad" => {
            let mut features = Vec::new();
            let operation = str_pad::prepare(
                d,
                payload,
                &result_path,
                driver,
                Some(intermediates),
                &mut features,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::StrPad(operation)
        }
        "impute" | "precision" | "imputation_flag" | "to_date" => {
            let mut features = Vec::new();
            let operation = imputation::prepare(
                d,
                payload,
                op,
                &result_path,
                driver,
                Some(intermediates),
                &mut features,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::Imputation(operation)
        }
        "diff" | "study_day" | "to_epoch_day" | "shift" | "duration_value" => {
            let mut features = Vec::new();
            let operation = temporal::prepare(
                d,
                payload,
                op,
                &result_path,
                driver,
                Some(intermediates),
                &mut features,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::Temporal(operation)
        }
        "round_half_away_from_zero" => {
            let mut features = Vec::new();
            let rounding = rounding::prepare(
                d,
                payload,
                &result_path,
                driver,
                Some(intermediates),
                &mut features,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::Round(rounding)
        }
        "first_available" => {
            let mut features = Vec::new();
            let operation = source_expressions::FirstAvailable::prepare(
                d,
                payload,
                &result_path,
                &mut features,
                intermediates,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::FirstAvailable(std::boxed::Box::new(operation))
        }
        "row_number" | "rank" | "row_value" | "locf" | "baseline_flag" => {
            let operation = windows::Declaration::prepare(d, payload, op, &result_path)?;
            Operation::Window(std::boxed::Box::new(operation))
        }
        "compute" => {
            for &(name, _) in mapping(d, payload)? {
                let name = text(d, name)?;
                if name != "expr" {
                    return Err(unsupported(&format!("{result_path}.{name}"), name));
                }
            }
            let expression = text(d, field(d, payload, "expr")?)?;
            match compile(expression, ExpressionContext::Compute, &result_path) {
                Ok(compiled) => Operation::Compute(compiled),
                Err(error @ CompileError::Invalid { .. }) => Operation::InvalidNumeric {
                    expression: expression.into(),
                    error,
                },
                Err(error) => {
                    return Err(PrepareError::Numeric {
                        path: format!("{result_path}.expr"),
                        error,
                    })
                }
            }
        }
        "source" => {
            let mut features = Vec::new();
            let source = source_expressions::Declaration::prepare(
                d,
                payload,
                &result_path,
                &mut features,
                intermediates,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::Source(std::boxed::Box::new(source))
        }
        _ => return Err(unsupported(&result_path, op)),
    };
    Ok(cases::Branch::value(operation, result_path))
}

#[allow(clippy::too_many_arguments)]
fn bind_row_compute(
    compiled: &CompiledExpression,
    path: &str,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
) -> Result<Option<Expression>, BindError> {
    let before = findings.len();
    let mut bindings = Vec::new();
    for identifier in compiled.identifiers() {
        if !catalog.record_derivation()
            && identifier.split_once('.').is_some_and(|(dataset, _)| {
                named.reference(identifier).is_none()
                    && (matches!(phase, Phase::Column { .. }) || catalog.joined(dataset).is_some())
            })
        {
            match phase {
                Phase::Column { .. } => findings.push(BindFinding::QualifiedNumericReference {
                    path: format!("{path}.expr"),
                    expression: compiled.expression().into(),
                    identifier: identifier.into(),
                }),
                Phase::Row { group_by } => {
                    let mut finding = LookupFinding::new(
                        &format!("{path}.expr"),
                        if group_by.is_some() {
                            crate::diagnostic::ConditionCode::GroupedRowNumericSourceScope
                        } else {
                            crate::diagnostic::ConditionCode::RowNumericSourceScope
                        },
                    );
                    finding.text("identifier", identifier);
                    finding.texts("drivers", &[driver]);
                    findings.push(BindFinding::Lookup(finding));
                }
            }
            continue;
        }
        if let Some(expression) = self_relations::bind_operand(
            identifier,
            &format!("{path}.expr"),
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        )? {
            let read = match expression {
                Expression::Source(column) => Read::Source(column),
                Expression::Column(column) => Read::Column(column),
                Expression::Intermediate { index, column } => Read::Intermediate { index, column },
                _ => return Err(BindError::Internal),
            };
            bindings.push(Binding {
                name: identifier.into(),
                read,
            });
        }
    }
    static_types::numeric(
        compiled,
        &bindings,
        named,
        catalog,
        secondary,
        intermediates,
        findings,
    )?;
    (findings.len() == before)
        .then(|| BoundNumeric::new(compiled.clone(), bindings).map(Expression::Compute))
        .transpose()
        .map_err(BindError::InvalidNumericBinding)
}

#[allow(clippy::too_many_arguments)]
fn bind_row_project(
    call: &usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    project: Option<&crate::project_calls::ProjectCalls>,
) -> Result<Option<Expression>, BindError> {
    let project = project.ok_or(BindError::Internal)?;
    let context = crate::project_call_binding::Context {
        source_dataset: 0,
        source_mode: crate::project_call_binding::SourceMode::Record,
        available_outputs: available,
        scope: Scope {
            drivers: &[driver],
            current_driver: true,
            reach: Reach::Scalar,
            joined: false,
            phase,
        },
    };
    match crate::project_call_binding::bind(project, *call, catalog, context) {
        Ok(bound) => {
            edges.extend(bound.dependencies);
            Ok(Some(Expression::ProjectFunction(bound.function)))
        }
        Err(crate::project_call_binding::Error::Findings(errors)) => {
            findings.extend(functions::bind_findings(project, *call, errors, row)?);
            Ok(None)
        }
        Err(crate::project_call_binding::Error::Reference(error)) => Err(BindError::Catalog(error)),
        Err(_) => Err(BindError::Internal),
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_row_case(
    branches: &[cases::Branch],
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    reported: bool,
    output: &TableSchema,
    project: Option<&crate::project_calls::ProjectCalls>,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
) -> Result<Option<Expression>, BindError> {
    let before = findings.len();
    let mut result = Vec::new();
    for branch in branches {
        let when = match branch.predicate() {
            Some(Err(invalid)) => {
                if !reported {
                    findings.push(BindFinding::Predicate(invalid.diagnostic()?));
                }
                None
            }
            Some(Ok(plan)) => {
                let mut bindings = Vec::new();
                let mut seen = std::collections::BTreeSet::new();
                for identifier in plan.identifiers() {
                    if !seen.insert(identifier) {
                        continue;
                    }
                    if let Phase::Row { group_by } = phase {
                        if identifier
                            .split_once('.')
                            .is_some_and(|(dataset, _)| catalog.joined(dataset).is_some())
                        {
                            let mut finding = LookupFinding::new(
                                plan.spec_path(),
                                if group_by.is_some() {
                                    crate::diagnostic::ConditionCode::GroupedRowNumericSourceScope
                                } else {
                                    crate::diagnostic::ConditionCode::RowNumericSourceScope
                                },
                            );
                            finding.text("identifier", identifier);
                            finding.texts("drivers", &[driver]);
                            findings.push(BindFinding::Lookup(finding));
                            continue;
                        }
                    }
                    let reference_start = findings.len();
                    let expression = self_relations::bind_operand(
                        identifier,
                        plan.spec_path(),
                        catalog,
                        driver,
                        row,
                        phase,
                        available,
                        edges,
                        findings,
                        output,
                        named,
                        secondary,
                        intermediates,
                    )?;
                    case_predicates::retain_reference_owner(
                        &mut findings[reference_start..],
                        catalog,
                    )?;
                    if let Some(expression) = expression {
                        let read = match expression {
                            Expression::Source(column) => Read::Source(column),
                            Expression::Column(column) => Read::Column(column),
                            Expression::Intermediate { index, column } => {
                                Read::Intermediate { index, column }
                            }
                            Expression::Joined(read) => Read::Joined(read),
                            _ => return Err(BindError::Internal),
                        };
                        bindings.push(Binding {
                            name: identifier.into(),
                            read,
                        });
                    }
                }
                if findings.len() == before {
                    Some(
                        BoundPredicate::new_contextual(plan.clone(), bindings, Vec::new())
                            .map_err(BindError::InvalidPredicateBinding)?,
                    )
                } else {
                    None
                }
            }
            None => None,
        };
        if let Some(expression) = bind_row(
            &branch.operation,
            &branch.path,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            reported,
            output,
            project,
            named,
            secondary,
            intermediates,
        )? {
            result.push(crate::dataset::CaseBranch {
                when,
                assignment: std::boxed::Box::new(Assignment {
                    column,
                    expression,
                    path: branch.path.clone(),
                }),
            });
        }
    }
    Ok((findings.len() == before).then_some(Expression::Case(result)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &Operation,
    path: &str,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    reported: bool,
    output: &TableSchema,
    project: Option<&crate::project_calls::ProjectCalls>,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
) -> Result<Option<Expression>, BindError> {
    match operation {
        Operation::Odm(operation) => odm::bind(operation, catalog, driver, row, phase, findings),
        Operation::Aggregate(operation) => aggregates::bind(
            operation,
            catalog,
            driver,
            row,
            phase,
            output,
            named,
            secondary,
            intermediates,
            project,
            edges,
            findings,
        ),
        Operation::Literal(value) => Ok(Some(Expression::Literal(value.clone()))),
        Operation::Source(source) => source_expressions::bind_row(
            source,
            column,
            path,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::Scalar(operation) => scalar::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::StrExtract(operation) => str_extract::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::StrReplace(operation) => str_extract::bind_row_replace(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::StrTemplate(operation) => str_template::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::MappingCut(operation) => mapping_cut::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::StrCase(operation) => str_case::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::StrPad(operation) => str_pad::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::Round(operation) => rounding::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::Temporal(operation) => temporal::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::Imputation(operation) => imputation::bind_row(
            operation,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::Flag(operation) => flags::bind_row(
            operation, catalog, driver, row, phase, available, edges, findings, reported,
        ),
        Operation::InvalidNumeric { expression, error } => {
            if !reported {
                findings.push(BindFinding::Numeric {
                    path: format!("{path}.expr"),
                    expression: expression.clone(),
                    error: error.clone(),
                });
            }
            Ok(None)
        }
        Operation::Compute(compiled) => bind_row_compute(
            compiled,
            path,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        ),
        Operation::ProjectFunction(call) => bind_row_project(
            call, catalog, driver, row, phase, available, edges, findings, project,
        ),
        Operation::Case(branches) => bind_row_case(
            branches,
            column,
            catalog,
            driver,
            row,
            phase,
            available,
            edges,
            findings,
            reported,
            output,
            project,
            named,
            secondary,
            intermediates,
        ),
        Operation::Window(window) => window.bind_available(catalog, available, edges, findings),
        Operation::FirstAvailable(selection) => bind_selection(
            selection,
            path,
            catalog,
            Some(SelectionContext {
                driver,
                row,
                phase,
                available,
            }),
            column,
            output,
            named,
            secondary,
            intermediates,
            edges,
            findings,
        ),
    }
}

/// Retain authored declaration defects before any row reference binding.
pub(super) fn declaration_findings(
    operation: &Operation,
    path: &str,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    match operation {
        Operation::Flag(operation) => operation.findings(findings),
        Operation::StrPad(operation) => str_pad::declaration_findings(operation, findings),
        Operation::Temporal(operation) => {
            for index in 0..operation.sources().len() {
                if matches!(
                    operation.literal(index),
                    Some(crate::value::Value::Float(_))
                ) && operation.expected(index) == "int"
                {
                    findings.push(BindFinding::Temporal(
                        operation.operand_diagnostic(index, "float"),
                    ));
                }
            }
        }
        Operation::InvalidNumeric { expression, error } => findings.push(BindFinding::Numeric {
            path: format!("{path}.expr"),
            expression: expression.clone(),
            error: error.clone(),
        }),
        Operation::Case(branches) => {
            for branch in branches {
                if let Some(Err(invalid)) = branch.predicate() {
                    findings.push(BindFinding::Predicate(invalid.diagnostic()?));
                }
                declaration_findings(&branch.operation, &branch.path, findings)?;
            }
        }
        _ => (),
    }
    Ok(())
}

pub(super) struct SelectionContext<'a> {
    driver: &'a str,
    row: Option<&'a str>,
    phase: Phase<'a>,
    available: Option<&'a [usize]>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind_selection(
    selection: &source_expressions::FirstAvailable,
    path: &str,
    catalog: &Catalog,
    row_context: Option<SelectionContext<'_>>,
    column: usize,
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let before = findings.len();
    let mut operands = Vec::new();
    for (index, source) in selection.sources.iter().enumerate() {
        let path = format!("{path}.sources[{index}]");
        let fallback = SelectionContext {
            driver: catalog.dataset_name(0).ok_or(BindError::Internal)?,
            row: None,
            phase: Phase::Column { groups: &[] },
            available: None,
        };
        let context = row_context.as_ref().unwrap_or(&fallback);
        let read = source_expressions::bind_row(
            source,
            column,
            &path,
            catalog,
            context.driver,
            context.row,
            context.phase,
            context.available,
            edges,
            findings,
            output,
            named,
            secondary,
            intermediates,
        )?
        .map(|expression| match expression {
            Expression::Source(column) if row_context.is_none() => {
                crate::dataset::SelectionRead::Collect {
                    column,
                    identifier: source.variable.clone(),
                    filter: None,
                }
            }
            Expression::Source(column) => crate::dataset::SelectionRead::Source(column),
            Expression::Column(column) => crate::dataset::SelectionRead::Column(column),
            expression => {
                crate::dataset::SelectionRead::Expression(std::boxed::Box::new(Assignment {
                    column,
                    expression,
                    path: path.clone(),
                }))
            }
        });
        if let Some(read) = read {
            operands.push(crate::dataset::SelectionSource { path, read });
        }
    }
    Ok(
        (findings.len() == before).then_some(Expression::FirstAvailable(std::boxed::Box::new(
            crate::dataset::FirstAvailable::new(operands, selection.missing.clone()),
        ))),
    )
}

pub(super) fn calls(operation: &Operation, result: &mut Vec<usize>) {
    match operation {
        Operation::ProjectFunction(call) => result.push(*call),
        Operation::Aggregate(operation) => operation.calls(result),
        Operation::Case(branches) => {
            for branch in branches {
                calls(&branch.operation, result);
            }
        }
        _ => (),
    }
}

/// Inspect authored dependencies before selecting per-template column defaults.
pub(super) fn references(
    d: &Document,
    value: usize,
    project: Option<&functions::Prepared>,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    references_mode(d, value, project, names, depth, true, true)
}
pub(super) fn current_references(
    d: &Document,
    value: usize,
    project: Option<&functions::Prepared>,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    references_mode(d, value, project, names, depth, false, true)
}
/// A donor window cannot read a named selector (REQ-1263). Its invalid fields
/// remain window binding findings, rather than executable dependency edges.
pub(super) fn donor_references(
    d: &Document,
    value: usize,
    project: Option<&functions::Prepared>,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    references_mode(d, value, project, names, depth, true, false)
}
fn references_mode(
    d: &Document,
    value: usize,
    project: Option<&functions::Prepared>,
    names: &mut Vec<String>,
    depth: usize,
    aggregate_relation: bool,
    window_references: bool,
) -> Result<(), PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            None,
        ));
    }
    scalar::operand_references(d, value, names, depth)?;
    if let Some(payload) = d.field(value, "function") {
        if let Some(project) = project {
            let call = project
                .calls
                .calls()
                .get(*project.nodes.get(&payload).ok_or(PrepareError::Internal)?)
                .ok_or(PrepareError::Internal)?;
            for argument in &call.located.call.arguments {
                if let crate::project_call_document::Input::Reference(name) = &argument.input {
                    names.push(name.clone());
                }
            }
        }
    }
    if let Some(payload) = d.field(value, "compute") {
        let expression = text(d, field(d, payload, "expr")?)?;
        match compile(expression, ExpressionContext::Compute, "") {
            Ok(compiled) => names.extend(compiled.identifiers().map(String::from)),
            Err(CompileError::Invalid { .. }) => (),
            Err(error) => return Err(PrepareError::expression(error, None)),
        }
    }
    if let Some(payload) = d.field(value, "aggregate") {
        let id = field(d, payload, "expr")?;
        match compile(text(d, id)?, ExpressionContext::Aggregate, "") {
            Ok(compiled) => {
                names.extend(
                    compiled
                        .identifiers()
                        .filter(|name| aggregate_relation || !name.contains('.'))
                        .map(String::from),
                );
                if aggregate_relation {
                    names.extend(
                        compiled
                            .record_relations()
                            .map(|dataset| format!("{dataset}.*")),
                    );
                }
            }
            Err(CompileError::Invalid { .. }) => (),
            Err(error) => return Err(PrepareError::expression(error, None)),
        }
        if let Some(key) = d.field(payload, "key") {
            match &d.nodes()[key] {
                N::Text(name) => names.push(name.clone()),
                N::Sequence(ids) => {
                    for &id in ids {
                        names.push(text(d, id)?.into());
                    }
                }
                N::Mapping(pairs) => {
                    for &(_, value) in pairs {
                        if let N::Text(name) = &d.nodes()[value] {
                            names.push(name.clone());
                        } else {
                            references_mode(
                                d,
                                value,
                                project,
                                names,
                                depth + 1,
                                aggregate_relation,
                                window_references,
                            )?;
                        }
                    }
                }
                N::Null => (),
                _ => return Err(PrepareError::Internal),
            }
        }
        if let Some(groups) = d
            .field(payload, "group_by")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            for &id in sequence(d, groups)? {
                let name = text(d, id)?;
                if aggregate_relation || !name.contains('.') {
                    names.push(name.into());
                }
            }
        }
        if let Some(filter) = d
            .field(payload, "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let mut filter_names = Vec::new();
            predicate_references(d, filter, &mut filter_names)?;
            names.extend(
                filter_names
                    .into_iter()
                    .filter(|name| aggregate_relation || !name.contains('.')),
            );
        }
        if let Some(between) = d
            .field(payload, "between")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            if let Some(value) = d.field(between, "value") {
                if let N::Text(name) = &d.nodes()[value] {
                    names.push(name.clone());
                }
            }
        }
    }
    if let Some(payload) = d.field(value, "flag") {
        let condition = field(d, payload, "condition")?;
        predicate_references(d, condition, names)?;
    }
    for op in ["diff", "study_day"] {
        if let Some(payload) = d.field(value, op) {
            for name in if op == "diff" {
                ["start", "end"]
            } else {
                ["date", "reference"]
            } {
                names.push(text(d, field(d, payload, name)?)?.into());
            }
        }
    }
    if let Some(payload) = d.field(value, "first_available") {
        for &item in sequence(d, field(d, payload, "sources")?)? {
            names.push(match &d.nodes()[item] {
                N::Text(name) => name.clone(),
                _ => text(d, field(d, item, "variable")?)?.into(),
            });
        }
    }
    if let Some(payload) = d.field(value, "case") {
        for &item in sequence(d, payload)? {
            if let Some(when) = d.field(item, "when") {
                predicate_references(d, when, names)?;
            }
            if let Some(child) = d.field(item, "then").or_else(|| d.field(item, "otherwise")) {
                references_mode(
                    d,
                    child,
                    project,
                    names,
                    depth + 1,
                    aggregate_relation,
                    window_references,
                )?;
            }
        }
    }
    if !window_references {
        return Ok(());
    }
    for op in ["row_number", "rank", "row_value", "locf", "baseline_flag"] {
        if let Some(payload) = d.field(value, op) {
            for field_name in ["source", "date", "reference_date", "value"] {
                if let Some(id) = d.field(payload, field_name) {
                    names.push(text(d, id)?.into());
                }
            }
            if let Some(terms) = d
                .field(payload, "tie_break")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                for &id in sequence(d, terms)? {
                    names.push(match &d.nodes()[id] {
                        N::Text(name) => name.clone(),
                        _ => text(d, field(d, id, "variable")?)?.into(),
                    });
                }
            }
            if let Some(window) = d
                .field(payload, "window")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                if let Some(group) = d
                    .field(window, "group_by")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    for &id in sequence(d, group)? {
                        names.push(text(d, id)?.into());
                    }
                }
                if let Some(order) = d
                    .field(window, "order_by")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    for &id in sequence(d, order)? {
                        names.push(match &d.nodes()[id] {
                            N::Text(name) => name.clone(),
                            _ => text(d, field(d, id, "variable")?)?.into(),
                        });
                    }
                }
                if let Some(filter) = d
                    .field(window, "filter")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    predicate_references(d, filter, names)?;
                }
            }
        }
    }
    Ok(())
}
fn predicate_references(
    d: &Document,
    id: usize,
    names: &mut Vec<String>,
) -> Result<(), PrepareError> {
    match compile(text(d, id)?, ExpressionContext::Predicate, "") {
        Ok(compiled) => names.extend(compiled.identifiers().map(String::from)),
        Err(CompileError::Invalid { .. }) => (),
        Err(error) => return Err(PrepareError::expression(error, None)),
    }
    Ok(())
}
