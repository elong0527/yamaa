//! Closed row lowering with per-template defaults and shared group-scope validation.
//!
//! Preparation lowers each admitted column default once, keyed by its
//! authored document node, and shares the immutable [`RowDeclaration`]
//! across every row template that inherits it; authored row overrides keep
//! their own lowered payloads. Sharing covers only the immutable prepared
//! operation/literal/predicate payload: driver-specific admission still
//! runs for each template, and binding still runs per template against its
//! own driver, catalog, grouping phase and available outputs, so no bound
//! assignment is ever cached by declaration identity alone.
use super::*;
use crate::dataset::{ColumnBinding, ColumnScope, Verification};
use crate::{
    bound_expression::{Binding, BoundPredicate, Read},
    reference_scope::{self, Phase, Reach, Scope},
};

#[derive(Debug)]
enum RowOperation {
    Shared(cases::Branch),
    ProjectFunction { call: usize },
    Output(String),
    Source(String),
}
#[derive(Debug)]
struct RowDeclaration {
    handler: Option<crate::conversion::LiteralHandler>,
    column: usize,
    path: String,
    /// The document node of the derivation's operation.
    value: usize,
    operation: RowOperation,
}
impl RowDeclaration {
    /// Whether a qualified aggregate occurs anywhere in this declaration.
    fn aggregates(&self) -> bool {
        fn nested(operation: &Operation) -> bool {
            match operation {
                Operation::Aggregate(_) => true,
                Operation::Case(branches) => {
                    branches.iter().any(|branch| nested(&branch.operation))
                }
                _ => false,
            }
        }
        matches!(&self.operation, RowOperation::Shared(branch) if nested(&branch.operation))
    }
}
/// One lowered declaration, shared immutably across templates by node.
///
/// The payload (operation, literal values, predicate arenas and compiled
/// patterns, mapping/cut dictionaries) is authored once per document node;
/// everything template-specific (driver/catalog bindings, grouping phase,
/// available outputs, row diagnostics, conversion-handler occurrence
/// ownership and per-run state) lives outside this declaration.
type SharedDeclaration = std::sync::Arc<RowDeclaration>;
#[derive(Debug)]
struct Template {
    id: String,
    driver: String,
    groups: Option<Vec<String>>,
    declarations: Vec<SharedDeclaration>,
    filter: Option<(String, Result<CompiledExpression, CompileError>)>,
}
fn deferred_odm_contexts(
    declarations: &[Template],
    templates: &[RowTemplate],
) -> std::sync::Arc<[crate::dataset::OdmTemplateScope]> {
    declarations
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
        .into()
}
#[derive(Debug)]
pub(super) struct Rows {
    templates: Vec<Template>,
    columns: Vec<SharedDeclaration>,
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
    project: Option<&functions::Prepared>,
    extraction: &str_extract::Prepared,
    string_templates: &str_template::Prepared,
    intermediates: &intermediates::Declarations,
) -> Result<RowDeclaration, PrepareError> {
    closed_fields(d, id, &["value", "unconvertible"], &prefix)?;
    let handler = literal_handler(d, id, &prefix)?;
    let value = field(d, id, "value")?;
    let &[(op, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let op = text(d, op)?;
    let path = format!("{prefix}.{op}");
    let operation = match op {
        "function" if project.is_some() => {
            let project = project.unwrap();
            // The tree types the call by the return its definition declares.
            project
                .returns
                .get(&payload)
                .ok_or(PrepareError::Internal)?;
            RowOperation::ProjectFunction {
                call: *project.nodes.get(&payload).ok_or(PrepareError::Internal)?,
            }
        }
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
        )?),
    };
    Ok(RowDeclaration {
        handler,
        column,
        path,
        value,
        operation,
    })
}
/// Lower an authored derivation node at most once per preparation.
///
/// The cache key is the authored document node itself: one node has one
/// owning column and one authored path, so every caller that reaches the
/// same node is asking for the same immutable declaration. Authored row
/// overrides are distinct nodes and therefore retain distinct payloads.
/// `declaration` ignores its grouped flag, so the shared lowering does not
/// depend on any one template's grouping; grouping, driver admission and
/// binding remain per-template work outside this cache.
#[allow(clippy::too_many_arguments)]
fn lower_shared(
    cache: &mut BTreeMap<usize, SharedDeclaration>,
    d: &Document,
    id: usize,
    column: usize,
    prefix: String,
    project: Option<&functions::Prepared>,
    extraction: &str_extract::Prepared,
    string_templates: &str_template::Prepared,
    intermediates: &intermediates::Declarations,
) -> Result<SharedDeclaration, PrepareError> {
    if let Some(shared) = cache.get(&id) {
        return Ok(shared.clone());
    }
    let shared = SharedDeclaration::new(declaration(
        d,
        id,
        column,
        prefix,
        false,
        project,
        extraction,
        string_templates,
        intermediates,
    )?);
    cache.insert(id, shared.clone());
    Ok(shared)
}
/// Inspect references in supported row-local defaults without reading source data.
/// Literal text and qualified stored fields never promote an output declaration.
/// The third element names why the default is not row-local (SPEC-6) when
/// it is not: the derivation kind that breaks row locality.
fn default_reads(
    d: &Document,
    declaration: usize,
    output: &TableSchema,
    project: Option<&functions::Prepared>,
    string_templates: &str_template::Prepared,
) -> Result<(bool, Vec<usize>, Option<&'static str>), PrepareError> {
    let value = field(d, declaration, "value")?;
    let &[(operation, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let operation = text(d, operation)?;
    fn contains_window(d: &Document, value: usize, depth: usize) -> Result<bool, PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                None,
                None,
            ));
        }
        if ["row_number", "rank", "row_value", "locf", "baseline_flag"]
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
        Ok(false)
    }
    // A column window spans the complete constructed dataset, including when
    // a nested branch or another output-key default depends on it.
    let window_default = contains_window(d, value, 0)?;
    let mut reads = Vec::new();
    // Input-backed named reads can be row defaults (SPEC-6), while column
    // SELF reads retain the complete constructed donor pool (LOOKUP-7).
    let mut names = Vec::new();
    derivations::references(d, value, project, &mut names, 0)?;
    self_relations::template_references(d, value, string_templates, &mut names, 0)?;
    let mut self_default = false;
    for name in names {
        self_default |= self_relations::is_self_reference(d, &name)?;
        reads.extend(row_intermediates::default_dependencies(
            d,
            &name,
            output,
            string_templates,
        )?);
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
        "greatest" | "least" | "case" | "first_available" | "mapping" | "cut" | "row_number"
        | "rank" | "row_value" | "locf" | "baseline_flag" => {
            let mut names = Vec::new();
            derivations::references(d, value, project, &mut names, 0)?;
            for name in names {
                reference(&name);
            }
        }
        "source" => {
            let name = text(d, field(d, payload, "variable")?)?;
            reference(name);
        }
        "str_template" => {
            for name in string_templates.get(payload)?.identifiers() {
                reference(name);
            }
        }
        "round_half_away_from_zero" => reference(text(d, field(d, payload, "source")?)?),
        "diff" | "study_day" | "to_epoch_day" | "shift" | "duration_value" | "dtc"
        | "time_of_day" => {
            for &name in temporal::fields(operation) {
                if let Some(N::Text(source)) = d.field(payload, name).map(|id| &d.nodes()[id]) {
                    reference(source);
                }
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
            let condition = field(d, payload, "condition")?;
            match compile(text(d, condition)?, ExpressionContext::Predicate, "") {
                Ok(compiled) => {
                    for name in compiled.identifiers() {
                        reference(name);
                    }
                }
                Err(CompileError::Invalid { .. }) => (),
                Err(error) => return Err(PrepareError::expression(error, None)),
            }
        }
        "compute" => {
            let expression = text(d, field(d, payload, "expr")?)?;
            match compile(expression, ExpressionContext::Compute, "") {
                Ok(compiled) => {
                    for name in compiled.identifiers() {
                        reference(name);
                    }
                }
                Err(CompileError::Invalid { .. }) => {}
                Err(error) => return Err(PrepareError::expression(error, None)),
            }
        }
        "aggregate" => return Ok((false, reads, Some("aggregate"))),
        _ => {
            return Ok((
                !self_default,
                reads,
                self_default.then_some("named_intermediate"),
            ));
        }
    }
    Ok((
        !window_default && !self_default,
        reads,
        if window_default {
            Some("window")
        } else {
            self_default.then_some("named_intermediate")
        },
    ))
}

fn select_defaults(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    rows: &[usize],
    filter_defaults: Vec<std::collections::BTreeSet<usize>>,
    project: Option<&functions::Prepared>,
    string_templates: &str_template::Prepared,
) -> Result<Vec<Vec<usize>>, PrepareError> {
    debug_assert_eq!(rows.len(), filter_defaults.len());
    let defaults = columns
        .iter()
        .map(|&column| {
            d.field(column, "derivation")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| default_reads(d, id, output, project, string_templates))
                .transpose()
                .map(|default| default.unwrap_or((true, Vec::new(), None)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut local = defaults
        .iter()
        .map(|(local, _, _)| *local)
        .collect::<Vec<_>>();
    // A default that only reads a non-row-local column inherits that column's
    // root cause, so the finding names the original kind (SPEC-6).
    let mut not_row_local = defaults
        .iter()
        .map(|(_, _, reason)| *reason)
        .collect::<Vec<_>>();
    loop {
        let mut changed = false;
        for (column, (_, reads, _)) in defaults.iter().enumerate() {
            if local[column] {
                if let Some(&read) = reads.iter().find(|&&read| !local[read]) {
                    local[column] = false;
                    not_row_local[column] = not_row_local[read];
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // ODM defaults own every template's item scope and row identity even
    // when no row overrides or filter reference them; they are row-phase in
    // all templates.
    let mut odm_selected = std::vec![false; columns.len()];
    for (column, &node) in columns.iter().enumerate() {
        if let Some(derivation) = d
            .field(node, "derivation")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            if odm::contains(d, derivation, 0)? {
                odm_selected[column] = local[column];
            }
        }
    }
    let mut overridden: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    let mut per_template: Vec<Vec<bool>> = Vec::with_capacity(rows.len());
    for (row, template_defaults) in rows.iter().zip(&filter_defaults) {
        let mut selected = odm_selected.clone();
        for &column in template_defaults {
            selected[column] = local[column];
        }
        for &(name, declaration) in mapping(d, field(d, *row, "derivations")?)? {
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
                    .push(text(d, field(d, *row, "id")?)?);
            }
            let (_, reads, _) = default_reads(d, declaration, output, project, string_templates)?;
            for read in reads {
                if local[read] {
                    selected[read] = true;
                }
            }
        }
        loop {
            let mut changed = false;
            for (column, (_, reads, _)) in defaults.iter().enumerate() {
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
        per_template.push(selected);
    }
    // Grouping preserves authored row order and emits columns in declaration order.
    let findings = overridden
        .into_iter()
        .map(|(column, rows)| PreflightFinding::DuplicateRowDefault {
            column: output.columns()[column].name.clone(),
            rows: rows.into_iter().map(String::from).collect(),
            // Every non-row-local column carries a root cause by
            // construction; the fallback only guards future classifications.
            not_row_local: not_row_local[column].unwrap_or("non_row_local").into(),
        })
        .collect::<Vec<_>>();
    if !findings.is_empty() {
        return Err(PrepareError::Invalid(findings));
    }
    Ok(per_template
        .into_iter()
        .map(|selected| {
            selected
                .into_iter()
                .enumerate()
                .filter_map(|(column, selected)| selected.then_some(column))
                .collect()
        })
        .collect())
}

/// Validate every row-phase use before projecting its authored context.
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
    fn register<'a>(site: Site<'a>, output: &TableSchema, sites: &mut Vec<Site<'a>>) {
        if site.identifier.contains('.') {
            return;
        }
        let Some(column) = output
            .columns()
            .iter()
            .position(|c| c.name == site.identifier)
        else {
            // Unknown bare sources retain their ordinary binding diagnostic.
            return;
        };
        if site
            .template
            .declarations
            .iter()
            .any(|d| d.column == column)
        {
            return;
        }
        sites.push(site);
    }
    let mut sites = Vec::new();

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
                ),
                RowOperation::ProjectFunction { call, .. } => {
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
                            );
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
/// Keep the first of the findings from `start` on that project to the same
/// diagnostics: the bindings of one declaration for several drivers report a
/// failure they share once.
fn distinct_findings(findings: &mut Vec<BindFinding>, start: usize, source: &SourceDeclaration) {
    let mut seen = Vec::new();
    let mut index = start;
    while index < findings.len() {
        match findings[index].diagnostics(source) {
            Some(diagnostics) if seen.contains(&diagnostics) => {
                findings.remove(index);
            }
            Some(diagnostics) => {
                seen.push(diagnostics);
                index += 1;
            }
            None => index += 1,
        }
    }
}
fn resolve_reference(
    reference: &str,
    path: &str,
    source: &TableSchema,
    name: &str,
    findings: &mut Vec<BindFinding>,
) -> Option<usize> {
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
}

#[allow(clippy::too_many_arguments)]
fn bind_assignment(
    source: &TableSchema,
    name: &str,
    catalog: &Catalog,
    declaration: &RowDeclaration,
    phase: Phase<'_>,
    row: Option<&str>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
    project: Option<&crate::project_calls::ProjectCalls>,
    output: &TableSchema,
    tree: &types::Tree,
    named: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
) -> Result<Option<Assignment>, BindError> {
    let start = findings.len();
    let expression = match &declaration.operation {
        RowOperation::ProjectFunction { call, .. } => {
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
                    row: row.map(String::from),
                    finding,
                });
                None
            } else {
                let Some(reference_binding::Binding::Output { column, .. }) =
                    catalog.bind(reference)
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
            tree,
            named,
            secondary,
            intermediates,
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
                    secondary,
                    intermediates,
                    output,
                    catalog,
                    row,
                    available,
                    edges,
                    findings,
                )?
            } else if let Some(column) =
                resolve_reference(reference, &declaration.path, source, name, findings)
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
    // VALUE-24 once per column: a default shared by several templates states
    // one derivation.
    if expression.is_some() && findings.len() == start && declaration.handler.is_none() {
        let captured = types::CapturedTypes::new(named, catalog, secondary, intermediates);
        let result = tree.result_type(declaration.value, ast::Scope::Row, &captured)?;
        let column = &output.columns()[declaration.column];
        if let Some(finding) =
            types::conversion_finding(&format!("columns.{}", column.name), result, column.kind)
        {
            if !findings
                .iter()
                .any(|prior| matches!(prior, BindFinding::Scalar(prior) if *prior == finding))
            {
                findings.push(BindFinding::Scalar(finding));
            }
        }
    }
    Ok(expression.map(|expression| Assignment {
        column: declaration.column,
        expression,
        path: declaration.path.clone(),
    }))
}

#[allow(clippy::too_many_arguments)]
fn bind_template_filter(
    predicate: Option<&crate::expression::CompiledExpression>,
    grouped: bool,
    template: &Template,
    filter_path: &str,
    named: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    row_catalog: &Catalog,
    available: &[usize],
    row_source: &TableSchema,
    assignments: &[Assignment],
    output: &TableSchema,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<BoundPredicate>, BindError> {
    let mut bindings = Vec::new();
    let before_filter = findings.len();
    if let Some(plan) = predicate {
        let mut seen = std::collections::BTreeSet::new();
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
                            filter_path,
                            named,
                            secondary,
                            intermediates,
                            output,
                            row_catalog,
                            Some(&template.id),
                            Some(available),
                            &mut edges,
                            findings,
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
                    if let Some(Expression::Joined(read)) = source_expressions::bind_catalog_join(
                        identifier,
                        filter_path,
                        row_catalog,
                        Some(&template.id),
                        Some(available),
                        &mut Vec::new(),
                        findings,
                    )? {
                        bindings.push(Binding {
                            name: identifier.into(),
                            read: Read::Joined(read),
                        });
                    }
                } else if !grouped {
                    if let Some(column) = resolve_reference(
                        identifier,
                        filter_path,
                        row_source,
                        &template.driver,
                        findings,
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
                    path: filter_path.to_string(),
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
            && predicate.is_some_and(|plan| plan.identifiers().any(|name| name.contains('.')))
    {
        None
    } else {
        predicate
            .map(|plan| {
                BoundPredicate::new_contextual(
                    types::CapturedTypes::new(named, row_catalog, secondary, intermediates)
                        .typed(plan),
                    bindings,
                    Vec::new(),
                )
            })
            .transpose()
            .map_err(BindError::InvalidPredicateBinding)?
    };
    if let Some(filter) = filter.as_ref().filter(|_| !grouped) {
        let bound = intermediates
            .iter()
            .filter_map(Option::as_ref)
            .cloned()
            .collect::<Vec<_>>();
        let deferred = crate::dataset::window_stages(assignments, output.columns().len(), &bound);
        for binding in filter.bindings() {
            let lookup_match = match binding.read {
                Read::Intermediate { index, .. } => {
                    row_intermediates::dependencies(index, intermediates)?
                        .into_iter()
                        .find(|&column| deferred.get(column).copied().unwrap_or(false))
                }
                _ => None,
            };
            if lookup_match.is_some()
                || crate::dataset::read_requires_window(&binding.read, &deferred, &bound)
            {
                let mut context: crate::diagnostic::Context = [
                    (
                        "identifier".into(),
                        crate::json::Value::String(binding.name.clone()),
                    ),
                    (
                        "row".into(),
                        crate::json::Value::String(template.id.clone()),
                    ),
                    (
                        "available_phase".into(),
                        crate::json::Value::String("window_derivation".into()),
                    ),
                    (
                        "required_phase".into(),
                        crate::json::Value::String("row_filter".into()),
                    ),
                ]
                .into();
                if let Some(column) = lookup_match {
                    context.insert(
                        "lookup_match".into(),
                        crate::json::Value::String(output.columns()[column].name.clone()),
                    );
                }
                findings.push(BindFinding::Scalar(crate::diagnostic::Diagnostic {
                    code: crate::diagnostic::ConditionCode::RowFilterPhaseBoundary,
                    spec_paths: vec![filter_path.to_string()],
                    context,
                    source_span: None,
                    operand_route: None,
                }));
            }
        }
    }
    Ok(filter)
}

#[allow(clippy::too_many_arguments)]
fn bind_template(
    template: &Template,
    index: usize,
    source: &TableSchema,
    name: &str,
    keys: &[usize],
    tree: &types::Tree,
    named: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    output: &TableSchema,
    project: Option<&crate::project_calls::ProjectCalls>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RowTemplate>, BindError> {
    let filter_path = format!("rows[{index}].filter");
    if named
        .reference(&format!("{}.driver", template.driver))
        .is_some_and(|(index, _)| intermediates.get(index).is_some_and(Option::is_none))
        && !findings.is_empty()
    {
        return Ok(None);
    }
    let (driver_index, row_source, named_driver) = row_drivers::select(
        source,
        name,
        secondary,
        &template.driver,
        named,
        intermediates,
    )?;
    if let Some(named_driver) = named_driver {
        named.driver_findings(
            &template.driver,
            &format!("rows[{index}].dataset"),
            intermediates[named_driver]
                .as_ref()
                .ok_or(BindError::Internal)?,
            secondary,
            findings,
        )?;
    }
    let row_catalog = row_drivers::catalog_visible(
        output,
        row_source,
        &template.driver,
        named_driver.and_then(|index| intermediates[index].as_ref()),
    )?;
    let row_catalog =
        row_drivers::extend_catalog(row_catalog, source, name, keys, secondary, &template.driver)?;
    let row_catalog = row_catalog.with_row_joins(&template.driver, template.groups.as_deref());
    let mode = match &template.groups {
        None => RowMode::Records,
        Some(groups) => RowMode::Groups(
            groups
                .iter()
                .enumerate()
                .filter_map(|(position, name)| {
                    let resolved = resolve_reference(
                        name,
                        &format!("rows[{index}].group_by[{position}]"),
                        row_source,
                        &template.driver,
                        findings,
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
            Err(CompileError::Invalid {
                position, failure, ..
            }),
        )) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                &filter_path,
                text,
                position.character,
                failure,
            )));
            None
        }
        Some((_, Err(error))) => return Err(BindError::ExpressionPolicy(error.clone())),
        Some((_, Ok(plan))) => Some(plan),
        None => None,
    };
    let grouped = template.groups.is_some();
    // Qualified grouped-filter failures precede derivation binding, as in reference planning.
    if let Some(plan) = predicate.filter(|_| grouped) {
        let mut seen = std::collections::BTreeSet::new();
        for identifier in plan.identifiers().filter(|name| name.contains('.')) {
            if !seen.insert(identifier) {
                continue;
            }
            findings.push(BindFinding::RowFilterPhase {
                path: filter_path.to_string(),
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
    let mut graph = std::vec![Vec::new(); output.columns().len()];
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
                derivations::declaration_findings(&branch.operation, &branch.path, findings)?;
            }
        }
    }
    for declaration in &template.declarations {
        if let Some(value) = bind_assignment(
            row_source,
            &template.driver,
            &row_catalog,
            declaration,
            phase,
            Some(&template.id),
            Some(&available),
            &mut graph[declaration.column],
            findings,
            declarations_reported,
            project,
            output,
            tree,
            named,
            secondary,
            intermediates,
        )? {
            assignments.insert(declaration.column, value);
        }
    }
    let analyzed = crate::dependency_analysis::analyze(&graph).map_err(|_| BindError::Internal)?;
    if let Some(cycle) = analyzed.cycle {
        let mut paths = std::vec![""; output.columns().len()];
        for declaration in &template.declarations {
            paths[declaration.column] = declaration.path.as_str();
        }
        let repeated = findings.iter().any(|finding| {
            let BindFinding::Dependencies {
                paths: prior_paths,
                diagnostics,
                ..
            } = finding
            else {
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
                diagnostics: std::vec![column_dependencies::Diagnostic::Cycle { columns: cycle }],
            });
        }
    }
    let assignments: Vec<Assignment> = analyzed
        .order
        .into_iter()
        .filter_map(|column| assignments.remove(&column))
        .collect();
    let filter = bind_template_filter(
        predicate,
        grouped,
        template,
        &filter_path,
        named,
        secondary,
        intermediates,
        &row_catalog,
        &available,
        row_source,
        &assignments,
        output,
        findings,
    )?;
    Ok(Some(RowTemplate {
        mode: if let Some(index) = named_driver {
            RowMode::IntermediateDriver {
                index,
                source: driver_index,
                mode: std::boxed::Box::new(mode),
            }
        } else if driver_index == 0 {
            mode
        } else {
            RowMode::Driver {
                source: driver_index,
                mode: std::boxed::Box::new(mode),
            }
        },
        assignments,
        filter,
    }))
}

type RowFilterDeclaration = Option<(String, Result<CompiledExpression, CompileError>)>;

fn prepare_row_filters(
    d: &Document,
    raw_rows: &[usize],
) -> Result<Vec<RowFilterDeclaration>, PrepareError> {
    raw_rows
        .iter()
        .enumerate()
        .map(|(index, &row)| {
            d.field(row, "filter")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| {
                    let expression = text(d, id)?;
                    let path = format!("rows[{index}].filter");
                    let compiled = compile(expression, ExpressionContext::Predicate, &path);
                    match &compiled {
                        Ok(_) | Err(CompileError::Invalid { .. }) => {}
                        Err(error) => {
                            return Err(PrepareError::expression(error.clone(), Some(&path)))
                        }
                    }
                    Ok((String::from(expression), compiled))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, PrepareError>>()
}

#[allow(clippy::too_many_arguments)]
fn prepare_filter_defaults(
    d: &Document,
    output: &TableSchema,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    string_templates: &str_template::Prepared,
    raw_rows: &[usize],
    filters: &[RowFilterDeclaration],
    columns: &[usize],
) -> Result<Vec<std::collections::BTreeSet<usize>>, PrepareError> {
    // Needs are tracked per template: a selector or secondary read in one
    // template promotes defaults for that template only, so a suppressed
    // candidate in another template never reads the promoted lookup.
    let donor =
        self_relations::donor_defaults(d, output, project, intermediates, string_templates)?;
    let mut filter_defaults = vec![donor; raw_rows.len()];
    for (index, (&row, filter)) in raw_rows.iter().zip(filters.iter()).enumerate() {
        let Some((_, Ok(plan))) = filter else {
            continue;
        };
        for name in plan.identifiers() {
            if !present(d, row, "group_by") {
                // A named lookup in an ungrouped filter also reads its
                // match defaults from this candidate's derived values.
                filter_defaults[index].extend(row_intermediates::default_dependencies(
                    d,
                    name,
                    output,
                    string_templates,
                )?);
            }
            if name.contains('.') {
                continue;
            }
            let Some(column) = output.columns().iter().position(|c| c.name == name) else {
                continue;
            };
            if d.field(columns[column], "derivation")
                .is_some_and(|id| !matches!(d.nodes()[id], N::Null))
            {
                filter_defaults[index].insert(column);
            }
        }
    }
    Ok(filter_defaults)
}

impl Rows {
    /// Each row template's driver name, in construction order.
    pub(super) fn drivers(&self) -> impl Iterator<Item = &str> {
        self.templates
            .iter()
            .map(|template| template.driver.as_str())
    }
    /// Each grouped row template's driver with its `group_by`, in
    /// construction order: the column phase checks a read of a driver
    /// against the groups of that driver's templates (NAME-4).
    fn column_groups(&self) -> Vec<(&str, Vec<&str>)> {
        self.templates
            .iter()
            .filter_map(|template| {
                let group = template.groups.as_ref()?;
                Some((
                    template.driver.as_str(),
                    group.iter().map(String::as_str).collect(),
                ))
            })
            .collect()
    }
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
        project: Option<&functions::Prepared>,
        _column_terminology: bool,
        extraction: &str_extract::Prepared,
        string_templates: &str_template::Prepared,
        intermediates: &intermediates::Declarations,
    ) -> Result<Self, PrepareError> {
        let root = d.root();
        let raw_rows = sequence(d, field(d, root, "rows")?)?;
        if raw_rows.is_empty() {
            return Err(PrepareError::Internal);
        }
        let columns = sequence(d, field(d, root, "columns")?)?;
        let filters = prepare_row_filters(d, raw_rows)?;
        let mut filter_defaults = prepare_filter_defaults(
            d,
            output,
            project,
            intermediates,
            string_templates,
            raw_rows,
            &filters,
            columns,
        )?;
        // Complete row-local identity defaults before a newly admitted selector can fail.
        // A key column becomes a row default here only when the row phase
        // actually needs its value before the filter: an implicit join
        // matches on the output keys when a row-phase filter or derivation
        // reads another input dataset, and a named intermediate read by a
        // row-phase filter or derivation admits that intermediate as a
        // selector for the template, whose matching is keyed on the row
        // identity. The mere presence of intermediates is not a row-phase
        // context (SPEC-6): promoting a key column whose derivation reads
        // a named lookup would make candidates the filter suppresses read
        // that lookup, and an intermediate without `no_match` must never
        // fail `unmatched_key` for them (LOOKUP-12). Named reads by a filter,
        // a row derivation, or another default already promote the match
        // columns they need through `default_dependencies`.
        // A reference owned by the template's own driver binds as a source
        // read, never as an intermediate read, so it admits no selector.
        // Needs are tracked per template: a selector or secondary read in
        // one template promotes keys for that template only, so a suppressed
        // candidate in another template never reads the promoted lookup.
        let named_read = |read: &str, row_driver: &str| -> bool {
            read.split_once('.')
                .is_some_and(|(owner, _)| owner != row_driver && intermediates.contains(owner))
        };
        let mut secondary_reads = vec![false; raw_rows.len()];
        let mut selector_reads = vec![false; raw_rows.len()];
        for (row_index, &row) in raw_rows.iter().enumerate() {
            let row_driver = d
                .field(row, "dataset")
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or(driver);
            if let Some((_, Ok(plan))) = &filters[row_index] {
                for read in plan.identifiers() {
                    if !selector_reads[row_index] && named_read(read, row_driver) {
                        selector_reads[row_index] = true;
                    }
                    if !secondary_reads[row_index]
                        && read.split_once('.').is_some_and(|(source, _)| {
                            source != row_driver
                                && d.field(
                                    field(d, root, "input").expect("admitted sources"),
                                    source,
                                )
                                .is_some()
                        })
                    {
                        secondary_reads[row_index] = true;
                    }
                }
            }
            for &(_, declaration) in mapping(d, field(d, row, "derivations")?)? {
                let mut reads = Vec::new();
                derivations::current_references(
                    d,
                    field(d, declaration, "value")?,
                    project,
                    &mut reads,
                    0,
                )?;
                self_relations::template_references(
                    d,
                    field(d, declaration, "value")?,
                    string_templates,
                    &mut reads,
                    0,
                )?;
                for read in &reads {
                    if !selector_reads[row_index] && named_read(read, row_driver) {
                        selector_reads[row_index] = true;
                    }
                    if !secondary_reads[row_index]
                        && read.split_once('.').is_some_and(|(source, _)| {
                            source != row_driver
                                && d.field(
                                    field(d, root, "input").expect("admitted sources"),
                                    source,
                                )
                                .is_some()
                        })
                    {
                        secondary_reads[row_index] = true;
                    }
                }
            }
        }
        let needs_keys: Vec<bool> = secondary_reads
            .iter()
            .zip(&selector_reads)
            .map(|(secondary, selector)| *secondary || *selector)
            .collect();
        let global_needs_keys = needs_keys.iter().any(|&needs| needs);
        for &key in sequence(d, field(d, root, "keys")?)? {
            let Some(column) = output
                .columns()
                .iter()
                .position(|column| text(d, key).is_ok_and(|key| key == column.name))
            else {
                continue;
            };
            let Some(derivation) = d
                .field(columns[column], "derivation")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            else {
                continue;
            };
            let value = field(d, derivation, "value")?;
            let &[(operation, _)] = mapping(d, value)? else {
                return Err(PrepareError::Internal);
            };
            if matches!(
                text(d, operation)?,
                "row_number" | "rank" | "row_value" | "locf" | "baseline_flag"
            ) {
                // An output-key window default spans the complete constructed
                // dataset. Selection identity cannot make it template-local.
                continue;
            }
            // A key whose derivation reads a named lookup must stay
            // template-local: promoting it globally would make suppressed
            // candidates in templates that never read it read the lookup
            // before their filter (LOOKUP-12). Safe keys (source, literal)
            // stay row-phase everywhere once any template needs complete
            // keys, so column-phase source reads never run under grouped
            // contexts they cannot admit.
            let mut reads = Vec::new();
            derivations::current_references(d, value, project, &mut reads, 0)?;
            self_relations::template_references(d, value, string_templates, &mut reads, 0)?;
            let reads_lookup = reads.iter().any(|read| {
                read.split_once('.')
                    .is_some_and(|(owner, _)| intermediates.contains(owner))
            });
            if reads_lookup {
                for (index, needs) in needs_keys.iter().enumerate() {
                    if *needs {
                        filter_defaults[index].insert(column);
                    }
                }
            } else if global_needs_keys {
                for template_defaults in filter_defaults.iter_mut() {
                    template_defaults.insert(column);
                }
            }
        }
        // Source-reading defaults cannot complete column-phase under grouped
        // contexts they cannot admit. When a template groups, keep safe
        // (non-lookup, non-window) defaults that read the grouped driver
        // row-phase everywhere. Lookup defaults stay per-template above, so
        // suppressed candidates in templates that never read them never read
        // the lookup (LOOKUP-12). Defaults that read only other datasets
        // (for example a reference date from a different input) stay
        // column-phase: promoting them into the grouped row context would
        // misbind their source (sdtm-lb, sdtm-vs-planned-time-points).
        {
            use std::collections::BTreeSet;
            let mut grouped_drivers: BTreeSet<String> = BTreeSet::new();
            for &row in raw_rows.iter() {
                if present(d, row, "group_by") {
                    let row_driver = d
                        .field(row, "dataset")
                        .map(|id| text(d, id))
                        .transpose()?
                        .unwrap_or(driver);
                    grouped_drivers.insert(row_driver.to_owned());
                }
            }
            if !grouped_drivers.is_empty() {
                for (column, &node) in columns.iter().enumerate() {
                    let Some(derivation) = d
                        .field(node, "derivation")
                        .filter(|&id| !matches!(d.nodes()[id], N::Null))
                    else {
                        continue;
                    };
                    let value = field(d, derivation, "value")?;
                    let &[(operation, _)] = mapping(d, value)? else {
                        return Err(PrepareError::Internal);
                    };
                    if matches!(
                        text(d, operation)?,
                        "row_number"
                            | "rank"
                            | "row_value"
                            | "locf"
                            | "baseline_flag"
                            | "aggregate"
                            | "mapping"
                    ) {
                        continue;
                    }
                    let mut reads = Vec::new();
                    derivations::current_references(d, value, project, &mut reads, 0)?;
                    self_relations::template_references(d, value, string_templates, &mut reads, 0)?;
                    if reads.iter().any(|read| {
                        read.split_once('.')
                            .is_some_and(|(owner, _)| intermediates.contains(owner))
                    }) {
                        continue;
                    }
                    let reads_grouped_driver = reads.iter().any(|read| {
                        read.split_once('.')
                            .is_some_and(|(owner, _)| grouped_drivers.contains(owner))
                    });
                    if !reads_grouped_driver {
                        continue;
                    }
                    for template_defaults in filter_defaults.iter_mut() {
                        template_defaults.insert(column);
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
        // Admission lowers every selected default once and keeps it: the
        // per-template pass below inherits these shared declarations
        // instead of lowering the same authored default again per template.
        // The union warms the cache; inherited defaults share the
        // `columns.<name>.derivation` node while authored overrides keep
        // their own `rows[i].derivations.<name>` nodes.
        let mut lowered: BTreeMap<usize, SharedDeclaration> = BTreeMap::new();
        {
            let mut union: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
            for template_selected in &selected {
                union.extend(template_selected.iter().copied());
            }
            for column in union {
                let metadata = &output.columns()[column];
                if let Some(id) = d
                    .field(columns[column], "derivation")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    lower_shared(
                        &mut lowered,
                        d,
                        id,
                        column,
                        format!("columns.{}.derivation", metadata.name),
                        project,
                        extraction,
                        string_templates,
                        intermediates,
                    )?;
                }
            }
        }
        for (index, (&row, filter)) in raw_rows.iter().zip(filters).enumerate() {
            let path = format!("rows[{index}]");
            closed_fields(
                d,
                row,
                &["id", "dataset", "group_by", "derivations", "filter"],
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
                    values
                        .iter()
                        .map(|&id| text(d, id).map(String::from))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let entries = mapping(d, field(d, row, "derivations")?)?;
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
            for &column in &selected[index] {
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
                // Driver-specific admission still runs for every template;
                // only the immutable lowering is shared through the cache.
                row_drivers::admit_reads(
                    d,
                    id,
                    Some(row_driver),
                    &prefix,
                    project,
                    string_templates,
                    intermediates,
                )?;
                declarations.push(lower_shared(
                    &mut lowered,
                    d,
                    id,
                    column,
                    prefix,
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
        // A grouped template's filter reads no qualified name at all
        // (ROW-6), which binding reports for every template.
        if let (None, Some((_, Ok(plan)))) = (&templates[0].groups, &templates[0].filter) {
            self_admission.filter(plan, "rows[0].filter");
        }
        let self_findings = self_admission.finish();
        if !self_findings.is_empty() {
            return Err(PrepareError::Invalid(self_findings));
        }
        validate_row_phases(&templates, project, output)?;
        // A column-level window spans the rows of every row template and
        // reads a driver field at the record that built each row, so it may
        // read only a driver they all share (WINDOW-11). The records that
        // build rows belong to each distinct driver and, for a driver
        // intermediate, to the input it reads.
        let mut drivers: Vec<&str> = Vec::new();
        for template in &templates {
            if !drivers.contains(&template.driver.as_str()) {
                drivers.push(&template.driver);
            }
        }
        let declared_inputs = mapping(d, field(d, root, "input")?)?;
        let mut records = drivers.clone();
        for &name in &drivers {
            if let Some(index) = intermediates.input(name) {
                let (input, _) = declared_inputs.get(index).ok_or(PrepareError::Internal)?;
                let input = text(d, *input)?;
                if !records.contains(&input) {
                    records.push(input);
                }
            }
        }
        let mut window_drivers = Vec::new();
        let mut lowered_columns = Vec::new();
        let mut unaddressable = Vec::new();
        for (column, &id) in columns.iter().enumerate() {
            let prefix = format!("columns.{}", output.columns()[column].name);
            // Third state: a default that is row-phase for some templates
            // stays column-phase for the rest. Columns selected in every
            // template need no column assignment; columns missing from at
            // least one template are completed column-phase for the rows
            // that did not derive them up front.
            if !selected.iter().all(|template| template.contains(&column)) {
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
                let declaration = lower_shared(
                    &mut lowered,
                    d,
                    derivation,
                    column,
                    format!("{prefix}.derivation"),
                    project,
                    extraction,
                    string_templates,
                    intermediates,
                )?;
                if let RowOperation::Shared(branch) = &declaration.operation {
                    windows::column_driver_findings(
                        &branch.operation,
                        &drivers,
                        &records,
                        &mut window_drivers,
                    );
                }
                // Every driver binds the column for its own rows (#2829); a
                // named intermediate's unaddressable record is refused here.
                let mut needing: Vec<&str> = Vec::new();
                for (template, selected) in templates.iter().zip(&selected) {
                    if !selected.contains(&column) && !needing.contains(&template.driver.as_str()) {
                        needing.push(&template.driver);
                    }
                }
                for driver in needing {
                    if intermediates.contains(driver)
                        && row_drivers::unaddressable(
                            d,
                            derivation,
                            driver,
                            declaration.aggregates(),
                            project,
                            string_templates,
                        )?
                    {
                        unaddressable.push(UnsupportedFeature {
                            operation: "intermediate_driver_column_read".into(),
                            path: declaration.path.clone(),
                        });
                        break;
                    }
                }
                lowered_columns.push(declaration);
            }
        }
        // A language refusal comes before an implementation refusal.
        if !window_drivers.is_empty() {
            return Err(PrepareError::Invalid(
                window_drivers
                    .into_iter()
                    .map(PreflightFinding::Scalar)
                    .collect(),
            ));
        }
        if !unaddressable.is_empty() {
            return Err(PrepareError::Unsupported(unaddressable));
        }
        Ok(Self {
            templates,
            columns: lowered_columns,
        })
    }
    /// Each row declaration's operation node and path, with the branch it
    /// shares with case and column lowering.
    pub(super) fn audit_entries(
        &self,
    ) -> impl Iterator<Item = (usize, &str, Option<&cases::Branch>)> + '_ {
        self.templates
            .iter()
            .flat_map(|template| &template.declarations)
            .chain(&self.columns)
            .map(|declaration| {
                let branch = match &declaration.operation {
                    RowOperation::Shared(branch) => Some(branch),
                    _ => None,
                };
                (declaration.value, declaration.path.as_str(), branch)
            })
    }
    /// The operation node of each row declaration: once for each template
    /// that evaluates it at the row phase, and once if the column phase
    /// completes it.
    pub(super) fn declaration_values(&self) -> impl Iterator<Item = usize> + '_ {
        self.audit_entries().map(|(value, ..)| value)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind(
        &self,
        prepared: &PreparedSpecification,
        source: &TableSchema,
        name: &str,
        output: &TableSchema,
        keys: &[usize],
        verifications: &[Verification],
        project: Option<&crate::project_calls::ProjectCalls>,
        named: &intermediates::Declarations,
        secondary: Vec<crate::dataset::SecondarySource>,
        intermediates: Vec<Option<crate::dataset::Intermediate>>,
        mut findings: Vec<BindFinding>,
    ) -> Result<DatasetPlan, BindError> {
        // The whole-dataset catalog binds a column holding a qualified
        // aggregate; every other column binds against each driver's own.
        let mut catalog = row_drivers::joined_catalog(output, source, name, keys, &secondary)?;
        if self
            .templates
            .iter()
            .any(|template| template.driver != name)
        {
            // The rows have no record owner shared by every template. Every
            // stored source uses its captured port and output keys.
            catalog = row_drivers::extend_catalog(catalog, source, name, keys, &secondary, "")?;
        }
        let groups = self.column_groups();
        let column_groups = groups
            .iter()
            .map(|(driver, group)| (*driver, group.as_slice()))
            .collect::<Vec<_>>();
        let mut templates = Vec::new();
        // The declared template behind each bound one, by position.
        let mut declared = Vec::new();
        for (index, template) in self.templates.iter().enumerate() {
            if let Some(bound) = bind_template(
                template,
                index,
                source,
                name,
                keys,
                &prepared.tree,
                named,
                &secondary,
                &intermediates,
                output,
                project,
                &mut findings,
            )? {
                templates.push(bound);
                declared.push(template);
            }
        }
        // A column derivation evaluates only for the rows of the bound
        // templates that leave the column to the column phase. A template
        // whose driver intermediate failed to bind was skipped with its own
        // findings, and so are its rows here.
        let leaves = |template: &Template, column: usize| {
            !template
                .declarations
                .iter()
                .any(|declaration| declaration.column == column)
        };
        let mut odm_contexts: Option<std::sync::Arc<[crate::dataset::OdmTemplateScope]>> = None;
        let mut columns: Vec<ColumnBinding> = Vec::new();
        let mut dependencies = std::vec![None; output.columns().len()];
        for declaration in &self.columns {
            if let RowOperation::Shared(branch) = &declaration.operation {
                if let Operation::StrPad(operation) = branch.operation.as_ref() {
                    str_pad::declaration_findings(operation, &mut findings);
                }
            }
            let needing = declared
                .iter()
                .enumerate()
                .filter(|(_, template)| leaves(template, declaration.column))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if needing.is_empty() {
                dependencies[declaration.column] = Some(Vec::new());
                continue;
            }
            let scopes: Vec<(ColumnScope, Vec<usize>)> = if declaration.aggregates() {
                // A qualified aggregate reduces a declared input through the
                // primary port (LOOKUP-18), so its column keeps the
                // whole-dataset scope, whichever driver builds the rows that
                // evaluate it. Preparation has refused that column's reads of
                // a driver intermediate.
                std::vec![(ColumnScope::Dataset, needing)]
            } else {
                // Every driver binds the column once, for the templates it
                // drives: its qualifier reads the record that built the row
                // (LOOKUP-1), another input joins on the output keys it
                // carries, and a named lookup matches through its key.
                let mut scopes: Vec<(ColumnScope, Vec<usize>)> = Vec::new();
                for index in needing {
                    let driver = &declared[index].driver;
                    match scopes
                        .iter_mut()
                        .find(|(_, members)| declared[members[0]].driver == *driver)
                    {
                        Some((_, members)) => members.push(index),
                        None => scopes.push((ColumnScope::Template, std::vec![index])),
                    }
                }
                scopes
            };
            let bound = scopes.len();
            let start = findings.len();
            let mut edges = Vec::new();
            for (scope, members) in scopes {
                let column_catalog;
                let (catalog_ref, column_source, column_driver, port) = match scope {
                    ColumnScope::Dataset => (&catalog, source, name, 0),
                    ColumnScope::Template => {
                        // Resolve the driver as its row template does: an
                        // input's stored schema or the intermediate's visible
                        // fields, without the row joins, so another input
                        // still joins on the output keys.
                        let driver = declared[members[0]].driver.as_str();
                        let (port, driver_source, named_driver) = row_drivers::select(
                            source,
                            name,
                            &secondary,
                            driver,
                            named,
                            &intermediates,
                        )?;
                        column_catalog = row_drivers::extend_catalog(
                            row_drivers::catalog_visible(
                                output,
                                driver_source,
                                driver,
                                named_driver.and_then(|index| intermediates[index].as_ref()),
                            )?,
                            source,
                            name,
                            keys,
                            &secondary,
                            driver,
                        )?;
                        (&column_catalog, driver_source, driver, port)
                    }
                };
                let mut binding_edges = Vec::new();
                if let Some(mut value) = bind_assignment(
                    column_source,
                    column_driver,
                    catalog_ref,
                    declaration,
                    Phase::Column {
                        groups: &column_groups,
                    },
                    None,
                    None,
                    &mut binding_edges,
                    &mut findings,
                    false,
                    project,
                    output,
                    &prepared.tree,
                    named,
                    &secondary,
                    &intermediates,
                )? {
                    // An ODM read takes the scope of each row's template.
                    crate::dataset::odm::visit_mut(&mut value.expression, &mut |bound| {
                        let contexts = odm_contexts.get_or_insert_with(|| {
                            deferred_odm_contexts(&self.templates, &templates)
                        });
                        bound.defer(contexts.clone(), port)
                    })
                    .map_err(BindError::InvalidPlan)?;
                    columns.push(ColumnBinding {
                        assignment: value,
                        templates: members,
                        scope,
                    });
                }
                for edge in binding_edges {
                    if !edges.contains(&edge) {
                        edges.push(edge);
                    }
                }
            }
            // A read that several bindings of one declaration make the same
            // way is one authored read: its failure is reported once.
            if bound > 1 {
                distinct_findings(&mut findings, start, prepared.source());
            }
            dependencies[declaration.column] = Some(edges);
        }
        let analyzed = column_dependencies::analyze(&dependencies, keys, true)
            .map_err(|_| BindError::Internal)?;
        if !analyzed.diagnostics.is_empty() {
            let mut paths = std::vec![String::new(); output.columns().len()];
            for declaration in &self.columns {
                paths[declaration.column] = declaration.path.clone();
            }
            findings.push(BindFinding::Dependencies {
                columns: output.columns().iter().map(|c| c.name.clone()).collect(),
                paths,
                diagnostics: analyzed.diagnostics,
            });
        }
        // A column binding evaluates only for the rows of the templates it
        // completes, a dataset verification for the rows of every template,
        // and a template's own derivations and row filter for its rows only
        // (LOOKUP-8, LOOKUP-9).
        let verification_reads = relation_predicates::predicate_reads(
            relation_predicates::verification_predicates(verifications),
        );
        let filter_reads = templates
            .iter()
            .map(|template| relation_predicates::predicate_reads(template.filter.as_ref()))
            .collect::<Vec<_>>();
        let contexts = columns
            .iter()
            .map(|binding| {
                (
                    vec![&binding.assignment.expression],
                    binding
                        .templates
                        .iter()
                        .map(|&template| templates[template].mode.source_index())
                        .collect(),
                )
            })
            .chain(std::iter::once((
                verification_reads.iter().collect(),
                templates
                    .iter()
                    .map(|template| template.mode.source_index())
                    .collect(),
            )))
            .chain(
                templates
                    .iter()
                    .zip(&filter_reads)
                    .map(|(template, filter)| {
                        (
                            template
                                .assignments
                                .iter()
                                .map(|assignment| &assignment.expression)
                                .chain(filter)
                                .collect(),
                            vec![template.mode.source_index()],
                        )
                    }),
            )
            .collect::<Vec<(Vec<&Expression>, Vec<usize>)>>();
        lookup_range::admit(contexts.iter().cloned(), &intermediates, &mut findings);
        aggregates::admit_record_keys(&contexts, &mut findings);
        let mut admitted = Vec::new();
        for (index, ((template, declaration), filter)) in templates
            .iter()
            .zip(&declared)
            .zip(&filter_reads)
            .enumerate()
        {
            let mut template_findings = Vec::new();
            relation_predicates::admit_reads(
                template
                    .assignments
                    .iter()
                    .chain(
                        columns
                            .iter()
                            .filter(|binding| binding.templates.contains(&index))
                            .map(|binding| &binding.assignment),
                    )
                    .map(|assignment| &assignment.expression)
                    .chain(filter)
                    .chain(&verification_reads),
                &intermediates,
                named,
                template.mode.source_index(),
                &template.mode,
                Some((&declaration.id, &declaration.driver)),
                &mut template_findings,
            );
            // A read that several templates make the same way is one authored
            // read: its failure is reported once.
            for finding in template_findings {
                if !admitted
                    .iter()
                    .any(|prior| relation_predicates::same_finding(prior, &finding))
                {
                    admitted.push(finding);
                }
            }
        }
        findings.extend(admitted);
        if !findings.is_empty() {
            return Err(BindError::Invalid(findings));
        }
        let plan = DatasetPlan::new_with_bindings(
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
            verifications.to_vec(),
        );
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
        let plan = plan.with_row_contexts(contexts);
        // Repeated defaults in several templates share one declaration and
        // one counter. Only effective row declarations register handlers.
        let mut seen = std::collections::BTreeSet::new();
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
        Ok(plan.with_conversion_handlers(handlers))
    }
}

impl Rows {
    /// `primary` is the input behind the first row template, which binds a
    /// column holding a qualified aggregate, as in [`Rows::bind`].
    pub(super) fn check_findings(
        &self,
        output: &TableSchema,
        primary: &str,
        tree: &types::Tree,
        declared: &types::DeclaredTypes<'_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        self.declaration_findings(primary, findings)?;
        // Static types in binding order: each template's derivations, then the
        // column-phase defaults, a shared default stating one derivation.
        for declaration in self
            .templates
            .iter()
            .flat_map(|template| &template.declarations)
            .chain(&self.columns)
        {
            let start = findings.len();
            if let RowOperation::Shared(branch) = &declaration.operation {
                static_checks::derivation_types(&branch.operation, tree, declared, findings)?;
            }
            if declaration.handler.is_some() || findings.len() != start {
                continue;
            }
            let column = &output.columns()[declaration.column];
            let result = tree.result_type(declaration.value, ast::Scope::Row, declared)?;
            if let Some(finding) =
                types::conversion_finding(&format!("columns.{}", column.name), result, column.kind)
            {
                if !findings
                    .iter()
                    .any(|prior| matches!(prior, BindFinding::Scalar(prior) if *prior == finding))
                {
                    findings.push(BindFinding::Scalar(finding));
                }
            }
        }
        Ok(())
    }
    /// The contexts whose rows read lookup ranges, in binding's order
    /// ([`lookup_range::check`]): the column phase, whose derivations and
    /// dataset verifications (`verified`) evaluate for the rows of every
    /// template, then each template, whose derivations and filter evaluate
    /// for its rows only. A row's driver is the input behind its template,
    /// named directly or through a named intermediate.
    pub(super) fn range_contexts<'a>(
        &'a self,
        sources: &'a [SourceDeclaration],
        named: &intermediates::Declarations,
        verified: impl Iterator<Item = lookup_range::Evaluated<'a>>,
    ) -> Vec<lookup_range::Declared<'a>> {
        let Some(drivers) = self
            .templates
            .iter()
            .map(|template| {
                if sources.iter().any(|source| source.name == template.driver) {
                    Some(template.driver.as_str())
                } else {
                    named
                        .input(&template.driver)
                        .and_then(|index| sources.get(index))
                        .map(|source| source.name.as_str())
                }
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        let evaluated =
            |declaration: &'a RowDeclaration, own: Option<&'a str>| match &declaration.operation {
                RowOperation::Shared(branch) => {
                    Some(lookup_range::Evaluated::Operation(&branch.operation, own))
                }
                RowOperation::Source(name) => Some(lookup_range::Evaluated::Name(name)),
                RowOperation::ProjectFunction { call, .. } => {
                    Some(lookup_range::Evaluated::Call(*call))
                }
                RowOperation::Output(_) => None,
            };
        // A column derivation evaluates only for the rows of the templates that
        // leave the column to this phase (LOOKUP-9), and every driver binds
        // it for its own rows. A column holding an aggregate binds once
        // against the whole dataset, where every input joins unless every
        // row template is driven by the input behind the first.
        let column_phase = self
            .columns
            .iter()
            .flat_map(|declaration| {
                let leaving = self.templates.iter().zip(&drivers).filter(|(template, _)| {
                    !template
                        .declarations
                        .iter()
                        .any(|derived| derived.column == declaration.column)
                });
                let mut scopes: Vec<(Option<&str>, Vec<&str>)> = Vec::new();
                if declaration.aggregates() {
                    let own = drivers.first().copied().filter(|&primary| {
                        self.templates
                            .iter()
                            .all(|template| template.driver == primary)
                    });
                    scopes.push((own, leaving.map(|(_, &driver)| driver).collect()));
                } else {
                    for (template, &driver) in leaving {
                        let own = Some(template.driver.as_str());
                        match scopes.iter_mut().find(|(known, _)| *known == own) {
                            Some((_, inputs)) => inputs.push(driver),
                            None => scopes.push((own, vec![driver])),
                        }
                    }
                }
                scopes.into_iter().filter_map(move |(own, inputs)| {
                    evaluated(declaration.as_ref(), own).map(|evaluated| lookup_range::Declared {
                        evaluated: vec![evaluated],
                        drivers: inputs,
                    })
                })
            })
            .collect::<Vec<_>>();
        let templates = self
            .templates
            .iter()
            .zip(&drivers)
            .map(|(template, &driver)| {
                let mut evaluated = template
                    .declarations
                    .iter()
                    .filter_map(|declaration| {
                        evaluated(declaration.as_ref(), Some(&template.driver))
                    })
                    .collect::<Vec<_>>();
                if let Some((_, Ok(filter))) = &template.filter {
                    evaluated.extend(filter.identifiers().map(lookup_range::Evaluated::Name));
                }
                lookup_range::Declared {
                    evaluated,
                    drivers: vec![driver],
                }
            });
        // A dataset verification evaluates for the rows of every template.
        column_phase
            .into_iter()
            .chain(std::iter::once(lookup_range::Declared {
                evaluated: verified.collect(),
                drivers: drivers.clone(),
            }))
            .chain(templates)
            .collect()
    }
    /// The driver of each binding [`Rows::bind`] makes of a column-phase
    /// declaration: `primary` for a column holding a qualified aggregate
    /// (LOOKUP-18), and otherwise each distinct driver of the row templates
    /// that leave the column to this phase, in construction order.
    fn column_drivers<'a>(
        &'a self,
        declaration: &RowDeclaration,
        primary: &'a str,
    ) -> Vec<&'a str> {
        if declaration.aggregates() {
            return vec![primary];
        }
        let mut drivers = Vec::new();
        for template in &self.templates {
            let leaves = !template
                .declarations
                .iter()
                .any(|derived| derived.column == declaration.column);
            if leaves && !drivers.contains(&template.driver.as_str()) {
                drivers.push(template.driver.as_str());
            }
        }
        drivers
    }
    fn declaration_findings(
        &self,
        primary: &str,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let groups = self.column_groups();
        let column_groups = groups
            .iter()
            .map(|(driver, group)| (*driver, group.as_slice()))
            .collect::<Vec<_>>();
        for declaration in &self.columns {
            if let RowOperation::Shared(branch) = &declaration.operation {
                let drivers = self.column_drivers(declaration, primary);
                static_checks::scoped_branch(
                    branch,
                    Some(static_checks::Scope {
                        drivers: &drivers,
                        row: None,
                        phase: Some(Phase::Column {
                            groups: &column_groups,
                        }),
                    }),
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
            let driver = [template.driver.as_str()];
            for declaration in &template.declarations {
                if let RowOperation::Shared(branch) = &declaration.operation {
                    static_checks::scoped_branch(
                        branch,
                        Some(static_checks::Scope {
                            drivers: &driver,
                            row: Some(&template.id),
                            phase: Some(Phase::Row {
                                group_by: groups.as_deref(),
                            }),
                        }),
                        findings,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal child-before-parent arena builder for row preparations.
    struct Arena(Vec<N>);
    impl Arena {
        fn new() -> Self {
            Self(Vec::new())
        }
        fn push(&mut self, node: N) -> usize {
            let index = self.0.len();
            self.0.push(node);
            index
        }
        fn text(&mut self, value: &str) -> usize {
            self.push(N::Text(value.into()))
        }
        fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
            let fields = fields
                .into_iter()
                .map(|(name, value)| {
                    let key = self.text(name);
                    (key, value)
                })
                .collect::<Vec<_>>();
            self.push(N::Mapping(fields))
        }
        fn seq(&mut self, items: Vec<usize>) -> usize {
            self.push(N::Sequence(items))
        }
        fn literal_derivation(&mut self, value: &str) -> usize {
            let payload = self.text(value);
            let value = self.map(vec![("literal", payload)]);
            self.map(vec![("value", value)])
        }
        fn source_derivation(&mut self, variable: &str) -> usize {
            let variable = self.text(variable);
            let payload = self.map(vec![("variable", variable)]);
            let value = self.map(vec![("source", payload)]);
            self.map(vec![("value", value)])
        }
        fn column(&mut self, name: &str, derivation: usize) -> usize {
            let name = self.text(name);
            self.map(vec![("name", name), ("derivation", derivation)])
        }
    }

    struct RowSpec {
        id: &'static str,
        driver: Option<&'static str>,
        groups: Option<Vec<&'static str>>,
        filter: Option<&'static str>,
        derivations: Vec<(&'static str, usize)>,
    }
    fn row(arena: &mut Arena, spec: RowSpec) -> usize {
        let id = arena.text(spec.id);
        let mut fields = vec![("id", id)];
        if let Some(driver) = spec.driver {
            let driver = arena.text(driver);
            fields.push(("dataset", driver));
        }
        if let Some(groups) = spec.groups {
            let groups = groups
                .into_iter()
                .map(|group| arena.text(group))
                .collect::<Vec<_>>();
            let groups = arena.seq(groups);
            fields.push(("group_by", groups));
        }
        if let Some(filter) = spec.filter {
            let filter = arena.text(filter);
            fields.push(("filter", filter));
        }
        let derivations = spec
            .derivations
            .into_iter()
            .map(|(name, derivation)| {
                let key = arena.text(name);
                (key, derivation)
            })
            .collect::<Vec<_>>();
        let derivations = arena.push(N::Mapping(derivations));
        fields.push(("derivations", derivations));
        arena.map(fields)
    }

    /// Columns: ID (source), BIG (large literal default with an
    /// unconvertible handler), NOTE (literal default). Rows: a plain SRC
    /// template overriding NOTE, an ALT-driver template overriding BIG, and
    /// a grouped SRC template overriding neither. Every template filters on
    /// the two bare defaults, so each template needs all three columns
    /// row-phase and the inherited defaults stay shared.
    fn shared_default_rows() -> Rows {
        let large = "shared-authored-default ".repeat(256);
        let mut arena = Arena::new();
        let id_default = arena.source_derivation("SRC.ID");
        let big_default = {
            let payload = arena.text(&large);
            let value = arena.map(vec![("literal", payload)]);
            let handler = arena.text("unconvertible-big");
            arena.map(vec![("value", value), ("unconvertible", handler)])
        };
        let note_default = arena.literal_derivation("note-default");
        let id_column = arena.column("ID", id_default);
        let big_column = arena.column("BIG", big_default);
        let note_column = arena.column("NOTE", note_default);
        let columns = arena.seq(vec![id_column, big_column, note_column]);

        let a_id = arena.source_derivation("SRC.ID");
        let a_note = arena.literal_derivation("note-a");
        let row_a = row(
            &mut arena,
            RowSpec {
                id: "records-a",
                driver: None,
                groups: None,
                filter: Some("BIG IS NOT NULL AND NOTE IS NOT NULL"),
                derivations: vec![("ID", a_id), ("NOTE", a_note)],
            },
        );
        let b_id = arena.source_derivation("ALT.ID");
        let b_big = arena.literal_derivation("big-override");
        let row_b = row(
            &mut arena,
            RowSpec {
                id: "records-b",
                driver: Some("ALT"),
                groups: None,
                filter: Some("BIG IS NOT NULL AND NOTE IS NOT NULL"),
                derivations: vec![("ID", b_id), ("BIG", b_big)],
            },
        );
        let c_id = arena.source_derivation("SRC.ID");
        let row_c = row(
            &mut arena,
            RowSpec {
                id: "grouped",
                driver: None,
                groups: Some(vec!["ID"]),
                filter: Some("BIG IS NOT NULL AND NOTE IS NOT NULL"),
                derivations: vec![("ID", c_id)],
            },
        );
        let rows = arena.seq(vec![row_a, row_b, row_c]);

        let src_path = arena.text("src.csv");
        let src = arena.map(vec![("path", src_path)]);
        let alt_path = arena.text("alt.csv");
        let alt = arena.map(vec![("path", alt_path)]);
        let input = arena.map(vec![("SRC", src), ("ALT", alt)]);
        let key = arena.text("ID");
        let keys = arena.seq(vec![key]);
        let root = arena.map(vec![
            ("input", input),
            ("keys", keys),
            ("columns", columns),
            ("rows", rows),
        ]);
        let document = Document::new(arena.0, root).unwrap();
        let output = TableSchema::new(vec![
            Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "BIG".into(),
                kind: ColumnType::Str,
            },
            Column {
                name: "NOTE".into(),
                kind: ColumnType::Str,
            },
        ])
        .unwrap();
        let mut findings = Vec::new();
        let declared = types::DeclaredTypes::default();
        let extraction = str_extract::Prepared::new(&document, &declared, &mut findings).unwrap();
        let string_templates =
            str_template::Prepared::new(&document, &output, &declared, &mut findings).unwrap();
        assert!(findings.is_empty());
        let intermediates = intermediates::Declarations::default();
        Rows::prepare(
            &document,
            &output,
            "SRC",
            None,
            false,
            &extraction,
            &string_templates,
            &intermediates,
        )
        .unwrap()
    }

    fn declaration(rows: &Rows, template: usize, column: usize) -> &SharedDeclaration {
        rows.templates[template]
            .declarations
            .iter()
            .find(|declaration| declaration.column == column)
            .unwrap()
    }
    fn literal_payload(declaration: &RowDeclaration) -> &str {
        let RowOperation::Shared(branch) = &declaration.operation else {
            panic!("literal default lowers to a shared branch");
        };
        let Operation::Literal(crate::value::Value::Str(value)) = branch.operation.as_ref() else {
            panic!("literal default payload");
        };
        value
    }

    #[test]
    fn admitted_defaults_are_shared_once_across_templates() {
        let rows = shared_default_rows();
        assert_eq!(rows.templates.len(), 3);
        // BIG (column 1): templates 0 and 2 inherit the authored default and
        // hold the very same immutable declaration and literal payload.
        let a_big = declaration(&rows, 0, 1);
        let c_big = declaration(&rows, 2, 1);
        assert!(std::sync::Arc::ptr_eq(a_big, c_big));
        assert_eq!(a_big.path, "columns.BIG.derivation.literal");
        assert!(std::ptr::eq(literal_payload(a_big), literal_payload(c_big)));
        assert_eq!(literal_payload(a_big).len(), 6144);
        // The conversion handler rides the shared declaration, not a copy.
        let handler = a_big.handler.as_ref().unwrap();
        assert_eq!(handler.spec_path, "columns.BIG.derivation.unconvertible");
        assert!(std::ptr::eq(handler, c_big.handler.as_ref().unwrap()));
        // NOTE (column 2): templates 1 and 2 share the default.
        let b_note = declaration(&rows, 1, 2);
        let c_note = declaration(&rows, 2, 2);
        assert!(std::sync::Arc::ptr_eq(b_note, c_note));
        assert_eq!(literal_payload(b_note), "note-default");
    }

    #[test]
    fn authored_overrides_retain_their_own_payloads() {
        let rows = shared_default_rows();
        // Template 1 overrides BIG: its own declaration, path and payload.
        let b_big = declaration(&rows, 1, 1);
        assert!(!std::sync::Arc::ptr_eq(b_big, declaration(&rows, 0, 1)));
        assert_eq!(b_big.path, "rows[1].derivations.BIG.literal");
        assert_eq!(literal_payload(b_big), "big-override");
        assert!(b_big.handler.is_none());
        // Template 0 overrides NOTE likewise.
        let a_note = declaration(&rows, 0, 2);
        assert!(!std::sync::Arc::ptr_eq(a_note, declaration(&rows, 1, 2)));
        assert_eq!(a_note.path, "rows[0].derivations.NOTE.literal");
        assert_eq!(literal_payload(a_note), "note-a");
        // Every template still carries every selected column exactly once.
        for template in &rows.templates {
            let mut columns = template
                .declarations
                .iter()
                .map(|declaration| declaration.column)
                .collect::<Vec<_>>();
            columns.sort_unstable();
            assert_eq!(columns, [0, 1, 2]);
        }
        assert_eq!(rows.self_columns(), [0, 1, 2]);
    }

    #[test]
    fn template_scope_stays_outside_the_shared_declaration() {
        let rows = shared_default_rows();
        // Driver and grouping remain per-template state even where the
        // declarations themselves are shared.
        assert_eq!(rows.templates[0].driver, "SRC");
        assert_eq!(rows.templates[1].driver, "ALT");
        assert_eq!(rows.templates[2].driver, "SRC");
        assert_eq!(rows.templates[0].groups, None);
        assert_eq!(rows.templates[2].groups, Some(vec!["ID".to_string()]));
        // A repeated preparation of the same document admits the same
        // per-template structure (sharing is per-preparation, never a
        // cross-run cache of bound or prepared state).
        let repeated = shared_default_rows();
        for (template, again) in rows.templates.iter().zip(&repeated.templates) {
            assert_eq!(template.id, again.id);
            assert_eq!(template.driver, again.driver);
            assert_eq!(template.groups, again.groups);
            for (declaration, again) in template.declarations.iter().zip(&again.declarations) {
                assert_eq!(declaration.column, again.column);
                assert_eq!(declaration.path, again.path);
                assert!(!std::sync::Arc::ptr_eq(declaration, again));
            }
        }
    }
}
