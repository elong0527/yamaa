//! Shared expression lowering for column, case, concatenation and row declarations.
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
        Default::default(),
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
    window_limits: CompilationLimits,
) -> Result<cases::Branch, PrepareError> {
    if depth > 64 || result_path.len() > 65_536 {
        return Err(PrepareError::Limit("expression_depth"));
    }
    let &[(op, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let op = text(d, op)?;
    let result_path = format!("{result_path}.{op}");
    let operation = match op {
        "flag" => Operation::Flag(flags::prepare(d, payload, &result_path)?),
        "literal" => Operation::Literal(literal(d, payload, &result_path)?),
        "function" => Operation::ProjectFunction(
            *project
                .and_then(|p| p.nodes.get(&payload))
                .ok_or_else(|| unsupported(&result_path, "function_environment"))?,
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
        "str_contains" => Operation::StrContains(extraction.get(payload)?),
        "greatest" | "least" | "str_concat" => {
            Operation::Scalar(alloc::boxed::Box::new(scalar::prepare(
                d,
                payload,
                op,
                &result_path,
                project,
                intermediates,
                record,
                depth + 1,
                extraction,
                templates,
            )?))
        }
        "mapping" | "cut" => {
            let mut features = Vec::new();
            let declaration =
                mapping_cut::prepare(d, payload, op, &result_path, intermediates, &mut features)?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            if record && declaration.source.has_filter() {
                return Err(unsupported(&result_path, "row_selection_filter"));
            }
            Operation::MappingCut(alloc::boxed::Box::new(declaration))
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
        "date_impute" | "date_precision" | "datetime_impute" | "datetime_precision" | "to_date" => {
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
        "date_diff" | "study_day" | "to_epoch_day" => {
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
            if operation
                .sources
                .iter()
                .any(source_expressions::Declaration::has_filter)
            {
                return Err(unsupported(&result_path, "row_selection_filter"));
            }
            Operation::FirstAvailable(alloc::boxed::Box::new(operation))
        }
        "row_number" | "rank" | "row_value" | "previous_non_missing" | "locf" | "baseline_flag" => {
            if depth != 0 {
                return Err(unsupported(&result_path, "nested_window"));
            }
            let mut features = Vec::new();
            let operation = windows::Declaration::prepare(
                d,
                payload,
                op,
                &result_path,
                window_limits,
                &mut features,
            )?;
            if !features.is_empty() {
                return Err(PrepareError::Unsupported(features));
            }
            Operation::Window(alloc::boxed::Box::new(operation))
        }
        "compute" => {
            for &(name, _) in mapping(d, payload)? {
                let name = text(d, name)?;
                if name != "expr" {
                    return Err(unsupported(&format!("{result_path}.{name}"), name));
                }
            }
            let expression = text(d, field(d, payload, "expr")?)?;
            match compile_numeric_with_policy(
                expression,
                &result_path,
                Default::default(),
                crate::numeric_compiler::MathPolicy::PortableLibmV1,
            ) {
                Ok(compiled) => Operation::Compute(compiled),
                Err(error @ CompileError::Parse(ParseError::Grammar { .. })) => {
                    Operation::InvalidNumeric {
                        expression: expression.into(),
                        error,
                    }
                }
                Err(CompileError::Unsupported { .. }) => {
                    return Err(unsupported(&result_path, "numeric_function"))
                }
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
            if source.has_filter() {
                return Err(unsupported(&result_path, "case_source_filter"));
            }
            if source.has_selection() || source.has_absent() {
                return Err(unsupported(&result_path, "case_source_options"));
            }
            Operation::Source(alloc::boxed::Box::new(source))
        }
        _ => return Err(unsupported(&result_path, op)),
    };
    Ok(cases::Branch::value(operation, result_path))
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
        Operation::Literal(value) => Ok(Some(Expression::Literal(value.clone()))),
        Operation::Source(source) => {
            if named.reference(&source.variable).is_some() {
                row_intermediates::bind(
                    &source.variable,
                    path,
                    named,
                    secondary,
                    intermediates,
                    output,
                    catalog,
                    available,
                    edges,
                    findings,
                )
            } else {
                rounding::bind_row_operand(
                    &source.variable,
                    path,
                    catalog,
                    driver,
                    row,
                    phase,
                    available,
                    edges,
                    findings,
                )
            }
        }
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
            reported,
            output,
            project,
            named,
            secondary,
            intermediates,
        ),
        Operation::StrContains(operation) => scalar::bind_row_contains(
            operation, column, catalog, driver, row, phase, available, edges, findings,
        ),
        Operation::StrExtract(operation) => str_extract::bind_row(
            operation, column, catalog, driver, row, phase, available, edges, findings,
        ),
        Operation::StrTemplate(operation) => str_template::bind_row(
            operation, column, catalog, driver, row, phase, available, edges, findings,
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
            operation, column, catalog, driver, row, phase, available, edges, findings,
        ),
        Operation::StrPad(operation) => str_pad::bind_row(
            operation, column, catalog, driver, row, phase, available, edges, findings,
        ),
        Operation::Round(operation) => rounding::bind_row(
            operation, column, catalog, driver, row, phase, available, edges, findings,
        ),
        Operation::Temporal(operation) => temporal::bind_row(
            operation, column, catalog, driver, row, phase, available, edges, findings,
        ),
        Operation::Imputation(operation) => imputation::bind_row(
            operation, column, catalog, driver, row, phase, available, edges, findings,
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
        Operation::Compute(compiled) => {
            let before = findings.len();
            let mut bindings = Vec::new();
            for identifier in compiled.identifiers() {
                if let Some(expression) = rounding::bind_row_operand(
                    identifier,
                    &format!("{path}.expr"),
                    catalog,
                    driver,
                    row,
                    phase,
                    available,
                    edges,
                    findings,
                )? {
                    let read = match expression {
                        Expression::Source(column) => Read::Source(column),
                        Expression::Column(column) => Read::Column(column),
                        _ => return Err(BindError::Internal),
                    };
                    bindings.push(Binding {
                        name: identifier.into(),
                        read,
                    });
                }
            }
            (findings.len() == before)
                .then(|| BoundNumeric::new(compiled.clone(), bindings).map(Expression::Compute))
                .transpose()
                .map_err(BindError::InvalidNumericBinding)
        }
        Operation::ProjectFunction(call) => {
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
                Err(crate::project_call_binding::Error::Reference(error)) => {
                    Err(BindError::Catalog(error))
                }
                Err(_) => Err(BindError::Internal),
            }
        }
        Operation::Case(branches) => {
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
                        let mut seen = alloc::collections::BTreeSet::new();
                        for identifier in plan.identifiers() {
                            if !seen.insert(identifier) {
                                continue;
                            }
                            if let Some(expression) = rounding::bind_row_operand(
                                identifier,
                                plan.spec_path(),
                                catalog,
                                driver,
                                row,
                                phase,
                                available,
                                edges,
                                findings,
                            )? {
                                let read = match expression {
                                    Expression::Source(column) => Read::Source(column),
                                    Expression::Column(column) => Read::Column(column),
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
                                BoundPredicate::new(plan.clone(), bindings)
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
                        assignment: alloc::boxed::Box::new(Assignment {
                            column,
                            expression,
                            path: branch.path.clone(),
                        }),
                    });
                }
            }
            Ok((findings.len() == before).then_some(Expression::Case(result)))
        }
        Operation::Window(window) => window
            .bind_available(catalog, available, edges, findings)
            .map(|window| window.map(Expression::Window)),
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
        Operation::Scalar(operation) => {
            if let scalar::Declaration::Concat { sources, .. } = operation.as_ref() {
                for branch in sources {
                    declaration_findings(&branch.operation, &branch.path, findings)?;
                }
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

pub(super) fn bind_selection(
    selection: &source_expressions::FirstAvailable,
    path: &str,
    catalog: &Catalog,
    row_context: Option<SelectionContext<'_>>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let before = findings.len();
    let mut operands = Vec::new();
    for (index, source) in selection.sources.iter().enumerate() {
        let path = format!("{path}.sources[{index}]");
        let read = if let Some(SelectionContext {
            driver,
            row,
            phase,
            available,
        }) = &row_context
        {
            rounding::bind_row_operand(
                &source.variable,
                &path,
                catalog,
                driver,
                *row,
                *phase,
                *available,
                edges,
                findings,
            )?
            .map(|expression| match expression {
                Expression::Source(column) => crate::dataset::SelectionRead::Source(column),
                Expression::Column(column) => crate::dataset::SelectionRead::Column(column),
                _ => unreachable!(),
            })
        } else {
            match catalog.bind(&source.variable).map_err(BindError::Catalog)? {
                Some(reference_binding::Binding::Output { column, .. }) => {
                    edges.push(column);
                    Some(crate::dataset::SelectionRead::Column(column))
                }
                Some(reference_binding::Binding::Dataset { field, .. }) => {
                    Some(crate::dataset::SelectionRead::Collect {
                        column: field,
                        identifier: source.variable.clone(),
                        filter: None,
                    })
                }
                None => {
                    findings.push(BindFinding::UnknownReference {
                        path: path.clone(),
                        name: source.variable.clone(),
                    });
                    None
                }
            }
        };
        if let Some(read) = read {
            operands.push(crate::dataset::SelectionSource { path, read });
        }
    }
    Ok(
        (findings.len() == before).then_some(Expression::FirstAvailable(alloc::boxed::Box::new(
            crate::dataset::FirstAvailable::new(operands, selection.missing.clone()),
        ))),
    )
}

pub(super) fn calls(operation: &Operation, result: &mut Vec<usize>) {
    match operation {
        Operation::ProjectFunction(call) => result.push(*call),
        Operation::Case(branches) => {
            for branch in branches {
                calls(&branch.operation, result);
            }
        }
        Operation::Scalar(operation) => {
            if let scalar::Declaration::Concat { sources, .. } = operation.as_ref() {
                for branch in sources {
                    calls(&branch.operation, result);
                }
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
    if depth > 64 {
        return Err(PrepareError::Limit("expression_depth"));
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
        match crate::numeric_parser::parse_numeric(
            text(d, field(d, payload, "expr")?)?,
            Default::default(),
        ) {
            Ok(parsed) => names.extend(parsed.identifiers().map(String::from)),
            Err(crate::numeric_parser::ParseError::Grammar { .. }) => (),
            Err(_) => return Err(PrepareError::Limit("numeric_parser")),
        }
    }
    if let Some(payload) = d.field(value, "flag") {
        let condition = if matches!(d.nodes()[payload], N::Text(_)) {
            payload
        } else {
            field(d, payload, "condition")?
        };
        predicate_references(d, condition, names)?;
    }
    for op in ["date_diff", "study_day"] {
        if let Some(payload) = d.field(value, op) {
            for name in if op == "date_diff" {
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
                references(d, child, project, names, depth + 1)?;
            }
        }
    }
    if let Some(payload) = d.field(value, "str_concat") {
        for &child in sequence(d, field(d, payload, "sources")?)? {
            references(d, child, project, names, depth + 1)?;
        }
    }
    for op in [
        "row_number",
        "rank",
        "row_value",
        "previous_non_missing",
        "locf",
        "baseline_flag",
    ] {
        if let Some(payload) = d.field(value, op) {
            for field_name in ["source", "date", "reference_date"] {
                if let Some(id) = d.field(payload, field_name) {
                    names.push(text(d, id)?.into());
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
    match crate::predicate_parser::parse_predicate(text(d, id)?, Default::default()) {
        Ok(parsed) => names.extend(parsed.identifiers().map(String::from)),
        Err(crate::predicate_parser::ParseError::Grammar { .. }) => (),
        Err(_) => return Err(PrepareError::Limit("predicate_parser")),
    }
    Ok(())
}
