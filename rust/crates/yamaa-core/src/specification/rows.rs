//! Closed row lowering with per-template defaults and shared group-scope validation.
//!
//! Preparation admits each column default once, keyed by its expression in
//! the tree, and shares the immutable [`RowDeclaration`] across every row
//! template that inherits it; authored row overrides keep their own
//! declarations. Sharing covers only the immutable declaration: its handler,
//! column, path and expression. Driver-specific admission still
//! runs for each template, and binding still runs per template against its
//! own driver, catalog, grouping phase and available outputs, so no bound
//! assignment is ever cached by declaration identity alone.
use super::*;
use crate::dataset::{ColumnBinding, ColumnScope, Verification};
use crate::{
    bound_expression::{Binding, BoundPredicate, Read},
    reference_scope::Phase,
};

/// One derivation a row template or the column phase evaluates: the column
/// it derives, its expression, the path of its operation and its answer for
/// a value its column type cannot hold.
#[derive(Debug)]
struct RowDeclaration {
    handler: Option<crate::conversion::LiteralHandler>,
    column: usize,
    path: String,
    expr: ExprId,
}
/// One admitted derivation, shared immutably across templates by its
/// expression: a column default every template that inherits it evaluates,
/// or a template's own override. Everything template-specific (driver and
/// catalog bindings, grouping phase, available outputs, row diagnostics and
/// per-run state) lives outside it.
type SharedDeclaration = std::sync::Arc<RowDeclaration>;
#[derive(Debug)]
struct Template {
    id: String,
    /// The template's own path, `rows.<id>` (P-CLR-2).
    path: String,
    driver: String,
    groups: Option<Vec<String>>,
    declarations: Vec<SharedDeclaration>,
    filter: Option<ExprId>,
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
/// How a spec's rows are built: the one implicit template of a spec without
/// `rows`, or its row templates. Binding takes one route for both
/// ([`Rows::bind`]), and every derivation binds through one
/// [`BindContext`].
#[derive(Debug)]
pub(super) enum Rows {
    Implicit(Implicit),
    Templates(Templates),
}
/// A spec without `rows` builds one implicit template over its base driver
/// (`yaml/schema_spec.yaml`'s root `filter` is that template's filter). Its
/// rows are its keys' distinct combinations of the retained base records, or
/// the records themselves when a window derives a key. Every column binds
/// in declaration order; a key is computed from one base record and is the
/// template's row-phase assignment, and every other column completes the
/// rows in the column phase.
#[derive(Debug)]
pub(super) struct Implicit {
    /// Each column's derivation, in declaration order.
    declarations: Vec<RowDeclaration>,
    /// The root filter, which reads only the base driver (ROW-9).
    filter: Option<ExprId>,
    /// Whether every row is one base record: a window derives a key.
    records: bool,
}
/// The row templates, and the derivations the column phase completes.
#[derive(Debug)]
pub(super) struct Templates {
    templates: Vec<Template>,
    columns: Vec<SharedDeclaration>,
}

/// What binding the row templates reads, built once where they bind
/// ([`Templates::bind`]) and borrowed by each template, the column phase and
/// the reads they admit: the prepared specification (its tree, output, keys
/// and lookup declarations), the captured inputs, and the lookups bound over
/// them.
#[derive(Clone, Copy)]
struct Context<'a> {
    prepared: &'a PreparedSpecification,
    inputs: binder::Inputs<'a>,
    donors: binder::Donors<'a>,
}

impl<'a> Context<'a> {
    /// The base driver's layout.
    fn source(&self) -> &'a TableSchema {
        self.inputs.driver_schema().layout()
    }
    /// The base driver's name.
    fn name(&self) -> &'a str {
        &self.inputs.sources[self.inputs.driver].name
    }
}

impl Rows {
    /// Bind every derivation over the rows, after the intermediates: the
    /// implicit template's columns in declaration order, or each row
    /// template's derivations and then the column phase. The inputs follow
    /// authored order.
    pub(super) fn bind(
        &self,
        prepared: &PreparedSpecification,
        inputs: binder::Inputs<'_>,
    ) -> Result<DatasetPlan, BindError> {
        let mut findings = Vec::new();
        let self_columns = match self {
            Rows::Implicit(_) => None,
            Rows::Templates(templates) => Some(templates.self_columns()),
        };
        let intermediates::Bound {
            secondary,
            items: intermediates,
            withheld,
        } = prepared.intermediates.bind(
            inputs,
            &prepared.output,
            self_columns.as_deref(),
            prepared.project_calls.as_ref(),
            &prepared.tree,
            &mut findings,
        )?;
        let plan = match self {
            Rows::Implicit(implicit) => {
                implicit.bind(prepared, inputs, secondary, intermediates, findings)
            }
            Rows::Templates(templates) => {
                let verifications =
                    prepared
                        .verifications
                        .bind(prepared, &secondary, &intermediates)?;
                templates.bind(
                    prepared,
                    inputs,
                    &verifications,
                    secondary,
                    intermediates,
                    findings,
                )
            }
        }
        .map_err(|error| match error {
            // Every read of a lookup withheld from `check()` is withheld too.
            BindError::Invalid(mut findings) => {
                intermediates::withhold_reads(&mut findings, &withheld);
                BindError::Invalid(findings)
            }
            error => error,
        })?;
        Ok(plan
            .with_column_verifications(prepared.column_verification_groups())
            .with_warning_verifications(prepared.warning_verification_paths())
            .with_output_order(prepared.bound_output_order())
            .with_required_matches(prepared.required_matches.iter().cloned()))
    }
}

impl Implicit {
    /// The implicit template of `spec`: each column's derivation in
    /// declaration order, admitted before any source is read, and the root
    /// filter. A derivation binding does not support is collected in `extra`.
    pub(super) fn prepare(
        spec: &ast::Spec,
        keys: &[usize],
        admission: &derivations::Admission<'_>,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        let mut declarations = Vec::new();
        for (column, declared) in spec.columns.iter().enumerate() {
            let derivation = declared.derivation.as_ref().ok_or(PrepareError::Internal)?;
            let handler = literal_handler(spec, derivation)?;
            match admission.admit(spec, derivation.expr, 0) {
                Ok(()) => {}
                Err(PrepareError::Unsupported(features)) => extra.extend(features),
                Err(error) => return Err(error),
            }
            declarations.push(RowDeclaration {
                handler,
                column,
                path: checks::path(spec, derivation.expr),
                expr: derivation.expr,
            });
        }
        // A window that derives a key gives each retained base record its own
        // row: the key table is the records themselves, not their distinct
        // key combinations.
        let records = keys.iter().any(|&key| {
            declarations.get(key).is_some_and(|declaration| {
                matches!(spec.expr(declaration.expr).kind, ExprKind::Window(_))
            })
        });
        Ok(Self {
            declarations,
            filter: spec.rows.filter,
            records,
        })
    }
    /// Whether every row is one base record: a window derives a key, so the
    /// key table is the records themselves, not their distinct key
    /// combinations.
    pub(super) fn record_rows(&self) -> bool {
        self.records
    }
    /// The scope the derivation of `column` binds in: the base driver's
    /// rows, in the column phase without grouping; a key reads one base
    /// record (COLUMN-7).
    pub(super) fn scope<'a>(
        &self,
        keys: &'a [usize],
        base: &'a str,
        column: usize,
    ) -> binder::Scope<'a> {
        binder::Scope {
            template: binder::Template::Implicit(binder::Implicit {
                key: keys.contains(&column),
                records: self.records,
            }),
            phase: Phase::Column { groups: &[] },
            driver: base,
            available: None,
        }
    }
    /// Bind each column in declaration order, then order the columns
    /// (COLUMN-5, COLUMN-6, COLUMN-7), bind the root filter and the dataset
    /// verifications, and admit what every row reads from its lookups.
    fn bind(
        &self,
        prepared: &PreparedSpecification,
        inputs: binder::Inputs<'_>,
        secondary: Vec<crate::dataset::SecondarySource>,
        intermediates: Vec<Option<crate::dataset::Intermediate>>,
        mut findings: Vec<BindFinding>,
    ) -> Result<DatasetPlan, BindError> {
        let source = inputs.driver_schema().layout();
        let base = prepared.source().name.as_str();
        let keys = &prepared.keys;
        let donors = binder::Donors {
            named: &prepared.intermediates,
            secondary: &secondary,
            items: &intermediates,
            inputs,
        };
        let catalog = row_drivers::joined_catalog(
            prepared.tree.spec(),
            &prepared.output,
            source,
            base,
            donors,
        )?;
        let mut assignments = BTreeMap::new();
        let mut dependencies = Vec::new();
        for (column, declaration) in self.declarations.iter().enumerate() {
            // A key that reads other records than its one base record has no
            // key-phase value to bind (COLUMN-7).
            let refused = key_record(prepared, column);
            if !refused.is_empty() {
                findings.extend(refused);
                dependencies.push(Some(Vec::new()));
                continue;
            }
            let spec = prepared.tree.spec();
            let reported = cases::padding_findings(spec, declaration.expr, &mut findings)?;
            let mut edges = Vec::new();
            let start = findings.len();
            let expression = BindContext {
                scope: self.scope(keys, base, column),
                donors,
                catalog: &catalog,
                output: &prepared.output,
                project: prepared.project_calls.as_ref(),
                tree: &prepared.tree,
                edges: &mut edges,
                findings: &mut findings,
                reported,
            }
            .bind(declaration.expr, column)?;
            if expression.is_some() && findings.len() == start && declaration.handler.is_none() {
                let captured = types::CapturedTypes::new(donors, &catalog);
                let result =
                    prepared
                        .tree
                        .result_type(declaration.expr, ast::Scope::Row, &captured);
                findings.extend(
                    prepared
                        .conversion_finding(column, result)
                        .map(BindFinding::Scalar),
                );
            }
            // Invalid declarations remain in the graph so independent cycles and
            // key dependencies are still reported after the reference findings.
            dependencies.push(Some(edges));
            if let Some(expression) = expression {
                assignments.insert(
                    column,
                    Assignment {
                        column,
                        expression,
                        path: declaration.path.clone(),
                    },
                );
            }
        }
        let programs = (0..dependencies.len())
            .map(|column| assignments.get(&column))
            .collect::<Vec<_>>();
        let (analysis, recurrences) =
            crate::dataset::recurrence::analyze(&dependencies, &programs, keys, false)
                .map_err(BindError::DependencyPolicy)?;
        if !analysis.diagnostics.is_empty() {
            findings.push(BindFinding::Dependencies {
                columns: prepared
                    .output
                    .columns()
                    .iter()
                    .map(|c| c.name.clone())
                    .collect(),
                paths: self.declarations.iter().map(|d| d.path.clone()).collect(),
                diagnostics: analysis.diagnostics,
            });
        }
        let filter = self
            .filter
            .map(|filter| {
                root_filter::bind(
                    prepared.tree.spec(),
                    filter,
                    base,
                    inputs.driver_schema(),
                    &mut findings,
                )
            })
            .transpose()?
            .flatten();
        // Every row is built from the base driver, the primary port. A dataset
        // verification reads its intermediates for every row, as a column does.
        let verifications = prepared
            .verifications
            .bind(prepared, &secondary, &intermediates)?;
        let verification_reads = relation_predicates::predicate_reads(
            relation_predicates::verification_predicates(&verifications),
        );
        let contexts = [(
            assignments
                .values()
                .map(|assignment| &assignment.expression)
                .chain(&verification_reads)
                .collect::<Vec<_>>(),
            vec![lookup_range::RowDriver::BASE],
        )];
        lookup_range::admit(
            contexts.iter().cloned(),
            &intermediates,
            &prepared.intermediates,
            &mut findings,
        );
        aggregates::admit_record_keys(&contexts, &mut findings);
        relation_predicates::admit_reads(
            assignments
                .values()
                .map(|assignment| &assignment.expression)
                .chain(&verification_reads),
            &intermediates,
            &prepared.intermediates,
            0,
            &RowMode::Keys,
            None,
            &mut findings,
        );
        if !findings.is_empty() {
            return Err(BindError::Invalid(findings));
        }
        let mut key_assignments = Vec::new();
        let mut columns = Vec::new();
        for column in analysis.order {
            let assignment = assignments.remove(&column).expect("analyzed declaration");
            if keys.contains(&column) {
                key_assignments.push(assignment)
            } else {
                columns.push(assignment)
            }
        }
        let record_windows = key_assignments
            .iter()
            .any(|a| a.expression.window().is_some());
        Ok(DatasetPlan::new_with_intermediates(
            crate::dataset::SourceSchemas {
                primary: source.clone(),
                secondary,
            },
            intermediates
                .into_iter()
                .map(|item| item.expect("validated intermediate"))
                .collect(),
            prepared.output.clone(),
            vec![RowTemplate {
                mode: if record_windows {
                    RowMode::Records
                } else {
                    RowMode::Keys
                },
                assignments: key_assignments,
                filter,
            }],
            columns,
            keys.clone(),
            verifications,
        )
        .with_recurrences(recurrences)
        .with_conversion_handlers(
            self.declarations
                .iter()
                .filter_map(|declaration| {
                    declaration
                        .handler
                        .as_ref()
                        .map(|handler| crate::dataset::ConversionHandler {
                            assignment_path: declaration.path.clone(),
                            handler: handler.clone(),
                        })
                })
                .collect(),
        ))
    }
}
/// Admit one row-phase or column-phase derivation of `column`: a project
/// call reads its admitted call, a lone variable needs nothing more, and
/// any other derivation is admitted as every derivation is.
fn declaration(
    spec: &ast::Spec,
    derivation: &ast::Derivation,
    column: usize,
    admission: &derivations::Admission<'_>,
) -> Result<RowDeclaration, PrepareError> {
    let handler = literal_handler(spec, derivation)?;
    let expr = derivation.expr;
    match &spec.expr(expr).kind {
        ExprKind::Call { .. } => {
            // The tree types the call by the return its definition declares.
            let project = admission.project.ok_or_else(|| {
                PrepareError::Unsupported(vec![UnsupportedFeature {
                    operation: crate::expression_table::Verb::Function.name().into(),
                    path: checks::path(spec, expr),
                }])
            })?;
            project.call(expr).ok_or(PrepareError::Internal)?;
        }
        ExprKind::Ref(_) => {}
        _ => admission.admit(spec, expr, 0)?,
    }
    Ok(RowDeclaration {
        handler,
        column,
        path: checks::path(spec, expr),
        expr,
    })
}
/// Admit a derivation at most once per preparation, keyed by its
/// expression: one expression has one owning column and one path, so every
/// template that reaches the same default shares one immutable declaration.
/// Overrides are distinct expressions and keep their own declarations.
/// Grouping, driver admission and binding remain per-template work.
fn lower_shared(
    cache: &mut BTreeMap<ExprId, SharedDeclaration>,
    spec: &ast::Spec,
    derivation: &ast::Derivation,
    column: usize,
    admission: &derivations::Admission<'_>,
) -> Result<SharedDeclaration, PrepareError> {
    if let Some(shared) = cache.get(&derivation.expr) {
        return Ok(shared.clone());
    }
    let shared = SharedDeclaration::new(declaration(spec, derivation, column, admission)?);
    cache.insert(derivation.expr, shared.clone());
    Ok(shared)
}
/// Validate every row-phase use before projecting its authored context.
fn validate_row_phases(
    spec: &ast::Spec,
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
            match &spec.expr(declaration.expr).kind {
                ExprKind::Ref(identifier) => register(
                    Site {
                        template,
                        declaration,
                        identifier: identifier.as_str(),
                        function: None,
                    },
                    output,
                    &mut sites,
                ),
                ExprKind::Call { .. } => {
                    let project = project.ok_or(PrepareError::Internal)?;
                    let call = &project
                        .calls
                        .calls()
                        .get(
                            project
                                .call(declaration.expr)
                                .ok_or(PrepareError::Internal)?,
                        )
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
pub(super) fn distinct_findings(
    findings: &mut Vec<BindFinding>,
    start: usize,
    source: &SourceDeclaration,
) {
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

/// Bind one row declaration in its context: a call, a read of a variable or
/// an operation. A derivation that bound without a finding states its result
/// type once per column (VALUE-24): a default shared by several templates
/// states one derivation.
fn bind_assignment(
    cx: &mut BindContext<'_, '_>,
    declaration: &RowDeclaration,
) -> Result<Option<Assignment>, BindError> {
    let start = cx.findings.len();
    let spec = cx.spec();
    let expression = match &spec.expr(declaration.expr).kind {
        ExprKind::Call { .. } => cx.call(cx.tree.call(declaration.expr)?)?,
        ExprKind::Ref(reference) => cx.operand(reference.as_str(), &declaration.path)?,
        _ => cx.bind(declaration.expr, declaration.column)?,
    };
    if expression.is_some() && cx.findings.len() == start && declaration.handler.is_none() {
        let result = cx
            .tree
            .result_type(declaration.expr, ast::Scope::Row, &cx.captured());
        let column = &cx.output.columns()[declaration.column];
        if let Some(finding) =
            types::conversion_finding(&format!("columns.{}", column.name), result, column.kind)
        {
            if !cx
                .findings
                .iter()
                .any(|prior| matches!(prior, BindFinding::Scalar(prior) if *prior == finding))
            {
                cx.findings.push(BindFinding::Scalar(finding));
            }
        }
    }
    Ok(expression.map(|expression| Assignment {
        column: declaration.column,
        expression,
        path: declaration.path.clone(),
    }))
}

/// Bind a row template's filter in the template's row phase (`cx`, whose
/// edges no column keeps): a read of a lookup, of another input through its
/// row join, of the driver or of a column the template derives in the row
/// phase; then whether a read waits for a window the row filter cannot
/// (`row_filter_phase_boundary`).
fn bind_template_filter(
    cx: &mut BindContext<'_, '_>,
    predicate: Option<&crate::expression::CompiledExpression>,
    grouped: bool,
    template: &Template,
    filter_path: &str,
    row_source: &TableSchema,
    assignments: &[Assignment],
) -> Result<Option<BoundPredicate>, BindError> {
    let binder::Donors {
        named,
        items: intermediates,
        ..
    } = cx.donors;
    let (row_catalog, output) = (cx.catalog, cx.output);
    let mut bindings = Vec::new();
    let before_filter = cx.findings.len();
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
                    if let Some(Expression::Intermediate { index, column }) =
                        row_intermediates::bind(cx, identifier, filter_path)?
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
                        cx.scope.available,
                        &mut Vec::new(),
                        cx.findings,
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
                        cx.findings,
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
                cx.findings.push(BindFinding::RowFilterPhase {
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
    let filter = if cx.findings.len() != before_filter
        || grouped
            && predicate.is_some_and(|plan| plan.identifiers().any(|name| name.contains('.')))
    {
        None
    } else {
        predicate
            .map(|plan| {
                BoundPredicate::new_contextual(
                    types::CapturedTypes::new(cx.donors, row_catalog).typed(plan),
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
                cx.findings
                    .push(BindFinding::Scalar(crate::diagnostic::Diagnostic {
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

/// Bind one row template: its driver, its `group_by`, each derivation it
/// evaluates in the row phase in dependency order, and its filter.
fn bind_template(
    cx: &Context<'_>,
    template: &Template,
    declared: &TemplatePhase<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RowTemplate>, BindError> {
    let (source, name) = (cx.source(), cx.name());
    let (tree, output) = (&cx.prepared.tree, &cx.prepared.output);
    let project = cx.prepared.project_calls.as_ref();
    let binder::Donors {
        named,
        secondary,
        items: intermediates,
        ..
    } = cx.donors;
    let filter_path = format!("{}.filter", template.path);
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
            &format!("{}.dataset", template.path),
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
        cx.donors,
    )?;
    let row_catalog = row_drivers::extend_catalog(
        tree.spec(),
        row_catalog,
        source,
        name,
        cx.donors,
        &template.driver,
    )?;
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
                        &format!("{}.group_by[{position}]", template.path),
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
                                &format!("{}.group_by[{position}]", template.path),
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
    let spec = tree.spec();
    let predicate = match template.filter.map(|filter| &spec.expr(filter).kind) {
        Some(ExprKind::Formula(formula)) => match &formula.compiled {
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Lookup(LookupFinding::grammar(
                    &filter_path,
                    &formula.text.text,
                    position.character,
                    failure,
                )));
                None
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
            Ok(plan) => Some(plan),
        },
        Some(_) => return Err(BindError::Internal),
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
    let mut graph = std::vec![Vec::new(); output.columns().len()];
    let mut assignments = BTreeMap::new();
    // Reference planning validates one complete row template's authored
    // derivations before resolving any of that template's references.
    let mut declaration_findings = Vec::new();
    let mut declarations_reported = false;
    for declaration in &template.declarations {
        declarations_reported |=
            cases::padding_findings(spec, declaration.expr, &mut declaration_findings)?;
    }
    if declarations_reported {
        for declaration in &template.declarations {
            derivations::declaration_findings(spec, declaration.expr, findings)?;
        }
    }
    let scope = declared.scope();
    let donors = cx.donors;
    for declaration in &template.declarations {
        let mut cx = BindContext {
            scope,
            donors,
            catalog: &row_catalog,
            output,
            project,
            tree,
            edges: &mut graph[declaration.column],
            findings: &mut *findings,
            reported: declarations_reported,
        };
        if let Some(value) = bind_assignment(&mut cx, declaration)? {
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
    // The filter reads in the template's row phase; no column keeps its edges.
    let mut edges = Vec::new();
    let filter = bind_template_filter(
        &mut BindContext {
            scope,
            donors,
            catalog: &row_catalog,
            output,
            project,
            tree,
            edges: &mut edges,
            findings,
            reported: false,
        },
        predicate,
        grouped,
        template,
        &filter_path,
        row_source,
        &assignments,
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

/// One row template as binding reads it: its id, driver, `group_by` and the
/// columns it derives in the row phase, in construction order. Its
/// [`scope`](Self::scope) is what those derivations bind in, decided by the
/// declarations alone.
pub(super) struct TemplatePhase<'a> {
    pub id: &'a str,
    pub driver: &'a str,
    pub groups: Option<Vec<&'a str>>,
    /// The columns the template derives during row construction, which its
    /// row-phase derivations may read.
    pub available: Vec<usize>,
}

impl TemplatePhase<'_> {
    /// The scope the template's row-phase derivations bind in.
    pub(super) fn scope(&self) -> binder::Scope<'_> {
        binder::Scope {
            template: binder::Template::Row(self.id),
            phase: Phase::Row {
                group_by: self.groups.as_deref(),
            },
            driver: self.driver,
            available: Some(&self.available),
        }
    }
}

/// One binding of a column-phase derivation (#2829): whose records its
/// driver reads address, that driver, and the row templates whose rows it
/// completes, ascending.
pub(super) struct ColumnPhaseBinding<'a> {
    pub scope: ColumnScope,
    pub driver: &'a str,
    pub templates: Vec<usize>,
}

impl Templates {
    /// Each row template and its row-phase derivations, in construction order.
    pub(super) fn phases(&self) -> Vec<TemplatePhase<'_>> {
        self.templates
            .iter()
            .map(|template| TemplatePhase {
                id: &template.id,
                driver: &template.driver,
                groups: template
                    .groups
                    .as_ref()
                    .map(|groups| groups.iter().map(String::as_str).collect()),
                available: template
                    .declarations
                    .iter()
                    .map(|declaration| declaration.column)
                    .collect(),
            })
            .collect()
    }
    /// The bindings a column-phase derivation of `column` gets, from the
    /// declarations alone: one over the whole dataset through `primary` when
    /// it holds a qualified aggregate, which reduces a declared input through
    /// the primary port (LOOKUP-18); otherwise one per distinct driver of the
    /// row templates that leave the column to this phase, in construction
    /// order. Its qualifier reads the record that built each row (LOOKUP-1),
    /// another input joins on the output keys it carries, and a named lookup
    /// matches through its key.
    pub(super) fn column_bindings<'a>(
        &'a self,
        column: usize,
        aggregates: bool,
        primary: &'a str,
    ) -> Vec<ColumnPhaseBinding<'a>> {
        let leaving = self
            .templates
            .iter()
            .enumerate()
            .filter(|(_, template)| {
                !template
                    .declarations
                    .iter()
                    .any(|derived| derived.column == column)
            })
            .map(|(index, _)| index);
        if aggregates {
            return vec![ColumnPhaseBinding {
                scope: ColumnScope::Dataset,
                driver: primary,
                templates: leaving.collect(),
            }];
        }
        let mut bindings: Vec<ColumnPhaseBinding<'a>> = Vec::new();
        for index in leaving {
            let driver = self.templates[index].driver.as_str();
            match bindings.iter_mut().find(|binding| binding.driver == driver) {
                Some(binding) => binding.templates.push(index),
                None => bindings.push(ColumnPhaseBinding {
                    scope: ColumnScope::Template,
                    driver,
                    templates: vec![index],
                }),
            }
        }
        bindings
    }
}

impl Templates {
    /// Each row template's path and driver name, in construction order.
    pub(super) fn constructions(&self) -> impl Iterator<Item = (&str, &str)> {
        self.templates
            .iter()
            .map(|template| (template.path.as_str(), template.driver.as_str()))
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
    /// The row templates of `spec`, each with the derivations it evaluates
    /// in the row phase, and the derivations the column phase completes,
    /// admitted before any source is read. Every selected default is
    /// admitted once and shared by every template that inherits it.
    pub(super) fn prepare(
        spec: &ast::Spec,
        output: &TableSchema,
        driver: &str,
        admission: &derivations::Admission<'_>,
    ) -> Result<Self, PrepareError> {
        if spec.rows.templates.is_empty() {
            return Err(PrepareError::Internal);
        }
        for template in &spec.rows.templates {
            derivations::predicate_limits(spec, template.filter)?;
        }
        let selected = super::phases::row_phases(spec, driver)?;
        let calls = admission.project.is_some();
        let mut templates = Vec::new();
        // Admission lowers every selected default once and keeps it: the
        // per-template pass below inherits these shared declarations
        // instead of admitting the same default again per template.
        let mut lowered: BTreeMap<ExprId, SharedDeclaration> = BTreeMap::new();
        {
            let mut union: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
            for template_selected in &selected {
                union.extend(template_selected.iter().copied());
            }
            for column in union {
                if let Some(derivation) = &spec.columns[column].derivation {
                    lower_shared(&mut lowered, spec, derivation, column, admission)?;
                }
            }
        }
        for (index, row) in spec.rows.templates.iter().enumerate() {
            let row_driver = row
                .driver
                .as_ref()
                .map_or(driver, |driver| driver.name.as_str());
            let groups = row.group_by.as_ref().map(|groups| {
                groups
                    .iter()
                    .filter_map(|&group| checks::name(spec, group).map(String::from))
                    .collect::<Vec<_>>()
            });
            for (name, _) in &row.overrides {
                if !output.columns().iter().any(|c| c.name == name.name) {
                    // Authored preflight already owns the undeclared-column
                    // diagnostic; this is only a normalized-plan invariant.
                    return Err(PrepareError::Internal);
                }
            }
            let mut declarations = Vec::new();
            for &column in &selected[index] {
                let metadata = &output.columns()[column];
                let derivation = match row
                    .overrides
                    .iter()
                    .find(|(name, _)| name.name == metadata.name)
                {
                    Some((_, derivation)) => derivation,
                    None => spec.columns[column]
                        .derivation
                        .as_ref()
                        .ok_or(PrepareError::Internal)?,
                };
                declarations.push(lower_shared(
                    &mut lowered,
                    spec,
                    derivation,
                    column,
                    admission,
                )?);
            }
            templates.push(Template {
                id: row.id.name.clone(),
                path: checks::render(spec, row.meta),
                driver: row_driver.into(),
                groups,
                declarations,
                filter: row.filter,
            });
        }
        // The first template's SELF reads, every alternative included.
        let roots = templates[0]
            .declarations
            .iter()
            .map(|declaration| declaration.expr)
            .collect::<Vec<_>>();
        let filter = spec.rows.templates[0]
            .filter
            .filter(|_| templates[0].groups.is_none());
        let self_findings =
            checks::lookups::first_template(spec, &roots, filter, calls, admission.intermediates)?;
        if !self_findings.is_empty() {
            return Err(PrepareError::Invalid(self_findings));
        }
        validate_row_phases(spec, &templates, admission.project, output)?;
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
        let mut records = drivers.clone();
        for &name in &drivers {
            if let Some(index) = admission.intermediates.input(name) {
                let input = spec
                    .inputs
                    .get(index)
                    .ok_or(PrepareError::Internal)?
                    .name
                    .name
                    .as_str();
                if !records.contains(&input) {
                    records.push(input);
                }
            }
        }
        let mut window_drivers = Vec::new();
        let mut lowered_columns = Vec::new();
        let mut unaddressable = Vec::new();
        for (column, declared) in spec.columns.iter().enumerate() {
            // Third state: a default that is row-phase for some templates
            // stays column-phase for the rest. Columns selected in every
            // template need no column assignment; columns missing from at
            // least one template are completed column-phase for the rows
            // that did not derive them up front.
            if selected.iter().all(|template| template.contains(&column)) {
                continue;
            }
            // Authored preflight already retains missing defaults and row
            // derivations before lowering any template.
            let derivation = declared.derivation.as_ref().ok_or(PrepareError::Internal)?;
            let declaration = lower_shared(&mut lowered, spec, derivation, column, admission)?;
            windows::column_driver_findings(
                spec,
                declaration.expr,
                &drivers,
                &records,
                &mut window_drivers,
            )?;
            // Every driver binds the column for its own rows (#2829); a
            // named intermediate's unaddressable record is refused here.
            let mut needing: Vec<&str> = Vec::new();
            for (template, selected) in templates.iter().zip(&selected) {
                if !selected.contains(&column) && !needing.contains(&template.driver.as_str()) {
                    needing.push(&template.driver);
                }
            }
            for driver in needing {
                if admission.intermediates.contains(driver)
                    && checks::lookups::unaddressable(
                        spec,
                        declaration.expr,
                        driver,
                        checks::lookups::aggregates(spec, declaration.expr),
                        calls,
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
    /// The root of each row declaration: once for each template that
    /// evaluates it at the row phase, and once if the column phase
    /// completes it.
    pub(super) fn declaration_roots(&self) -> impl Iterator<Item = ExprId> + '_ {
        self.templates
            .iter()
            .flat_map(|template| &template.declarations)
            .chain(&self.columns)
            .map(|declaration| declaration.expr)
    }

    /// Bind each row template's derivations and filter, then the column
    /// phase ([`Self::bind_column_phase`]), admit what each template's rows
    /// read of the lookups ([`admit_reads`]), and build the plan.
    pub(super) fn bind(
        &self,
        prepared: &PreparedSpecification,
        inputs: binder::Inputs<'_>,
        verifications: &[Verification],
        secondary: Vec<crate::dataset::SecondarySource>,
        intermediates: Vec<Option<crate::dataset::Intermediate>>,
        mut findings: Vec<BindFinding>,
    ) -> Result<DatasetPlan, BindError> {
        let cx = Context {
            prepared,
            inputs,
            donors: binder::Donors {
                named: &prepared.intermediates,
                secondary: &secondary,
                items: &intermediates,
                inputs,
            },
        };
        let (source, name, output) = (cx.source(), cx.name(), &prepared.output);
        // The whole-dataset catalog binds a column holding a qualified
        // aggregate; every other column binds against each driver's own.
        let spec = prepared.tree.spec();
        let mut catalog = row_drivers::joined_catalog(spec, output, source, name, cx.donors)?;
        if self
            .templates
            .iter()
            .any(|template| template.driver != name)
        {
            // The rows have no record owner shared by every template. Every
            // stored source uses its captured port and its implicit join's key.
            catalog = row_drivers::extend_catalog(spec, catalog, source, name, cx.donors, "")?;
        }
        // Each template's scope and the column phase's grouping come from the
        // declarations alone, as `check()` reads them: the column phase checks
        // a read of a driver against the groups of that driver's templates
        // (NAME-4).
        let phases = self.phases();
        let mut templates = Vec::new();
        // The declared template behind each bound one, by position, and the
        // bound position of each declared template.
        let mut declared = Vec::new();
        let mut positions = Vec::new();
        for (index, template) in self.templates.iter().enumerate() {
            let bound = bind_template(&cx, template, &phases[index], &mut findings)?;
            positions.push(bound.as_ref().map(|_| templates.len()));
            if let Some(bound) = bound {
                templates.push(bound);
                declared.push(template);
            }
        }
        let (columns, recurrences) = self.bind_column_phase(
            &cx,
            &catalog,
            &phases,
            &templates,
            &positions,
            &mut findings,
        )?;
        admit_reads(
            &cx,
            verifications,
            &templates,
            &declared,
            &columns,
            &mut findings,
        );
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
            prepared.keys.to_vec(),
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
        let plan = plan
            .with_row_contexts(contexts)
            .with_recurrences(recurrences);
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

    /// The column phase: each column the templates leave to it binds for
    /// the rows of the bound templates that leave it, once for each of their
    /// drivers, or once over the whole dataset (`catalog`) when it holds a
    /// qualified aggregate (#2829, LOOKUP-18); then the columns' dependency
    /// order (COLUMN-5, COLUMN-6).
    fn bind_column_phase(
        &self,
        cx: &Context<'_>,
        catalog: &Catalog,
        phases: &[TemplatePhase<'_>],
        templates: &[RowTemplate],
        positions: &[Option<usize>],
        findings: &mut Vec<BindFinding>,
    ) -> Result<
        (
            Vec<ColumnBinding>,
            Vec<crate::dataset::recurrence::Recurrence>,
        ),
        BindError,
    > {
        let prepared = cx.prepared;
        let (source, name) = (cx.source(), cx.name());
        let (output, keys) = (&prepared.output, &prepared.keys);
        let project = prepared.project_calls.as_ref();
        let binder::Donors {
            named,
            secondary,
            items: intermediates,
            ..
        } = cx.donors;
        let column_groups = phases
            .iter()
            .filter_map(|phase| Some((phase.driver, phase.groups.as_deref()?)))
            .collect::<Vec<_>>();
        let mut odm_contexts: Option<std::sync::Arc<[crate::dataset::OdmTemplateScope]>> = None;
        let mut columns: Vec<ColumnBinding> = Vec::new();
        let mut dependencies = std::vec![None; output.columns().len()];
        let spec = prepared.tree.spec();
        for declaration in &self.columns {
            if let ExprKind::Scalar { .. } = &spec.expr(declaration.expr).kind {
                findings.extend(
                    crate::scalar_verb::Operation::from_tree(spec, declaration.expr)
                        .map_err(|_| BindError::Internal)?
                        .defect()
                        .cloned()
                        .map(BindFinding::Scalar),
                );
            }
            // A column derivation evaluates only for the rows of the bound
            // templates that leave the column to the column phase. A template
            // whose driver intermediate failed to bind was skipped with its
            // own findings, and so are its rows here.
            let mut scopes = self
                .column_bindings(
                    declaration.column,
                    checks::lookups::aggregates(spec, declaration.expr),
                    name,
                )
                .into_iter()
                .filter_map(|binding| {
                    let members = binding
                        .templates
                        .iter()
                        .filter_map(|&template| positions[template])
                        .collect::<Vec<_>>();
                    (!members.is_empty()).then_some((binding.scope, binding.driver, members))
                })
                .collect::<Vec<_>>();
            scopes.sort_by_key(|(_, _, members)| members[0]);
            if scopes.is_empty() {
                dependencies[declaration.column] = Some(Vec::new());
                continue;
            }
            let bound = scopes.len();
            let start = findings.len();
            let mut edges = Vec::new();
            for (scope, driver, members) in scopes {
                let column_catalog;
                let (catalog_ref, column_driver, port) = match scope {
                    ColumnScope::Dataset => (catalog, name, 0),
                    ColumnScope::Template => {
                        // Resolve the driver as its row template does: an
                        // input's stored schema or the intermediate's visible
                        // fields, without the row joins, so another input
                        // still joins on its implicit join's key.
                        let (port, driver_source, named_driver) = row_drivers::select(
                            source,
                            name,
                            secondary,
                            driver,
                            named,
                            intermediates,
                        )?;
                        column_catalog = row_drivers::extend_catalog(
                            spec,
                            row_drivers::catalog_visible(
                                output,
                                driver_source,
                                driver,
                                named_driver.and_then(|index| intermediates[index].as_ref()),
                                cx.donors,
                            )?,
                            source,
                            name,
                            cx.donors,
                            driver,
                        )?;
                        (&column_catalog, driver, port)
                    }
                };
                let mut binding_edges = Vec::new();
                let mut cx = BindContext {
                    scope: binder::Scope {
                        template: binder::Template::Any,
                        phase: Phase::Column {
                            groups: &column_groups,
                        },
                        driver: column_driver,
                        available: None,
                    },
                    donors: cx.donors,
                    catalog: catalog_ref,
                    output,
                    project,
                    tree: &prepared.tree,
                    edges: &mut binding_edges,
                    findings: &mut *findings,
                    reported: false,
                };
                if let Some(mut value) = bind_assignment(&mut cx, declaration)? {
                    // An ODM read takes the scope of each row's template.
                    crate::dataset::odm::visit_mut(&mut value.expression, &mut |bound| {
                        let contexts = odm_contexts.get_or_insert_with(|| {
                            deferred_odm_contexts(&self.templates, templates)
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
                distinct_findings(findings, start, prepared.source());
            }
            dependencies[declaration.column] = Some(edges);
        }
        let programs = (0..dependencies.len())
            .map(|column| {
                let mut bindings = columns
                    .iter()
                    .filter(|binding| binding.assignment.column == column);
                let binding = bindings.next()?;
                (bindings.next().is_none() && binding.templates.len() == templates.len())
                    .then_some(&binding.assignment)
            })
            .collect::<Vec<_>>();
        let (analyzed, recurrences) =
            crate::dataset::recurrence::analyze(&dependencies, &programs, keys, true)
                .map_err(|_| BindError::Internal)?;
        let order = analyzed.order.clone();
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
        columns.sort_by_key(|binding| {
            order
                .iter()
                .position(|&column| column == binding.assignment.column)
        });
        Ok((columns, recurrences))
    }
}

/// What each template's rows read of the lookups, admitted once the rows
/// bind: a column binding evaluates only for the rows of the templates it
/// completes, a dataset verification for the rows of every template, and a
/// template's own derivations and row filter for its rows only (LOOKUP-8,
/// LOOKUP-9).
fn admit_reads(
    cx: &Context<'_>,
    verifications: &[Verification],
    templates: &[RowTemplate],
    declared: &[&Template],
    columns: &[ColumnBinding],
    findings: &mut Vec<BindFinding>,
) {
    let binder::Donors {
        named,
        items: intermediates,
        ..
    } = cx.donors;
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
                    .map(|&template| lookup_range::RowDriver::of(&templates[template].mode))
                    .collect(),
            )
        })
        .chain(std::iter::once((
            verification_reads.iter().collect(),
            templates
                .iter()
                .map(|template| lookup_range::RowDriver::of(&template.mode))
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
                        vec![lookup_range::RowDriver::of(&template.mode)],
                    )
                }),
        )
        .collect::<Vec<(Vec<&Expression>, Vec<lookup_range::RowDriver>)>>();
    lookup_range::admit(contexts.iter().cloned(), intermediates, named, findings);
    aggregates::admit_record_keys(&contexts, findings);
    let mut admitted = Vec::new();
    for (index, ((template, declaration), filter)) in templates
        .iter()
        .zip(declared)
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
            intermediates,
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
}

/// Without row templates, each key is computed from one base record before
/// any key combination exists (COLUMN-7). Unless record rows make every
/// record its own row, an aggregate, which reduces the records of a key
/// combination, and an `odm` read, which reads them, fail as
/// `key_dependency` at the operation, also inside a `case` branch.
///
/// With record rows, a key is derived as any column is, but during row
/// construction: an unqualified aggregate, which reduces the constructed
/// rows, is not supported there yet and fails as `unsupported_operation`.
fn key_record(prepared: &PreparedSpecification, column: usize) -> Vec<BindFinding> {
    let spec = prepared.tree.spec();
    if matches!(prepared.rows, Rows::Templates(_)) || !prepared.keys.contains(&column) {
        return Vec::new();
    }
    let Some(derivation) = spec
        .columns
        .get(column)
        .and_then(|column| column.derivation.as_ref())
    else {
        return Vec::new();
    };
    let found = checks::positions(spec, derivation.expr)
        .into_iter()
        .filter_map(|e| match &spec.expr(e).kind {
            ExprKind::Aggregate(aggregate) => Some((
                crate::expression_table::Verb::Aggregate.name(),
                e,
                reduces_output(spec, aggregate),
            )),
            ExprKind::Odm(_) => Some((crate::expression_table::Verb::Odm.name(), e, false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if prepared.record_rows() {
        return found
            .into_iter()
            .filter(|&(_, _, output)| output)
            .map(|(_, e, _)| {
                BindFinding::Scalar(crate::application_issue::unsupported(
                    "output_aggregate_key",
                    Some(&checks::path(spec, e)),
                ))
            })
            .collect();
    }
    let name = &prepared.output.columns()[column].name;
    found
        .into_iter()
        .map(|(operation, e, _)| {
            BindFinding::Scalar(crate::diagnostic::Diagnostic {
                code: crate::diagnostic::ConditionCode::ColumnKeyRecordRead,
                spec_paths: vec![checks::path(spec, e)],
                context: [
                    ("column".into(), crate::json::Value::String(name.clone())),
                    (
                        "operation".into(),
                        crate::json::Value::String(operation.into()),
                    ),
                ]
                .into(),
                source_span: None,
                operand_route: None,
            })
        })
        .collect()
}

/// Whether an aggregate reduces the constructed output rows: neither its
/// expression nor a derive binding names a relation (LOOKUP-18).
fn reduces_output(spec: &ast::Spec, aggregate: &ast::Aggregate) -> bool {
    aggregate.bindings.is_empty()
        && matches!(&spec.expr(aggregate.expr).kind, ExprKind::Formula(ast::Formula {
            compiled: Ok(compiled),
            ..
        }) if !compiled.identifiers().any(|name| name.contains('.'))
            && compiled.record_relations().next().is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Document, DocumentNode as N};

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
        /// Quoted text, as an expression field writes a literal (D18).
        fn literal(&mut self, value: &str) -> usize {
            self.text(&format!("'{value}'"))
        }
        fn literal_derivation(&mut self, value: &str) -> usize {
            let value = self.literal(value);
            self.map(vec![("value", value)])
        }
        fn source_derivation(&mut self, variable: &str) -> usize {
            let value = self.text(variable);
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
    fn shared_default_rows() -> (Templates, ast::Spec) {
        let large = "shared-authored-default ".repeat(256);
        let mut arena = Arena::new();
        let id_default = arena.source_derivation("SRC.ID");
        let big_default = {
            let value = arena.literal(&large);
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
        let version = arena.text("1.0");
        let file_type = arena.text("spec");
        let root = arena.map(vec![
            ("yamaa", version),
            ("type", file_type),
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
        let intermediates = intermediates::Declarations::default();
        let spec = crate::front_end::v1::spec::lower(&document, None).into_spec();
        let admission = derivations::Admission {
            project: None,
            intermediates: &intermediates,
        };
        let rows = Templates::prepare(&spec, &output, "SRC", &admission).unwrap();
        (rows, spec)
    }

    fn declaration(rows: &Templates, template: usize, column: usize) -> &SharedDeclaration {
        rows.templates[template]
            .declarations
            .iter()
            .find(|declaration| declaration.column == column)
            .unwrap()
    }
    fn literal_payload<'s>(spec: &'s ast::Spec, declaration: &RowDeclaration) -> &'s str {
        let ExprKind::Literal(ast::Literal::Text(value)) = &spec.expr(declaration.expr).kind else {
            panic!("literal default payload");
        };
        value
    }

    #[test]
    fn admitted_defaults_are_shared_once_across_templates() {
        let (rows, spec) = shared_default_rows();
        assert_eq!(rows.templates.len(), 3);
        // BIG (column 1): templates 0 and 2 inherit the authored default and
        // hold the very same immutable declaration and literal payload.
        let a_big = declaration(&rows, 0, 1);
        let c_big = declaration(&rows, 2, 1);
        assert!(std::sync::Arc::ptr_eq(a_big, c_big));
        assert_eq!(a_big.path, "columns.BIG.derivation");
        assert!(std::ptr::eq(
            literal_payload(&spec, a_big),
            literal_payload(&spec, c_big)
        ));
        assert_eq!(literal_payload(&spec, a_big).len(), 6144);
        // The conversion handler rides the shared declaration, not a copy.
        let handler = a_big.handler.as_ref().unwrap();
        assert_eq!(handler.spec_path, "columns.BIG.derivation.unconvertible");
        assert!(std::ptr::eq(handler, c_big.handler.as_ref().unwrap()));
        // NOTE (column 2): templates 1 and 2 share the default.
        let b_note = declaration(&rows, 1, 2);
        let c_note = declaration(&rows, 2, 2);
        assert!(std::sync::Arc::ptr_eq(b_note, c_note));
        assert_eq!(literal_payload(&spec, b_note), "note-default");
    }

    #[test]
    fn authored_overrides_retain_their_own_payloads() {
        let (rows, spec) = shared_default_rows();
        // Template 1 overrides BIG: its own declaration, path and payload.
        let b_big = declaration(&rows, 1, 1);
        assert!(!std::sync::Arc::ptr_eq(b_big, declaration(&rows, 0, 1)));
        assert_eq!(b_big.path, "rows.records-b.derivations.BIG");
        assert_eq!(literal_payload(&spec, b_big), "big-override");
        assert!(b_big.handler.is_none());
        // Template 0 overrides NOTE likewise.
        let a_note = declaration(&rows, 0, 2);
        assert!(!std::sync::Arc::ptr_eq(a_note, declaration(&rows, 1, 2)));
        assert_eq!(a_note.path, "rows.records-a.derivations.NOTE");
        assert_eq!(literal_payload(&spec, a_note), "note-a");
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
        let (rows, _) = shared_default_rows();
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
        let (repeated, _) = shared_default_rows();
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

    /// Templates a (SRC, derives column 0), b (ALT, derives 0 and 1) and c
    /// (SRC grouped by SRC.ID, derives 0). Column 2 is left to the column
    /// phase by every template.
    fn scoped_templates() -> Templates {
        let template = |id: &str, driver: &str, groups: Option<&str>, columns: &[usize]| Template {
            id: id.into(),
            path: format!("rows.{id}"),
            driver: driver.into(),
            groups: groups.map(|group| vec![group.to_string()]),
            declarations: columns
                .iter()
                .map(|&column| {
                    SharedDeclaration::new(RowDeclaration {
                        handler: None,
                        column,
                        path: format!("rows.{id}.{column}"),
                        expr: ExprId(0),
                    })
                })
                .collect(),
            filter: None,
        };
        Templates {
            templates: vec![
                template("a", "SRC", None, &[0]),
                template("b", "ALT", None, &[0, 1]),
                template("c", "SRC", Some("SRC.ID"), &[0]),
            ],
            columns: Vec::new(),
        }
    }

    #[test]
    fn column_phase_bindings_follow_the_templates_that_leave_the_column() {
        let rows = scoped_templates();
        let summary = |bindings: Vec<ColumnPhaseBinding<'_>>| {
            bindings
                .into_iter()
                .map(|binding| (binding.scope, binding.driver.to_string(), binding.templates))
                .collect::<Vec<_>>()
        };
        // Only a and c leave column 1; both are driven by SRC.
        assert_eq!(
            summary(rows.column_bindings(1, false, "SRC")),
            [(ColumnScope::Template, "SRC".to_string(), vec![0, 2])]
        );
        // Every template leaves column 2: one binding per distinct driver, in
        // construction order, each completing its own driver's templates.
        assert_eq!(
            summary(rows.column_bindings(2, false, "SRC")),
            [
                (ColumnScope::Template, "SRC".to_string(), vec![0, 2]),
                (ColumnScope::Template, "ALT".to_string(), vec![1]),
            ]
        );
        // A qualified aggregate keeps the whole-dataset scope through the
        // primary port, for every template that leaves the column.
        assert_eq!(
            summary(rows.column_bindings(2, true, "PRIMARY")),
            [(ColumnScope::Dataset, "PRIMARY".to_string(), vec![0, 1, 2])]
        );
        // No template leaves column 0.
        assert!(summary(rows.column_bindings(0, false, "SRC")).is_empty());
    }

    #[test]
    fn a_template_scope_is_its_row_phase_from_the_declarations() {
        let rows = scoped_templates();
        let phases = rows.phases();
        let scope = phases[1].scope();
        assert!(matches!(scope.template, binder::Template::Row("b")));
        assert_eq!(scope.driver, "ALT");
        assert_eq!(scope.available, Some(&[0, 1][..]));
        assert!(matches!(scope.phase, Phase::Row { group_by: None }));
        let grouped = phases[2].scope();
        assert!(matches!(grouped.template, binder::Template::Row("c")));
        assert!(matches!(
            grouped.phase,
            Phase::Row {
                group_by: Some(["SRC.ID"])
            }
        ));
        assert_eq!(grouped.available, Some(&[0][..]));
    }
}
