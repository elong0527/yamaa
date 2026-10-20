//! Shared expression lowering for column, case and row declarations.
use super::*;
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
            value,
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
        "function" => {
            let project = project.ok_or(PrepareError::Internal)?;
            // The tree types the call by the return its definition declares.
            project
                .returns
                .get(&payload)
                .ok_or(PrepareError::Internal)?;
            Operation::ProjectFunction {
                call: *project.nodes.get(&payload).ok_or(PrepareError::Internal)?,
            }
        }
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
        "diff" | "study_day" | "to_epoch_day" | "shift" | "duration_value" | "dtc"
        | "time_of_day" => {
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
    Ok(cases::Branch::value(operation, result_path, value))
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
/// A donor window cannot read a named selector (LOOKUP-16). Its invalid fields
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
