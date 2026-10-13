//! Closed row lowering with per-template defaults and shared group-scope validation.
use super::*;
use crate::dataset::Verification;
use crate::{
    bound_expression::{Binding, BoundPredicate, Read},
    predicate_compiler,
    reference_scope::{self, Phase, Reach, Scope},
};

#[derive(Debug)]
enum RowOperation {
    Shared(cases::Branch),
    ProjectFunction(usize),
    Output(String),
    Source(String),
}
impl RowOperation {
    fn calls(&self) -> Vec<usize> {
        let mut calls = Vec::new();
        match self {
            Self::ProjectFunction(call) => calls.push(*call),
            Self::Shared(branch) => derivations::calls(&branch.operation, &mut calls),
            _ => (),
        }
        calls
    }
}
#[derive(Debug)]
struct RowDeclaration {
    handler: Option<crate::conversion::LiteralHandler>,
    column: usize,
    path: String,
    operation: RowOperation,
}
#[derive(Debug)]
struct Template {
    id: String,
    driver: String,
    groups: Option<Vec<String>>,
    declarations: Vec<RowDeclaration>,
    filter: Option<(
        String,
        Result<crate::predicate::Plan, predicate_compiler::Error>,
    )>,
}
fn deferred_odm_contexts(
    declarations: &[Template],
    templates: &[RowTemplate],
) -> Result<alloc::rc::Rc<[crate::dataset::OdmTemplateScope]>, BindError> {
    declarations
        .iter()
        .try_fold(0usize, |used, template| {
            used.checked_add(template.id.len()).and_then(|used| {
                used.checked_add(
                    core::mem::size_of::<crate::dataset::OdmTemplateScope>()
                        + 8 * core::mem::size_of::<usize>(),
                )
            })
        })
        .filter(|&bytes| bytes <= 16_777_216)
        .ok_or(BindError::Internal)?;
    Ok(declarations
        .iter()
        .zip(templates)
        .map(|(declaration, template)| crate::dataset::OdmTemplateScope {
            row: declaration.id.clone(),
            driver: template.mode.source_index(),
            groups: match template.mode.local_mode() {
                RowMode::Groups(groups) => Some(
                    crate::odm_source::HIERARCHY
                        .into_iter()
                        .filter(|field| groups.contains(field))
                        .collect(),
                ),
                _ => None,
            },
        })
        .collect::<Vec<_>>()
        .into())
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
#[allow(clippy::too_many_arguments)]
fn declaration(
    d: &Document,
    id: usize,
    column: usize,
    prefix: String,
    _grouped: bool,
    limits: CompilationLimits,
    project: Option<&functions::Prepared>,
    extraction: &str_extract::Prepared,
    string_templates: &str_template::Prepared,
    intermediates: &intermediates::Declarations,
) -> Result<RowDeclaration, PrepareError> {
    closed_fields(d, id, &["value", "unconvertible"], &prefix)?;
    let handler = literal_handler(d, id, &prefix)?;
    let &[(op, payload)] = mapping(d, field(d, id, "value")?)? else {
        return Err(PrepareError::Internal);
    };
    let op = text(d, op)?;
    let path = format!("{prefix}.{op}");
    let operation = match op {
        "function" if project.is_some() => RowOperation::ProjectFunction(
            *project
                .unwrap()
                .nodes
                .get(&payload)
                .ok_or(PrepareError::Internal)?,
        ),
        "function" if project.is_none() => return Err(unsupported("function", &path)),
        "source" if mapping(d, payload)?.len() == 1 => {
            closed_fields(d, payload, &["variable"], &path)?;
            let name = text(d, field(d, payload, "variable")?)?;
            if name.contains('.') {
                RowOperation::Source(name.into())
            } else {
                RowOperation::Output(name.into())
            }
        }
        _ => RowOperation::Shared(derivations::prepare_value_with_driver(
            d,
            field(d, id, "value")?,
            &prefix,
            project,
            intermediates,
            true,
            0,
            extraction,
            string_templates,
            None,
            limits,
        )?),
    };
    Ok(RowDeclaration {
        handler,
        column,
        path,
        operation,
    })
}
/// Inspect references in supported row-local defaults without reading source data.
/// Literal text and qualified stored fields never promote an output declaration.
fn default_reads(
    d: &Document,
    declaration: usize,
    output: &TableSchema,
    project: Option<&functions::Prepared>,
    string_templates: &str_template::Prepared,
) -> Result<(bool, Vec<usize>), PrepareError> {
    let value = field(d, declaration, "value")?;
    let &[(operation, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let operation = text(d, operation)?;
    fn contains_window(d: &Document, value: usize, depth: usize) -> Result<bool, PrepareError> {
        if depth > 64 {
            return Err(PrepareError::Limit("expression_depth"));
        }
        if [
            "row_number",
            "rank",
            "row_value",
            "previous_non_missing",
            "locf",
            "baseline_flag",
            "window",
        ]
        .iter()
        .any(|operation| d.field(value, operation).is_some())
        {
            return Ok(true);
        }
        if let Some(branches) = d.field(value, "case") {
            for &branch in sequence(d, branches)? {
                for name in ["then", "otherwise"] {
                    if let Some(child) = d.field(branch, name) {
                        if contains_window(d, child, depth + 1)? {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        if let Some(concat) = d.field(value, "str_concat") {
            for &child in sequence(d, field(d, concat, "sources")?)? {
                if contains_window(d, child, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    // A column window spans the complete constructed dataset, including when
    // a nested branch or another output-key default depends on it.
    let window_default = contains_window(d, value, 0)?;
    let mut reads = Vec::new();
    let mut names = Vec::new();
    derivations::references(d, value, project, &mut names, 0)?;
    self_relations::template_references(d, value, string_templates, &mut names, 0)?;
    for name in names {
        if self_relations::is_reference(d, &name)? {
            return Ok((false, reads));
        }
    }
    let mut reference = |name: &str| {
        if !name.contains('.') {
            if let Some(column) = output.columns().iter().position(|c| c.name == name) {
                reads.push(column);
            }
        }
    };
    match operation {
        "literal" => {}
        "greatest"
        | "least"
        | "str_concat"
        | "str_contains"
        | "case"
        | "first_available"
        | "mapping"
        | "cut"
        | "row_number"
        | "rank"
        | "row_value"
        | "previous_non_missing"
        | "locf"
        | "baseline_flag" => {
            let mut names = Vec::new();
            derivations::references(d, value, project, &mut names, 0)?;
            for name in names {
                reference(&name);
            }
        }
        "source" => {
            let name = text(d, field(d, payload, "variable")?)?;
            reference(name);
            reads.extend(row_intermediates::default_dependencies(d, name, output)?);
        }
        "str_template" => {
            for name in string_templates.get(payload)?.identifiers() {
                reference(name);
            }
        }
        "round_half_away_from_zero" => reference(text(d, field(d, payload, "source")?)?),
        "date_diff" | "study_day" | "to_epoch_day" => {
            let fields: &[&str] = match operation {
                "date_diff" => &["start", "end"],
                "study_day" => &["date", "reference"],
                _ => &["source"],
            };
            for &name in fields {
                reference(text(d, field(d, payload, name)?)?);
            }
        }
        "function" => {
            if let Some(project) = project {
                let call = project
                    .calls
                    .calls()
                    .get(*project.nodes.get(&payload).ok_or(PrepareError::Internal)?)
                    .ok_or(PrepareError::Internal)?;
                for argument in &call.located.call.arguments {
                    if let crate::project_call_document::Input::Reference(name) = &argument.input {
                        reference(name);
                    }
                }
            }
        }
        "flag" => {
            let condition = if matches!(d.nodes()[payload], N::Text(_)) {
                payload
            } else {
                field(d, payload, "condition")?
            };
            match crate::predicate_parser::parse_predicate(text(d, condition)?, Default::default())
            {
                Ok(parsed) => {
                    for name in parsed.identifiers() {
                        reference(name);
                    }
                }
                Err(crate::predicate_parser::ParseError::Grammar { .. }) => (),
                Err(_) => return Err(PrepareError::Limit("predicate_parser")),
            }
        }
        "compute" => {
            let expression = text(d, field(d, payload, "expr")?)?;
            match crate::numeric_parser::parse_numeric(expression, Default::default()) {
                Ok(parsed) => {
                    for name in parsed.identifiers() {
                        reference(name);
                    }
                }
                Err(crate::numeric_parser::ParseError::Grammar { .. }) => {}
                Err(_) => return Err(PrepareError::Limit("numeric_parser")),
            }
        }
        "aggregate" | "lookup" | "window" => return Ok((false, reads)),
        _ => return Ok((true, reads)),
    }
    Ok((!window_default, reads))
}

fn select_defaults(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    rows: &[usize],
    filter_defaults: alloc::collections::BTreeSet<usize>,
    project: Option<&functions::Prepared>,
    string_templates: &str_template::Prepared,
) -> Result<Vec<usize>, PrepareError> {
    let defaults = columns
        .iter()
        .map(|&column| {
            d.field(column, "derivation")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| default_reads(d, id, output, project, string_templates))
                .transpose()
                .map(|default| default.unwrap_or((true, Vec::new())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let edges = defaults.iter().try_fold(0usize, |n, (_, edges)| {
        n.checked_add(edges.len())
            .ok_or(PrepareError::Limit("row_dependencies"))
    })?;
    if columns
        .len()
        .checked_mul(edges.max(1))
        .is_none_or(|work| work > 67_108_864)
    {
        return Err(PrepareError::Limit("row_dependencies"));
    }
    let mut local = defaults.iter().map(|(local, _)| *local).collect::<Vec<_>>();
    loop {
        let mut changed = false;
        for (column, (_, reads)) in defaults.iter().enumerate() {
            if local[column] && reads.iter().any(|&read| !local[read]) {
                local[column] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut selected = alloc::vec![false; columns.len()];
    // A column ODM default owns the enclosing template's item scope and row
    // identity even when no row overrides or filter reference it.
    for (column, &node) in columns.iter().enumerate() {
        if let Some(derivation) = d
            .field(node, "derivation")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            if odm::contains(d, derivation, 0)? {
                selected[column] = local[column];
            }
        }
    }
    for column in filter_defaults {
        selected[column] = local[column];
    }
    let mut overridden: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    for &row in rows {
        for &(name, declaration) in mapping(d, field(d, row, "derivations")?)? {
            let name = text(d, name)?;
            let column = output
                .columns()
                .iter()
                .position(|c| c.name == name)
                .ok_or(PrepareError::Internal)?;
            if local[column] {
                selected[column] = true;
            } else {
                overridden
                    .entry(column)
                    .or_default()
                    .push(text(d, field(d, row, "id")?)?);
            }
            let (_, reads) = default_reads(d, declaration, output, project, string_templates)?;
            for read in reads {
                if local[read] {
                    selected[read] = true;
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for (column, (_, reads)) in defaults.iter().enumerate() {
            if selected[column] {
                for &read in reads {
                    if local[read] && !selected[read] {
                        selected[read] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Charge the complete retained and projected context before the first copy.
    // Grouping preserves authored row order and emits columns in declaration order.
    let mut context_bytes = 0usize;
    for (&column, rows) in &overridden {
        let bytes = rows
            .iter()
            .try_fold(output.columns()[column].name.len(), |n, row| {
                n.checked_add(row.len())
                    .ok_or(PrepareError::Limit("row_default_context"))
            })?;
        context_bytes = bytes
            .checked_mul(4)
            .and_then(|n| context_bytes.checked_add(n))
            .filter(|&n| n <= 16_777_216)
            .ok_or(PrepareError::Limit("row_default_context"))?;
    }
    let findings = overridden
        .into_iter()
        .map(|(column, rows)| PreflightFinding::DuplicateRowDefault {
            column: output.columns()[column].name.clone(),
            rows: rows.into_iter().map(String::from).collect(),
        })
        .collect::<Vec<_>>();
    if !findings.is_empty() {
        return Err(PrepareError::Invalid(findings));
    }
    Ok(selected
        .into_iter()
        .enumerate()
        .filter_map(|(column, selected)| selected.then_some(column))
        .collect())
}

/// Bound every retained row-phase finding before copying any authored context.
fn validate_row_phases(
    templates: &[Template],
    project: Option<&functions::Prepared>,
    output: &TableSchema,
) -> Result<(), PrepareError> {
    struct Site<'a> {
        template: &'a Template,
        declaration: &'a RowDeclaration,
        identifier: &'a str,
        function: Option<(
            &'a crate::project_calls::LocatedCall,
            &'a crate::project_call_document::Argument,
        )>,
    }
    fn register<'a>(
        site: Site<'a>,
        output: &TableSchema,
        sites: &mut Vec<Site<'a>>,
        bytes: &mut usize,
    ) -> Result<(), PrepareError> {
        if site.identifier.contains('.') {
            return Ok(());
        }
        let Some(column) = output
            .columns()
            .iter()
            .position(|c| c.name == site.identifier)
        else {
            // Unknown bare sources retain their ordinary binding diagnostic.
            return Ok(());
        };
        if site
            .template
            .declarations
            .iter()
            .any(|d| d.column == column)
        {
            return Ok(());
        }
        let mut cost = site
            .declaration
            .path
            .len()
            .checked_add(site.identifier.len())
            .and_then(|n| n.checked_add(site.template.id.len()))
            .and_then(|n| n.checked_add(128))
            .ok_or(PrepareError::Limit("function_findings"))?;
        if let Some((call, argument)) = site.function {
            cost = cost
                .checked_add(call.call.name.len())
                .and_then(|n| n.checked_add(argument.name.len().checked_mul(2)?))
                .ok_or(PrepareError::Limit("function_findings"))?;
        }
        let cost = cost
            .checked_mul(4)
            .ok_or(PrepareError::Limit("function_findings"))?;
        *bytes = bytes
            .checked_add(cost)
            .filter(|&n| n <= 16_777_216)
            .ok_or(PrepareError::Limit("function_findings"))?;
        if sites.len() >= 65_536 {
            return Err(PrepareError::Limit("function_findings"));
        }
        sites.push(site);
        Ok(())
    }
    let mut sites = Vec::new();
    let mut bytes = 0usize;
    for template in templates {
        for declaration in &template.declarations {
            match &declaration.operation {
                RowOperation::Output(identifier) => register(
                    Site {
                        template,
                        declaration,
                        identifier,
                        function: None,
                    },
                    output,
                    &mut sites,
                    &mut bytes,
                )?,
                RowOperation::ProjectFunction(call) => {
                    let call = &project
                        .ok_or(PrepareError::Internal)?
                        .calls
                        .calls()
                        .get(*call)
                        .ok_or(PrepareError::Internal)?
                        .located;
                    for argument in &call.call.arguments {
                        if let crate::project_call_document::Input::Reference(identifier) =
                            &argument.input
                        {
                            register(
                                Site {
                                    template,
                                    declaration,
                                    identifier,
                                    function: Some((call, argument)),
                                },
                                output,
                                &mut sites,
                                &mut bytes,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if sites.is_empty() {
        return Ok(());
    }
    Err(PrepareError::Invalid(
        sites
            .into_iter()
            .map(|site| match site.function {
                Some((call, argument)) => PreflightFinding::ProjectFunction(FunctionFinding {
                    path: format!("{}.args.{}", call.path, argument.name),
                    function: call.call.name.clone(),
                    argument: Some(argument.name.clone()),
                    node: argument.node,
                    cause: functions::Cause::RowPhase {
                        identifier: site.identifier.into(),
                        row: site.template.id.clone(),
                    },
                }),
                None => PreflightFinding::RowPhase {
                    path: site.declaration.path.clone(),
                    identifier: site.identifier.into(),
                    row: site.template.id.clone(),
                },
            })
            .collect(),
    ))
}
impl Rows {
    pub(super) fn self_columns(&self) -> Vec<usize> {
        self.templates[0]
            .declarations
            .iter()
            .map(|declaration| declaration.column)
            .filter(|&column| {
                self.templates.iter().all(|template| {
                    template
                        .declarations
                        .iter()
                        .any(|declaration| declaration.column == column)
                })
            })
            .collect()
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        d: &Document,
        output: &TableSchema,
        driver: &str,
        limits: CompilationLimits,
        project: Option<&functions::Prepared>,
        _column_terminology: bool,
        extraction: &str_extract::Prepared,
        string_templates: &str_template::Prepared,
        intermediates: &intermediates::Declarations,
    ) -> Result<Self, PrepareError> {
        let root = d.root();
        let raw_rows = sequence(d, field(d, root, "rows")?)?;
        if raw_rows.is_empty() || raw_rows.len() > 16 {
            return Err(PrepareError::Limit("row_templates"));
        }
        row_intermediates::admit(d, raw_rows.len())?;
        let columns = sequence(d, field(d, root, "columns")?)?;
        let filters = raw_rows
            .iter()
            .enumerate()
            .map(|(index, &row)| {
                d.field(row, "filter")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                    .map(|id| {
                        let expression = text(d, id)?;
                        let path = format!("rows[{index}].filter");
                        let compiled =
                            predicate_compiler::compile(expression, &path, Default::default());
                        match &compiled {
                            Ok(_)
                            | Err(predicate_compiler::Error::Parse(
                                crate::predicate_parser::ParseError::Grammar { .. },
                            )) => {}
                            Err(predicate_compiler::Error::Parse(
                                crate::predicate_parser::ParseError::UnsupportedRegex { .. },
                            )) => return Err(unsupported("predicate_regex", &path)),
                            Err(predicate_compiler::Error::Internal) => {
                                return Err(PrepareError::Internal)
                            }
                            Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
                        }
                        Ok((String::from(expression), compiled))
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, PrepareError>>()?;
        let mut filter_defaults =
            self_relations::donor_defaults(d, output, project, intermediates, string_templates)?;
        for filter in &filters {
            if let Some((_, Ok(plan))) = filter {
                for name in plan
                    .identifiers()
                    .into_iter()
                    .filter(|name| !name.contains('.'))
                {
                    let Some(column) = output.columns().iter().position(|c| c.name == name) else {
                        continue;
                    };
                    if d.field(columns[column], "derivation")
                        .is_some_and(|id| !matches!(d.nodes()[id], N::Null))
                    {
                        filter_defaults.insert(column);
                    }
                }
            }
        }
        // Complete row-local identity defaults before a newly admitted selector can fail.
        let mut secondary_reads = false;
        for (row_index, &row) in raw_rows.iter().enumerate() {
            let row_driver = d
                .field(row, "dataset")
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or(driver);
            secondary_reads |= filters[row_index].as_ref().is_some_and(|(_, compiled)| {
                compiled.as_ref().is_ok_and(|plan| {
                    plan.identifiers().into_iter().any(|read| {
                        read.split_once('.').is_some_and(|(source, _)| {
                            source != row_driver
                                && d.field(
                                    field(d, root, "input").expect("admitted sources"),
                                    source,
                                )
                                .is_some()
                        })
                    })
                })
            });
            for &(_, declaration) in mapping(d, field(d, row, "derivations")?)? {
                let mut reads = Vec::new();
                derivations::current_references(
                    d,
                    field(d, declaration, "value")?,
                    project,
                    &mut reads,
                    0,
                )?;
                secondary_reads |= reads.iter().any(|read| {
                    read.split_once('.').is_some_and(|(source, _)| {
                        source != row_driver
                            && d.field(field(d, root, "input").expect("admitted sources"), source)
                                .is_some()
                    })
                });
            }
        }
        if present(d, root, "intermediates") || secondary_reads {
            for &key in sequence(d, field(d, root, "keys")?)? {
                if let Some(column) = output
                    .columns()
                    .iter()
                    .position(|column| text(d, key).is_ok_and(|key| key == column.name))
                {
                    let window_default = d
                        .field(columns[column], "derivation")
                        .filter(|&id| !matches!(d.nodes()[id], N::Null))
                        .map(|id| {
                            let value = field(d, id, "value")?;
                            let &[(operation, _)] = mapping(d, value)? else {
                                return Err(PrepareError::Internal);
                            };
                            Ok(matches!(
                                text(d, operation)?,
                                "row_number"
                                    | "rank"
                                    | "row_value"
                                    | "previous_non_missing"
                                    | "locf"
                                    | "baseline_flag"
                                    | "window"
                            ))
                        })
                        .transpose()?
                        .unwrap_or(false);
                    // An output-key window default spans the complete constructed
                    // dataset. Selection identity cannot make it template-local.
                    if !window_default {
                        filter_defaults.insert(column);
                    }
                }
            }
        }
        let mut templates = Vec::new();
        let selected = select_defaults(
            d,
            columns,
            output,
            raw_rows,
            filter_defaults,
            project,
            string_templates,
        )?;
        for &column in &selected {
            let metadata = &output.columns()[column];
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
                    limits,
                    project,
                    extraction,
                    string_templates,
                    intermediates,
                )?;
            }
        }
        for (index, (&row, filter)) in raw_rows.iter().zip(filters).enumerate() {
            let path = format!("rows[{index}]");
            closed_fields(
                d,
                row,
                &[
                    "id",
                    "dataset",
                    "group_by",
                    "derivations",
                    "filter",
                    "submission",
                ],
                &path,
            )?;
            let row_driver = d
                .field(row, "dataset")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or(driver);
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
                    // Authored preflight already owns the undeclared-column
                    // diagnostic; this is only a normalized-plan invariant.
                    return Err(PrepareError::Internal);
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
                row_drivers::admit_reads(
                    d,
                    id,
                    Some(row_driver),
                    &prefix,
                    project,
                    string_templates,
                    intermediates,
                )?;
                declarations.push(declaration(
                    d,
                    id,
                    column,
                    prefix,
                    groups.is_some(),
                    limits,
                    project,
                    extraction,
                    string_templates,
                    intermediates,
                )?);
            }
            templates.push(Template {
                id: text(d, field(d, row, "id")?)?.into(),
                driver: row_driver.into(),
                groups,
                declarations,
                filter,
            });
        }
        let mut self_admission =
            self_relations::FirstTemplate::new(d, project, intermediates, string_templates);
        for declaration in &templates[0].declarations {
            // The expanded occurrence retains its authored row or default path.
            let column = &output.columns()[declaration.column].name;
            let raw = mapping(d, field(d, raw_rows[0], "derivations")?)?
                .iter()
                .find(|&&(name, _)| text(d, name).is_ok_and(|name| name == column))
                .map(|&(_, id)| id)
                .or_else(|| d.field(columns[declaration.column], "derivation"));
            if let Some(raw) = raw {
                let prefix = declaration
                    .path
                    .rsplit_once('.')
                    .map_or(declaration.path.as_str(), |(prefix, _)| prefix);
                self_admission.visit(field(d, raw, "value")?, prefix, 0)?;
            }
        }
        let self_findings = self_admission.finish();
        if !self_findings.is_empty() {
            return Err(PrepareError::Invalid(self_findings));
        }
        validate_row_phases(&templates, project, output)?;
        let mut lowered_columns = Vec::new();
        for (column, &id) in columns.iter().enumerate() {
            let prefix = format!("columns.{}", output.columns()[column].name);
            if !selected.contains(&column) {
                let Some(derivation) = d
                    .field(id, "derivation")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                else {
                    // Authored preflight already retains missing defaults and
                    // row derivations before lowering any template.
                    return Err(PrepareError::Internal);
                };
                if templates.iter().any(|template| template.driver != driver) {
                    row_drivers::admit_reads(
                        d,
                        derivation,
                        None,
                        &format!("{prefix}.derivation"),
                        project,
                        string_templates,
                        intermediates,
                    )?;
                }
                lowered_columns.push(declaration(
                    d,
                    derivation,
                    column,
                    format!("{prefix}.derivation"),
                    false,
                    limits,
                    project,
                    extraction,
                    string_templates,
                    intermediates,
                )?);
            }
        }
        Ok(Self {
            templates,
            columns: lowered_columns,
        })
    }
    pub(super) fn call_uses(
        &self,
        calls: &crate::project_calls::ProjectCalls,
    ) -> Result<Vec<usize>, PrepareError> {
        let mut uses = alloc::vec![0usize; calls.calls().len()];
        // A row label can appear in each argument finding and its projected context.
        // Admit the cumulative copies before source binding can construct findings.
        let mut row_context = 0usize;
        for template in &self.templates {
            for declaration in &template.declarations {
                for call in declaration.operation.calls() {
                    let arguments = calls
                        .calls()
                        .get(call)
                        .ok_or(PrepareError::Internal)?
                        .located
                        .call
                        .arguments
                        .len();
                    let cost = template
                        .id
                        .len()
                        .checked_mul(arguments)
                        .and_then(|n| n.checked_mul(4))
                        .ok_or(PrepareError::Limit("function_row_context"))?;
                    row_context = row_context
                        .checked_add(cost)
                        .filter(|&n| n <= 16_777_216)
                        .ok_or(PrepareError::Limit("function_row_context"))?;
                }
            }
        }
        for declaration in self
            .templates
            .iter()
            .flat_map(|t| &t.declarations)
            .chain(&self.columns)
        {
            for call in declaration.operation.calls() {
                let count = uses.get_mut(call).ok_or(PrepareError::Internal)?;
                *count = count
                    .checked_add(1)
                    .ok_or(PrepareError::Limit("function_calls"))?;
            }
        }
        Ok(uses)
    }
    pub(super) fn admit_sources(
        &self,
        schemas: &[&TableSchema],
        output: &TableSchema,
    ) -> Result<(), BindError> {
        row_intermediates::admit_sources(
            schemas,
            output,
            self.templates.len(),
            self.templates
                .iter()
                .flat_map(|t| &t.declarations)
                .chain(&self.columns)
                .map(|d| d.path.len()),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind(
        &self,
        _prepared: &PreparedSpecification,
        source: &TableSchema,
        name: &str,
        output: &TableSchema,
        keys: &[usize],
        _verifications: &[Verification],
        project: Option<&crate::project_calls::ProjectCalls>,
        named: &intermediates::Declarations,
        secondary: Vec<crate::dataset::SecondarySource>,
        intermediates: Vec<Option<crate::dataset::Intermediate>>,
        mut findings: Vec<BindFinding>,
    ) -> Result<DatasetPlan, BindError> {
        let mut catalog =
            row_drivers::joined_catalog(output, source, name, keys, &secondary, name)?;
        if self
            .templates
            .iter()
            .any(|template| template.driver != name)
        {
            // Post-construction columns have no record owner shared by every
            // template. Every stored source uses its captured port and output keys.
            catalog = row_drivers::extend_catalog(catalog, source, name, keys, &secondary, "")?;
        }
        let groups = self
            .templates
            .iter()
            .filter_map(|t| t.groups.as_ref())
            .map(|g| g.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let column_groups = groups.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let resolve = |reference: &str,
                       path: &str,
                       source: &TableSchema,
                       name: &str,
                       findings: &mut Vec<BindFinding>| {
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
        let assignment = |source: &TableSchema,
                          name: &str,
                          catalog: &Catalog,
                          declaration: &RowDeclaration,
                          phase: Phase<'_>,
                          row: Option<&str>,
                          available: Option<&[usize]>,
                          edges: &mut Vec<usize>,
                          findings: &mut Vec<BindFinding>,
                          declarations_reported: bool|
         -> Result<Option<Assignment>, BindError> {
            let expression = match &declaration.operation {
                RowOperation::ProjectFunction(call) => {
                    let project = project.ok_or(BindError::Internal)?;
                    match crate::project_call_binding::bind(
                        project,
                        *call,
                        catalog,
                        crate::project_call_binding::Context {
                            source_dataset: 0,
                            source_mode: crate::project_call_binding::SourceMode::Record,
                            available_outputs: available,
                            scope: Scope {
                                drivers: &[name],
                                current_driver: true,
                                reach: Reach::Scalar,
                                joined: false,
                                phase,
                            },
                        },
                    ) {
                        Ok(bound) => {
                            edges.extend(bound.dependencies);
                            Some(Expression::ProjectFunction(bound.function))
                        }
                        Err(crate::project_call_binding::Error::Findings(errors)) => {
                            findings.extend(functions::bind_findings(project, *call, errors, row)?);
                            None
                        }
                        Err(crate::project_call_binding::Error::Reference(error)) => {
                            return Err(BindError::Catalog(error))
                        }
                        Err(_) => return Err(BindError::Internal),
                    }
                }
                RowOperation::Output(reference) => {
                    if let Some(finding) = catalog
                        .validate_output(reference, None, available, &[0])
                        .map_err(BindError::Catalog)?
                    {
                        findings.push(BindFinding::OutputReference {
                            path: declaration.path.clone(),
                            name: reference.clone(),
                            finding,
                        });
                        None
                    } else {
                        let Some(reference_binding::Binding::Output { column, .. }) =
                            catalog.bind(reference).map_err(BindError::Catalog)?
                        else {
                            return Err(BindError::Internal);
                        };
                        edges.push(column);
                        Some(Expression::Column(column))
                    }
                }
                RowOperation::Shared(branch) => derivations::bind_row(
                    &branch.operation,
                    &branch.path,
                    declaration.column,
                    catalog,
                    name,
                    row,
                    phase,
                    available,
                    edges,
                    findings,
                    declarations_reported,
                    output,
                    project,
                    named,
                    &secondary,
                    &intermediates,
                )?,
                RowOperation::Source(reference) => {
                    if reference
                        .split_once('.')
                        .is_some_and(|(dataset, _)| catalog.joined(dataset).is_some())
                    {
                        source_expressions::bind_catalog_join(
                            reference,
                            &declaration.path,
                            catalog,
                            row,
                            available,
                            edges,
                            findings,
                        )?
                    } else if named.reference(reference).is_some()
                        && reference
                            .split_once('.')
                            .is_none_or(|(owner, _)| owner != name)
                    {
                        row_intermediates::bind(
                            reference,
                            &declaration.path,
                            named,
                            &secondary,
                            &intermediates,
                            output,
                            catalog,
                            available,
                            edges,
                            findings,
                        )?
                    } else if let Some(column) =
                        resolve(reference, &declaration.path, source, name, findings)
                    {
                        let scoped = reference_scope::validate(
                            catalog,
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
            };
            Ok(expression.map(|expression| Assignment {
                column: declaration.column,
                expression,
                path: declaration.path.clone(),
            }))
        };
        let mut templates = Vec::new();
        for (index, template) in self.templates.iter().enumerate() {
            let filter_path = format!("rows[{index}].filter");
            if named
                .reference(&format!("{}.driver", template.driver))
                .is_some_and(|(index, _)| intermediates.get(index).is_some_and(Option::is_none))
                && !findings.is_empty()
            {
                continue;
            }
            let (driver_index, row_source, named_driver) = row_drivers::select(
                source,
                name,
                &secondary,
                &template.driver,
                named,
                &intermediates,
            )?;
            if let Some(named_driver) = named_driver {
                named.driver_findings(
                    &template.driver,
                    &format!("rows[{index}].dataset"),
                    intermediates[named_driver]
                        .as_ref()
                        .ok_or(BindError::Internal)?,
                    &secondary,
                    &mut findings,
                )?;
            }
            let row_catalog = row_drivers::catalog_visible(
                output,
                row_source,
                &template.driver,
                named_driver.and_then(|index| intermediates[index].as_ref()),
            )?;
            let row_catalog = row_drivers::extend_catalog(
                row_catalog,
                source,
                name,
                keys,
                &secondary,
                &template.driver,
            )?;
            let mode = match &template.groups {
                None => RowMode::Records,
                Some(groups) => RowMode::Groups(
                    groups
                        .iter()
                        .enumerate()
                        .filter_map(|(position, name)| {
                            let resolved = resolve(
                                name,
                                &format!("rows[{index}].group_by[{position}]"),
                                row_source,
                                &template.driver,
                                &mut findings,
                            );
                            resolved.filter(|&column| {
                                let visible = named_driver
                                    .and_then(|index| intermediates[index].as_ref())
                                    .is_none_or(|item| item.admits_column(column));
                                if !visible {
                                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                                        &format!("rows[{index}].group_by[{position}]"),
                                        name,
                                        Some(&template.driver),
                                        ReferenceCause::Value,
                                        None,
                                    )));
                                }
                                visible
                            })
                        })
                        .collect(),
                ),
            };
            let predicate = match &template.filter {
                Some((
                    text,
                    Err(predicate_compiler::Error::Parse(
                        crate::predicate_parser::ParseError::Grammar {
                            position, failure, ..
                        },
                    )),
                )) => {
                    findings.push(BindFinding::Lookup(LookupFinding::grammar(
                        &filter_path,
                        text,
                        position.character,
                        failure,
                    )?));
                    None
                }
                Some((_, Err(error))) => return Err(BindError::PredicatePolicy(error.clone())),
                Some((_, Ok(plan))) => Some(plan),
                None => None,
            };
            let grouped = template.groups.is_some();
            // Qualified grouped-filter failures precede derivation binding, as in reference planning.
            if let Some(plan) = predicate.filter(|_| grouped) {
                let mut seen = alloc::collections::BTreeSet::new();
                for identifier in plan
                    .identifiers()
                    .into_iter()
                    .filter(|name| name.contains('.'))
                {
                    if !seen.insert(identifier) {
                        continue;
                    }
                    findings.push(BindFinding::RowFilterPhase {
                        path: filter_path.clone(),
                        identifier: identifier.into(),
                        row: template.id.clone(),
                        grouped,
                        qualified: true,
                    });
                }
            }
            let group_names = template
                .groups
                .as_ref()
                .map(|g| g.iter().map(String::as_str).collect::<Vec<_>>());
            let phase = Phase::Row {
                group_by: group_names.as_deref(),
            };
            let available = template
                .declarations
                .iter()
                .map(|d| d.column)
                .collect::<Vec<_>>();
            let mut graph = alloc::vec![Vec::new(); output.columns().len()];
            let mut assignments = BTreeMap::new();
            // Reference planning validates one complete row template's authored
            // derivations before resolving any of that template's references.
            let mut declaration_findings = Vec::new();
            let mut declarations_reported = false;
            for declaration in &template.declarations {
                if let RowOperation::Shared(branch) = &declaration.operation {
                    declarations_reported |=
                        cases::padding_findings(&branch.operation, &mut declaration_findings)?;
                }
            }
            if declarations_reported {
                for declaration in &template.declarations {
                    if let RowOperation::Shared(branch) = &declaration.operation {
                        derivations::declaration_findings(
                            &branch.operation,
                            &branch.path,
                            &mut findings,
                        )?;
                    }
                }
            }
            for declaration in &template.declarations {
                if let Some(value) = assignment(
                    row_source,
                    &template.driver,
                    &row_catalog,
                    declaration,
                    phase,
                    Some(&template.id),
                    Some(&available),
                    &mut graph[declaration.column],
                    &mut findings,
                    declarations_reported,
                )? {
                    assignments.insert(declaration.column, value);
                }
            }
            let analyzed = crate::dependency_analysis::analyze(&graph, Default::default())
                .map_err(|_| BindError::Internal)?;
            if let Some(cycle) = analyzed.cycle {
                let mut paths = alloc::vec![""; output.columns().len()];
                for declaration in &template.declarations {
                    paths[declaration.column] = declaration.path.as_str();
                }
                let repeated = findings.iter().any(|finding| {
                    let BindFinding::Dependencies { paths: prior_paths, diagnostics, .. } = finding else {
                        return false;
                    };
                    diagnostics.iter().any(|diagnostic| {
                        matches!(diagnostic, column_dependencies::Diagnostic::Cycle { columns }
                            if columns == &cycle && cycle.iter().all(|&column| {
                                prior_paths.get(column).map(String::as_str) == paths.get(column).copied()
                            }))
                    })
                });
                if !repeated {
                    findings.push(BindFinding::Dependencies {
                        columns: output.columns().iter().map(|c| c.name.clone()).collect(),
                        paths: paths.into_iter().map(String::from).collect(),
                        diagnostics: alloc::vec![column_dependencies::Diagnostic::Cycle {
                            columns: cycle
                        }],
                    });
                }
            }
            let assignments: Vec<Assignment> = analyzed
                .order
                .into_iter()
                .filter_map(|column| assignments.remove(&column))
                .collect();
            let mut bindings = Vec::new();
            let before_filter = findings.len();
            if let Some(plan) = predicate {
                let mut seen = alloc::collections::BTreeSet::new();
                for identifier in plan.identifiers() {
                    if !seen.insert(identifier) {
                        continue;
                    }
                    if identifier.contains('.') {
                        if !grouped
                            && named.reference(identifier).is_some()
                            && identifier
                                .split_once('.')
                                .is_none_or(|(owner, _)| owner != template.driver)
                        {
                            let mut edges = Vec::new();
                            if let Some(Expression::Intermediate { index, column }) =
                                row_intermediates::bind(
                                    identifier,
                                    &filter_path,
                                    named,
                                    &secondary,
                                    &intermediates,
                                    output,
                                    &row_catalog,
                                    Some(&available),
                                    &mut edges,
                                    &mut findings,
                                )?
                            {
                                bindings.push(Binding {
                                    name: identifier.into(),
                                    read: Read::Intermediate { index, column },
                                });
                            }
                        } else if !grouped
                            && identifier
                                .split_once('.')
                                .is_some_and(|(owner, _)| row_catalog.joined(owner).is_some())
                        {
                            if let Some(Expression::Joined(read)) =
                                source_expressions::bind_catalog_join(
                                    identifier,
                                    &filter_path,
                                    &row_catalog,
                                    Some(&template.id),
                                    Some(&available),
                                    &mut Vec::new(),
                                    &mut findings,
                                )?
                            {
                                bindings.push(Binding {
                                    name: identifier.into(),
                                    read: Read::Joined(read),
                                });
                            }
                        } else if !grouped {
                            if let Some(column) = resolve(
                                identifier,
                                &filter_path,
                                row_source,
                                &template.driver,
                                &mut findings,
                            ) {
                                bindings.push(Binding {
                                    name: identifier.into(),
                                    read: Read::Source(column),
                                });
                            }
                        }
                    } else if let Some(declaration) = template
                        .declarations
                        .iter()
                        .find(|declaration| output.columns()[declaration.column].name == identifier)
                    {
                        bindings.push(Binding {
                            name: identifier.into(),
                            read: Read::Column(declaration.column),
                        });
                    } else {
                        findings.push(BindFinding::RowFilterPhase {
                            path: filter_path.clone(),
                            identifier: identifier.into(),
                            row: template.id.clone(),
                            grouped,
                            qualified: false,
                        });
                    }
                }
            }
            // Invalid templates never reach executable-plan admission. Keep semantic findings
            // instead of manufacturing a transport error for their incomplete bindings.
            let filter = if findings.len() != before_filter
                || grouped
                    && predicate.is_some_and(|plan| {
                        plan.identifiers()
                            .into_iter()
                            .any(|name| name.contains('.'))
                    }) {
                None
            } else {
                predicate
                    .map(|plan| BoundPredicate::new_contextual(plan.clone(), bindings, Vec::new()))
                    .transpose()
                    .map_err(BindError::InvalidPredicateBinding)?
            };
            if let Some(filter) = filter.as_ref() {
                let bound = intermediates
                    .iter()
                    .filter_map(Option::as_ref)
                    .cloned()
                    .collect::<Vec<_>>();
                let deferred =
                    crate::dataset::window_stages(&assignments, output.columns().len(), &bound);
                for binding in filter.bindings() {
                    if matches!(binding.read, Read::Column(column) if deferred.get(column).copied().unwrap_or(false))
                        || matches!(&binding.read, Read::Joined(read) if read.keys.iter().any(|key| deferred.get(key.output_column).copied().unwrap_or(true)))
                    {
                        findings.push(BindFinding::Scalar(crate::diagnostic::Diagnostic {
                            code: crate::diagnostic::ConditionCode::RowFilterPhaseBoundary,
                            spec_paths: vec![filter_path.clone()],
                            context: [
                                (
                                    "identifier".into(),
                                    crate::diagnostic::ContextValue::Scalar(
                                        crate::value::Value::Str(binding.name.clone()),
                                    ),
                                ),
                                (
                                    "row".into(),
                                    crate::diagnostic::ContextValue::Scalar(
                                        crate::value::Value::Str(template.id.clone()),
                                    ),
                                ),
                                (
                                    "available_phase".into(),
                                    crate::diagnostic::ContextValue::Scalar(
                                        crate::value::Value::Str("window_derivation".into()),
                                    ),
                                ),
                                (
                                    "required_phase".into(),
                                    crate::diagnostic::ContextValue::Scalar(
                                        crate::value::Value::Str("row_filter".into()),
                                    ),
                                ),
                            ]
                            .into(),
                            source_span: None,
                            operand_route: None,
                        }));
                    }
                }
            }
            templates.push(RowTemplate {
                mode: if let Some(index) = named_driver {
                    RowMode::IntermediateDriver {
                        index,
                        source: driver_index,
                        mode: alloc::boxed::Box::new(mode),
                    }
                } else if driver_index == 0 {
                    mode
                } else {
                    RowMode::Driver {
                        source: driver_index,
                        mode: alloc::boxed::Box::new(mode),
                    }
                },
                assignments,
                filter,
            });
        }
        let mut odm_contexts: Option<alloc::rc::Rc<[crate::dataset::OdmTemplateScope]>> = None;
        let mut columns = Vec::new();
        let mut dependencies = alloc::vec![None; output.columns().len()];
        for declaration in &self.columns {
            if let RowOperation::Shared(branch) = &declaration.operation {
                if let Operation::StrPad(operation) = branch.operation.as_ref() {
                    str_pad::declaration_findings(operation, &mut findings);
                }
            }
            let mut edges = Vec::new();
            if let Some(mut value) = assignment(
                source,
                name,
                &catalog,
                declaration,
                Phase::Column {
                    groups: &column_groups,
                },
                None,
                None,
                &mut edges,
                &mut findings,
                false,
            )? {
                crate::dataset::odm::visit_mut(&mut value.expression, &mut |bound| {
                    if odm_contexts.is_none() {
                        odm_contexts = Some(
                            deferred_odm_contexts(&self.templates, &templates)
                                .map_err(|_| crate::dataset::PlanError::InvalidSource)?,
                        );
                    }
                    bound.defer(odm_contexts.as_ref().unwrap().clone())
                })
                .map_err(BindError::InvalidPlan)?;
                columns.push(value);
            }
            dependencies[declaration.column] = Some(edges);
        }
        let analyzed = column_dependencies::analyze(&dependencies, keys, true, Default::default())
            .map_err(|_| BindError::Internal)?;
        if !analyzed.diagnostics.is_empty() {
            let mut paths = alloc::vec![String::new(); output.columns().len()];
            for declaration in &self.columns {
                paths[declaration.column] = declaration.path.clone();
            }
            findings.push(BindFinding::Dependencies {
                columns: output.columns().iter().map(|c| c.name.clone()).collect(),
                paths,
                diagnostics: analyzed.diagnostics,
            });
        }
        for template in &templates {
            relation_predicates::admit_reads(
                template
                    .assignments
                    .iter()
                    .chain(&columns)
                    .map(|assignment| &assignment.expression),
                &intermediates,
                template.mode.source_index(),
                &template.mode,
                &mut findings,
            );
        }
        if !findings.is_empty() {
            return Err(BindError::Invalid(findings));
        }
        let verifications =
            _prepared
                .verifications
                .bind_assertions(_prepared, &secondary, &intermediates)?;
        DatasetPlan::new_with_intermediates(
            crate::dataset::SourceSchemas {
                primary: source.clone(),
                secondary,
            },
            intermediates
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or(BindError::Internal)?,
            output.clone(),
            templates,
            columns,
            keys.to_vec(),
            verifications,
        )
        .and_then(|plan| {
            let contexts = self
                .templates
                .iter()
                .zip(plan.templates())
                .map(|(declaration, template)| {
                    match (&declaration.groups, template.mode.local_mode()) {
                        (Some(names), RowMode::Groups(columns)) => {
                            Some(crate::dataset::AggregateRowContext {
                                row: declaration.id.clone(),
                                groups: names
                                    .iter()
                                    .map(|name| {
                                        String::from(
                                            name.split_once('.')
                                                .map_or(name.as_str(), |(_, field)| field),
                                        )
                                    })
                                    .zip(columns.iter().copied())
                                    .collect(),
                            })
                        }
                        _ => None,
                    }
                })
                .collect();
            let plan = plan.with_row_contexts(contexts)?;
            // Repeated defaults in several templates share one declaration and
            // one counter. Only effective row declarations register handlers.
            let mut seen = alloc::collections::BTreeSet::new();
            let handlers = self
                .templates
                .iter()
                .flat_map(|template| &template.declarations)
                .chain(&self.columns)
                .filter_map(|declaration| {
                    declaration.handler.as_ref().and_then(|handler| {
                        seen.insert(&declaration.path)
                            .then(|| crate::dataset::ConversionHandler {
                                assignment_path: declaration.path.clone(),
                                handler: handler.clone(),
                            })
                    })
                })
                .collect();
            plan.with_conversion_handlers(handlers)
        })
        .map_err(BindError::InvalidPlan)
    }
}

impl Rows {
    pub(super) fn check_findings(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let groups = self
            .templates
            .iter()
            .filter_map(|template| template.groups.as_ref())
            .map(|group| group.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let column_groups = groups.iter().map(Vec::as_slice).collect::<Vec<_>>();
        for declaration in &self.columns {
            if let RowOperation::Shared(branch) = &declaration.operation {
                static_checks::scoped_branch(
                    branch,
                    Some((
                        "",
                        Phase::Column {
                            groups: &column_groups,
                        },
                    )),
                    findings,
                )?;
            }
        }
        for (index, template) in self.templates.iter().enumerate() {
            let groups = template
                .groups
                .as_ref()
                .map(|group| group.iter().map(String::as_str).collect::<Vec<_>>());
            if let Some((expression, compiled)) = &template.filter {
                static_checks::predicate(
                    compiled,
                    expression,
                    &format!("rows[{index}].filter"),
                    findings,
                )?;
            }
            for declaration in &template.declarations {
                if let RowOperation::Shared(branch) = &declaration.operation {
                    static_checks::scoped_branch(
                        branch,
                        Some((
                            &template.driver,
                            Phase::Row {
                                group_by: groups.as_deref(),
                            },
                        )),
                        findings,
                    )?;
                }
            }
        }
        Ok(())
    }
}
