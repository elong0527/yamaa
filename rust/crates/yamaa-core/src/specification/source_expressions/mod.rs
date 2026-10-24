//! Source eligibility is compiled in core and bound after actual source ingestion.
use super::*;
use crate::json::Value as JsonValue;
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{
        lookup::{self, Owner},
        ConditionCode as C, Diagnostic,
    },
    value::Value,
};
mod row_sources;
pub(super) use row_sources::bind as bind_template;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub variable: String,
    /// The range an implicit join matches besides its keys (LOOKUP-9).
    range: Option<lookup_range::Declaration>,
    filter: Option<Filter>,
    order: Vec<(String, bool, bool)>,
    keep: Option<crate::dataset::Keep>,
    /// The answer when the read finds no record, never when the variable
    /// does not exist, which fails under NAME-3 (D67).
    absent: Option<Value>,
    require_match: bool,
    /// Declaration defects, reported before any field is bound.
    declaration_findings: Vec<Diagnostic>,
}
#[derive(Clone, Debug)]
struct Filter {
    expression: String,
    compiled: Option<Result<CompiledExpression, CompileError>>,
}
#[derive(Debug)]
pub struct SourceFinding(Diagnostic);
impl SourceFinding {
    pub(super) fn from_diagnostic(diagnostic: Diagnostic) -> Self {
        Self(diagnostic)
    }
    pub fn diagnostic(&self) -> Diagnostic {
        self.0.clone()
    }
    pub fn definition(&self) -> crate::diagnostic::Definition {
        self.0.definition()
    }
    pub(super) fn new(code: C, path: &str, fields: &[(&str, &str)]) -> Self {
        Self(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: fields
                .iter()
                .map(|(name, value)| ((*name).into(), JsonValue::String((*value).into())))
                .collect(),
            source_span: None,
            operand_route: None,
        })
    }
}
/// The dataset a source binding's variable reads: its qualifier.
pub(super) fn source_dataset(variable: &str) -> &str {
    variable
        .split_once('.')
        .map_or(variable, |(dataset, _)| dataset)
}

impl Declaration {
    /// A declared grouped key is already one scalar in both row and column phase.
    /// Group scope is declaration metadata and grants no study-source authority.
    pub fn reject_group_key_filter(
        &self,
        path: &str,
        driver: &str,
        phase: crate::reference_scope::Phase<'_>,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        if self.filter.is_none() || !self.declaration_findings.is_empty() {
            return false;
        }
        let scalar = self.is_group_key(driver, phase);
        if scalar {
            findings.push(BindFinding::Source(SourceFinding::new(
                C::SourceFilterSingleValue,
                &format!("{path}.filter"),
                &[("identifier", &self.variable), ("construct", "filter")],
            )));
        }
        scalar
    }

    pub fn is_group_key(&self, driver: &str, phase: crate::reference_scope::Phase<'_>) -> bool {
        let Some((dataset, _)) = self.variable.split_once('.') else {
            return false;
        };
        match phase {
            crate::reference_scope::Phase::Row { group_by } => {
                dataset == driver
                    && group_by.is_some_and(|group| group.contains(&self.variable.as_str()))
            }
            crate::reference_scope::Phase::Column { groups } => {
                let mut own = groups
                    .iter()
                    .filter(|&&(owner, _)| owner == dataset)
                    .peekable();
                own.peek().is_some()
                    && own.all(|(_, group)| group.contains(&self.variable.as_str()))
            }
        }
    }

    /// The source binding at `e`: a name, or a variable with its record
    /// selection. `absent` answers a variable that does not exist only where
    /// the spec's surface says it does (D67).
    pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Self, PrepareError> {
        let path = checks::path(spec, e);
        let read = match &spec.expr(e).kind {
            ExprKind::Ref(name) => {
                return Ok(Self {
                    variable: name.0.clone(),
                    range: None,
                    filter: None,
                    order: Vec::new(),
                    keep: None,
                    absent: None,
                    require_match: false,
                    declaration_findings: Vec::new(),
                })
            }
            ExprKind::Source(read) => read,
            _ => return Err(PrepareError::Internal),
        };
        let variable: String = checks::name(spec, read.variable)
            .ok_or(PrepareError::Internal)?
            .into();
        let range = read
            .between
            .as_ref()
            .map(|between| lookup_range::Declaration::from_tree(spec, between))
            .transpose()?;
        let filter = read
            .filter
            .map(|filter| match &spec.expr(filter).kind {
                ExprKind::Formula(formula) => Ok(Filter {
                    expression: formula.text.text.clone(),
                    compiled: Some(formula.compiled.clone()),
                }),
                _ => Err(PrepareError::Internal),
            })
            .transpose()?;
        let keep = read.keep.map(|keep| match keep {
            ast::Keep::First => crate::dataset::Keep::First,
            ast::Keep::Last => crate::dataset::Keep::Last,
        });
        let order = read
            .order_by
            .iter()
            .map(|term| tree_order_term(spec, term))
            .collect::<Result<Vec<_>, _>>()?;
        let mut declaration_findings = Vec::new();
        declaration_findings.extend(
            range
                .as_ref()
                .and_then(|range| range.bounds_finding(Owner::Source(&variable))),
        );
        declaration_findings.extend((order.is_empty() != keep.is_none()).then(|| {
            let (declared, missing) = if keep.is_some() {
                ("keep", "order_by")
            } else {
                ("order_by", "keep")
            };
            lookup::selection_unpaired(&path, Owner::Source(&variable), &[declared], &[missing])
        }));
        let absent = checks::handler(spec, read.absent.as_ref())?;
        // Asserting a match and answering its absence are two absence policies.
        if read.require_match && absent.is_some() {
            declaration_findings.push(lookup::absence_conflict(
                format!("{path}.absent"),
                format!("{path}.require_match"),
                Owner::Source(&variable),
                source_dataset(&variable),
            ));
        }
        Ok(Self {
            variable,
            range,
            filter,
            order,
            keep,
            absent,
            require_match: read.require_match,
            declaration_findings,
        })
    }
    /// Scope checks precede source field resolution. Every occurrence is admitted,
    /// including identifiers behind a short-circuiting predicate branch.
    pub fn bind_filter_named(
        &self,
        path: &str,
        source: &str,
        schema: &TableSchema,
        intermediates: &intermediates::Declarations,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<BoundPredicate>, BindError> {
        let Some(Filter {
            expression,
            compiled,
        }) = &self.filter
        else {
            return Ok(None);
        };
        let path = format!("{path}.filter");
        let Some((dataset, _)) = self
            .variable
            .split_once('.')
            .filter(|(name, _)| !intermediates.contains(name))
        else {
            findings.push(BindFinding::Source(SourceFinding::new(
                C::SourceFilterSingleValue,
                &path,
                &[("identifier", &self.variable), ("construct", "filter")],
            )));
            return Ok(None);
        };
        let plan = match compiled.as_ref().ok_or(BindError::Internal)? {
            Ok(plan) => plan,
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Source(SourceFinding(failure.diagnostic(
                    ExpressionContext::Predicate,
                    &path,
                    expression,
                    position.character,
                ))));
                return Ok(None);
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
        };
        let start = findings.len();
        let mut bindings = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for name in plan.identifiers() {
            if !seen.insert(name) {
                continue;
            }
            let reference = name.split_once('.');
            if reference.is_none_or(|(name, _)| name != dataset) {
                findings.push(BindFinding::Source(SourceFinding::new(
                    C::SourceFilterReference,
                    &path,
                    &[("identifier", name), ("dataset", dataset)],
                )));
                continue;
            }
            // The predicate is bound against the named immutable input schema.
            if dataset != source {
                return Err(BindError::Internal);
            }
            let field = reference.expect("qualified filter reference").1;
            if let Some(column) = schema
                .columns()
                .iter()
                .position(|column| column.name == field)
            {
                if !bindings
                    .iter()
                    .any(|binding: &Binding| binding.name == name)
                {
                    bindings.push(Binding {
                        name: name.into(),
                        read: Read::Source(column),
                    });
                }
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: path.clone(),
                    name: name.into(),
                });
            }
        }
        if findings.len() != start {
            return Ok(None);
        }
        let typed = plan.typed(|name| {
            crate::result_type::ResultType::declared(
                name.split_once('.')
                    .and_then(|(_, field)| {
                        schema.columns().iter().find(|column| column.name == field)
                    })
                    .map(|column| column.kind),
            )
        });
        BoundPredicate::new(typed, bindings)
            .map(Some)
            .map_err(BindError::InvalidPredicateBinding)
    }
    /// A filtered current-driver source selects feeding records before copying
    /// its scalar value. Predicate identifiers remain donor-only, even in a case.
    pub fn bind_row_filter(
        &self,
        path: &str,
        driver: &str,
        catalog: &Catalog,
        intermediates: &intermediates::Declarations,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<BoundPredicate>, BindError> {
        let Some(Filter {
            expression,
            compiled,
        }) = &self.filter
        else {
            return Ok(None);
        };
        let path = format!("{path}.filter");
        let Some((dataset, _)) = self
            .variable
            .split_once('.')
            .filter(|(name, _)| !intermediates.contains(name))
        else {
            findings.push(BindFinding::Source(SourceFinding::new(
                C::SourceFilterSingleValue,
                &path,
                &[("identifier", &self.variable), ("construct", "filter")],
            )));
            return Ok(None);
        };
        let plan = match compiled.as_ref().ok_or(BindError::Internal)? {
            Ok(plan) => plan,
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Source(SourceFinding(failure.diagnostic(
                    ExpressionContext::Predicate,
                    &path,
                    expression,
                    position.character,
                ))));
                return Ok(None);
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
        };
        let start = findings.len();
        let mut bindings = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for name in plan.identifiers() {
            if !seen.insert(name) {
                continue;
            }
            if name.split_once('.').is_none_or(|(name, _)| name != dataset) {
                findings.push(BindFinding::Source(SourceFinding::new(
                    C::SourceFilterReference,
                    &path,
                    &[("identifier", name), ("dataset", dataset)],
                )));
                continue;
            }
            if dataset == driver {
                if let Some(reference_binding::Binding::Dataset { field, .. }) = catalog.bind(name)
                {
                    bindings.push(Binding {
                        name: name.into(),
                        read: Read::Source(field),
                    });
                    continue;
                }
            }
            findings.push(BindFinding::UnknownReference {
                path: path.clone(),
                name: name.into(),
            });
        }
        if findings.len() != start {
            return Ok(None);
        }
        let typed =
            plan.typed(|name| crate::result_type::ResultType::declared(catalog.type_of(name)));
        BoundPredicate::new(typed, bindings)
            .map(Some)
            .map_err(BindError::InvalidPredicateBinding)
    }
    pub fn bind_selection(
        &self,
        path: &str,
        dataset: &str,
        schema: &TableSchema,
        findings: &mut Vec<BindFinding>,
    ) -> Option<crate::dataset::SourceSelection> {
        let keep = self.keep?;
        let mut order_by = Vec::new();
        for (index, (name, descending, nulls_first)) in self.order.iter().enumerate() {
            if let Some(column) = name
                .strip_prefix(&format!("{dataset}."))
                .and_then(|name| schema.columns().iter().position(|c| c.name == name))
            {
                order_by.push(crate::dataset::OrderTerm {
                    column,
                    descending: *descending,
                    nulls_first: *nulls_first,
                });
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: format!("{path}.order_by[{index}]"),
                    name: name.clone(),
                });
            }
        }
        Some(crate::dataset::SourceSelection { order_by, keep })
    }
    /// Report every declaration defect; true when there was one.
    fn report_declaration(&self, findings: &mut Vec<BindFinding>) -> bool {
        findings.extend(
            self.declaration_findings
                .iter()
                .map(|finding| BindFinding::Source(SourceFinding(finding.clone()))),
        );
        !self.declaration_findings.is_empty()
    }
    /// A source binding's answer for a row its implicit join finds no record
    /// for. The concise `DATASET.VARIABLE` is shorthand for the binding, so it
    /// answers the same way, with the default (LOOKUP-24).
    fn absence(&self, path: &str) -> crate::dataset::JoinAbsence {
        crate::dataset::JoinAbsence {
            path: path.into(),
            absent: self.absent.clone(),
            require_match: self.require_match,
        }
    }
    /// `require_match` asserts what only an implicit join can miss; on a read
    /// of the driver, an output column or a named intermediate it states
    /// nothing and is refused (LOOKUP-24). True when refused.
    fn refuse_require_match(
        &self,
        path: &str,
        expression: &Expression,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let refused = self.require_match
            && !matches!(
                expression,
                Expression::SourceLookup { .. }
                    | Expression::Joined(_)
                    | Expression::JoinedSource { .. }
            );
        if refused {
            findings.push(BindFinding::Source(SourceFinding::new(
                C::SourceRequireMatchScope,
                &format!("{path}.require_match"),
                &[
                    ("identifier", &self.variable),
                    ("construct", "require_match"),
                ],
            )));
        }
        refused
    }
    /// `between` narrows what an implicit join matches; on a read of the
    /// driver, an output column or a named intermediate there is nothing to
    /// match, and it is refused (LOOKUP-9). True when refused.
    fn refuse_range(
        &self,
        path: &str,
        expression: &Expression,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let refused = self.range.is_some()
            && !matches!(
                expression,
                Expression::SourceLookup { .. } | Expression::JoinedSource { .. }
            );
        if refused {
            findings.push(BindFinding::Source(SourceFinding::new(
                C::SourceRangeScope,
                &format!("{path}.between"),
                &[("identifier", &self.variable), ("construct", "between")],
            )));
        }
        refused
    }
    /// Bind the range of this binding's implicit join over the joined
    /// relation (LOOKUP-9): `Some(None)` without a range, and `None` when a
    /// finding refuses it. Whether a record read names the driver of every row
    /// that reads the join is admitted per context ([`lookup_range::admit`]).
    /// The scope `cx` names the row template of a row-phase read, and `driver`
    /// the driver whose records build the rows the binding is read for. A bare
    /// value that no column takes fails whatever the bounds bind.
    pub(super) fn bind_range(
        &self,
        cx: &mut BindContext<'_, '_>,
        donor: &TableSchema,
        driver: Option<&str>,
    ) -> Result<Option<Option<crate::dataset::RelationRange>>, BindError> {
        let binder::Donors {
            secondary,
            items: intermediates,
            ..
        } = cx.donors;
        let (catalog, row, available) = (cx.catalog, cx.row(), cx.scope.available);
        let (edges, findings) = (&mut *cx.edges, &mut *cx.findings);
        let Some(range) = &self.range else {
            return Ok(Some(None));
        };
        let owner = Owner::Source(&self.variable);
        let wrap = |diagnostic| BindFinding::Source(SourceFinding(diagnostic));
        let mut diagnostics = Vec::new();
        let bounds = range.bind_bounds(
            owner,
            donor,
            &|column| donor.columns().get(column).map(|field| field.kind),
            &|_| true,
            &mut diagnostics,
        );
        findings.extend(diagnostics.into_iter().map(wrap));
        let Some(bounds) = bounds else {
            if range.names_no_column(catalog) {
                findings.push(wrap(range.unknown_value(owner)));
            }
            return Ok(None);
        };
        // A column only the column phase derives is not yet a value of a row
        // under construction.
        if !range.value.contains('.') {
            if let Some(finding @ reference_binding::Diagnostic::PhaseBoundary { .. }) = catalog
                .validate_output(&range.value, None, available, &[0])
                .map_err(BindError::Catalog)?
            {
                findings.push(BindFinding::OutputReference {
                    path: format!("{}.value", range.path()),
                    name: range.value.clone(),
                    row: row.map(String::from),
                    finding,
                });
                return Ok(None);
            }
        }
        if driver.is_some_and(|driver| {
            range.names_derived_driver_field(driver, secondary, intermediates)
        }) {
            findings.push(BindFinding::Scalar(crate::application_issue::unsupported(
                "intermediate_driver_range_value",
                Some(&format!("{}.value", range.path())),
            )));
            return Ok(None);
        }
        let Some((read, kind)) =
            range.current_value(catalog, secondary, intermediates, driver, available, edges)?
        else {
            findings.push(wrap(range.unknown_value(owner)));
            return Ok(None);
        };
        // A type refusal names the variable too, as every other refusal of
        // the binding's range does.
        Ok(match range.finish(read, kind, bounds, owner)? {
            Ok(range) => Some(Some(range.for_variable(&self.variable))),
            Err(diagnostic) => {
                findings.push(wrap(diagnostic));
                None
            }
        })
    }
    /// The authored range value, when the binding declares a range.
    pub(super) fn range_value(&self) -> Option<&str> {
        self.range.as_ref().map(|range| range.value.as_str())
    }
    fn wrap(&self, expression: Expression, column: usize, path: &str) -> Expression {
        if self.absent.is_some() || self.keep.is_some() {
            Expression::SourceBinding {
                source: std::boxed::Box::new(crate::dataset::Assignment {
                    column,
                    expression,
                    path: path.into(),
                }),
                absent: self.absent.clone(),
                selection_declared: self.keep.is_some(),
                variable: self.variable.clone(),
                path: path.into(),
            }
        } else {
            expression
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FirstAvailable {
    pub sources: Vec<Declaration>,
    pub missing: Value,
}
impl FirstAvailable {
    /// `first_available` from the tree: each source a binding, and the
    /// value when none is present.
    pub(super) fn from_tree(
        spec: &ast::Spec,
        sources: &[ExprId],
        missing: Option<&ast::Constant>,
    ) -> Result<Self, PrepareError> {
        Ok(Self {
            sources: sources
                .iter()
                .map(|&source| Declaration::from_tree(spec, source))
                .collect::<Result<_, _>>()?,
            missing: checks::handler(spec, missing)?.unwrap_or(Value::Missing),
        })
    }
}

/// Bind a structured source in this scope: the implicit template's rules,
/// or a row template's ([`row_sources::bind`]).
pub(super) fn bind(
    cx: &mut BindContext<'_, '_>,
    declaration: &Declaration,
    column: usize,
    path: &str,
) -> Result<Option<Expression>, BindError> {
    match cx.implicit() {
        Some(implicit) => bind_implicit_source(cx, implicit, declaration, column, path),
        None => row_sources::bind(cx, declaration, column, path),
    }
}

/// The implicit template's structured source, on the captured schemas: a
/// lookup read takes its selector's record; another input's field joins on
/// its implicit join's key (LOOKUP-1); a driver field reads a key's one record
/// and otherwise collects the distinct readings of the row's records.
fn bind_implicit_source(
    cx: &mut BindContext<'_, '_>,
    implicit: binder::Implicit,
    declaration: &Declaration,
    column: usize,
    path: &str,
) -> Result<Option<Expression>, BindError> {
    if declaration.report_declaration(cx.findings) {
        return Ok(None);
    }
    let catalog = cx.catalog;
    let binder::Donors {
        named,
        secondary,
        items,
    } = cx.donors;
    let base = cx.scope.driver;
    let base_fields = catalog
        .fields(base)
        .map(|fields| {
            TableSchema::new(
                fields
                    .into_iter()
                    .map(|field| crate::table::Column {
                        name: field.name.into(),
                        kind: field.column_type,
                    })
                    .collect(),
            )
        })
        .transpose()
        .map_err(|_| BindError::Internal)?
        .ok_or(BindError::Internal)?;
    let name = &declaration.variable;
    let expression = if let Some((index, field)) = named.reference(name) {
        if let Some(item) = &items[index] {
            // A filtered-source operand naming one intermediate record cannot filter it again.
            declaration.bind_filter_named(
                path,
                base,
                item.source_schema(secondary).ok_or(BindError::Internal)?,
                named,
                cx.findings,
            )?;
            if let Some(column) = item.source_column(secondary, field) {
                cx.edges
                    .extend(row_intermediates::dependencies(index, items)?);
                Some(Expression::Intermediate { index, column })
            } else {
                cx.findings
                    .push(BindFinding::Lookup(LookupFinding::reference(
                        path,
                        name,
                        None,
                        ReferenceCause::Value,
                        None,
                    )));
                None
            }
        } else {
            cx.findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.clone(),
            });
            None
        }
    } else if let Some((dataset, field)) = name.split_once('.') {
        if let Some((source_index, source)) = secondary
            .iter()
            .enumerate()
            .find(|(_, source)| source.name == dataset)
        {
            let filter =
                declaration.bind_filter_named(path, dataset, &source.schema, named, cx.findings)?;
            let selection = declaration.bind_selection(path, dataset, &source.schema, cx.findings);
            if let Some(source_column) =
                source.schema.columns().iter().position(|c| c.name == field)
            {
                let donor_fields = source
                    .schema
                    .columns()
                    .iter()
                    .map(|field| reference_binding::Field {
                        name: &field.name,
                        column_type: field.kind,
                    })
                    .collect::<Vec<_>>();
                // The input's declared key (LOOKUP-1).
                let key_names = row_drivers::declared_key(cx.spec(), dataset);
                match crate::key_relation::declared(catalog, &key_names, &donor_fields)
                    .map_err(BindError::Catalog)?
                {
                    crate::key_relation::Inference::Keys { keys } => {
                        let mut pairs = Vec::new();
                        for key in keys {
                            let output_column = cx
                                .output
                                .columns()
                                .iter()
                                .position(|column| column.name == key_names[key])
                                .ok_or(BindError::Internal)?;
                            let source_column = source
                                .schema
                                .columns()
                                .iter()
                                .position(|field| field.name == key_names[key])
                                .ok_or(BindError::Internal)?;
                            cx.edges.push(output_column);
                            pairs.push(crate::dataset::MatchKey {
                                source_column,
                                output_column,
                            });
                        }
                        let keys = pairs;
                        declaration
                            .bind_range(cx, &source.schema, None)?
                            .map(|range| Expression::SourceLookup {
                                lookup: crate::dataset::Lookup {
                                    source: source_index,
                                    column: source_column,
                                    keys,
                                },
                                range,
                                filter,
                                selection,
                                absence: Some(declaration.absence(path)),
                            })
                    }
                    refused => {
                        cx.findings
                            .push(join_key_finding(path, dataset, &key_names, refused)?);
                        None
                    }
                }
            } else {
                cx.findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.clone(),
                });
                None
            }
        } else if dataset == base {
            let filter =
                declaration.bind_filter_named(path, base, &base_fields, named, cx.findings)?;
            let selection = declaration.bind_selection(path, dataset, &base_fields, cx.findings);
            if let Some(reference_binding::Binding::Dataset { field, .. }) = catalog.bind(name) {
                if implicit.key && filter.is_none() && selection.is_none() {
                    Some(Expression::Source(field))
                } else {
                    Some(Expression::Collect {
                        column: field,
                        identifier: name.clone(),
                        filter,
                        selection,
                    })
                }
            } else {
                cx.findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.clone(),
                });
                None
            }
        } else {
            cx.findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.clone(),
            });
            None
        }
    } else {
        if let Some(finding) = catalog
            .validate_output(name, None, None, &[0])
            .map_err(BindError::Catalog)?
        {
            cx.findings.push(BindFinding::OutputReference {
                path: path.into(),
                name: name.clone(),
                row: None,
                finding,
            });
            return Ok(None);
        }
        declaration.bind_filter_named(path, base, &base_fields, named, cx.findings)?;
        if let Some(reference_binding::Binding::Output { column, .. }) = catalog.bind(name) {
            cx.edges.push(column);
            Some(Expression::Column(column))
        } else {
            cx.findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.clone(),
            });
            None
        }
    };
    if expression.as_ref().is_some_and(|expression| {
        declaration.refuse_require_match(path, expression, cx.findings)
            | declaration.refuse_range(path, expression, cx.findings)
    }) {
        return Ok(None);
    }
    Ok(expression.map(|expression| declaration.wrap(expression, column, path)))
}

/// Scalar expression operands use the same implicit join as a bare
/// stored-source derivation: on the input's declared key, `key_names`.
/// Admission retains the donor and key types.
pub(super) fn bind_implicit(
    name: &str,
    path: &str,
    key_names: &[&str],
    catalog: &Catalog,
    secondary: &[crate::dataset::SecondarySource],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let (dataset, field) = name.split_once('.').ok_or(BindError::Internal)?;
    let (index, source) = secondary
        .iter()
        .enumerate()
        .find(|(_, source)| source.name == dataset)
        .ok_or(BindError::Internal)?;
    let Some(column) = source
        .schema
        .columns()
        .iter()
        .position(|column| column.name == field)
    else {
        findings.push(BindFinding::UnknownReference {
            path: path.into(),
            name: name.into(),
        });
        return Ok(None);
    };
    let outputs = catalog.output_names();
    let donor = source
        .schema
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    match crate::key_relation::declared(catalog, key_names, &donor).map_err(BindError::Catalog)? {
        crate::key_relation::Inference::Keys { keys } => {
            let mut bound = Vec::new();
            for key in keys {
                let output_column = outputs
                    .iter()
                    .position(|name| *name == key_names[key])
                    .ok_or(BindError::Internal)?;
                let source_column = source
                    .schema
                    .columns()
                    .iter()
                    .position(|column| column.name == key_names[key])
                    .ok_or(BindError::Internal)?;
                edges.push(output_column);
                bound.push(crate::dataset::MatchKey {
                    source_column,
                    output_column,
                });
            }
            // An operand answers a row without a record with missing, counted
            // at its path (LOOKUP-24).
            Ok(Some(Expression::SourceLookup {
                lookup: crate::dataset::Lookup {
                    source: index,
                    column,
                    keys: bound,
                },
                range: None,
                filter: None,
                selection: None,
                absence: Some(crate::dataset::JoinAbsence::operand(path)),
            }))
        }
        refused => {
            findings.push(join_key_finding(path, dataset, key_names, refused)?);
            Ok(None)
        }
    }
}

/// The finding of an implicit join of `dataset` whose key does not resolve,
/// at the read: no key (LOOKUP-1, LOOKUP-5), a declared
/// column the input does not store or the current row has no column of
/// (LOOKUP-5), or a pair whose types cannot compare (LOOKUP-9).
pub(super) fn join_key_finding(
    path: &str,
    dataset: &str,
    key_names: &[&str],
    refused: crate::key_relation::Inference,
) -> Result<BindFinding, BindError> {
    use crate::key_relation::Inference;
    let diagnostic = match refused {
        Inference::NoApplicableKeys => lookup::undeclared_input_key(path, dataset, false),
        Inference::Incompatible {
            key,
            expected,
            actual,
        } => lookup::key_type(
            path,
            Owner::Unnamed,
            key_names[key],
            expected.into(),
            actual.into(),
        ),
        Inference::UnknownColumn { key } => lookup::donor_key_reference(
            path,
            Owner::Unnamed,
            &format!("{dataset}.{}", key_names[key]),
        ),
        Inference::UnknownValue { key } => {
            lookup::match_value_reference(path, Owner::Unnamed, key_names[key])
        }
        Inference::Keys { .. } => return Err(BindError::Internal),
    };
    Ok(BindFinding::Source(SourceFinding(diagnostic)))
}

/// Common scalar operand hook for every row driver and its declared donor ports.
pub(super) fn bind_catalog_join(
    name: &str,
    path: &str,
    catalog: &Catalog,
    row: Option<&str>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    use reference_binding::JoinedFinding;
    let read = catalog
        .joined_read(name, available)
        .map_err(BindError::Catalog)?
        .ok_or(BindError::Internal)?;
    match read {
        Ok((Read::Joined(read), _, dependencies)) => {
            edges.extend(dependencies);
            Ok(Some(Expression::Joined(read)))
        }
        Ok(_) => Err(BindError::Internal),
        Err(cause) => {
            if let Some(diagnostic) = cause.key_diagnostic(path) {
                findings.push(BindFinding::Source(SourceFinding(diagnostic)));
                return Ok(None);
            }
            let finding = match cause {
                JoinedFinding::UnknownField => BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.into(),
                },
                JoinedFinding::CurrentKey {
                    identifier,
                    finding,
                } => BindFinding::QualifiedReference {
                    path: path.into(),
                    name: identifier,
                    row: row.map(String::from),
                    finding,
                },
                JoinedFinding::UnavailableKey { column } => {
                    let identifier = String::from(
                        *catalog
                            .output_names()
                            .get(column)
                            .ok_or(BindError::Internal)?,
                    );
                    BindFinding::Source(SourceFinding::new(
                        C::RowDependencyPhaseBoundary,
                        path,
                        &[
                            ("identifier", &identifier),
                            ("row", row.ok_or(BindError::Internal)?),
                            ("available_phase", "column_derivation"),
                            ("required_phase", "row_construction"),
                        ],
                    ))
                }
                JoinedFinding::UndeclaredKey { .. }
                | JoinedFinding::DonorKey { .. }
                | JoinedFinding::MatchValue { .. }
                | JoinedFinding::KeyType { .. } => return Err(BindError::Internal),
            };
            findings.push(finding);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree;
    use crate::test_specification::{column, derivation, document, lower, op};

    /// `V` reads `SRC.UNKNOWN`, a field the input does not hold, with
    /// `absent: 7`. The variable fails under NAME-3: `absent` answers only a
    /// lookup without a record (D67, D102).
    #[test]
    fn absent_does_not_answer_a_variable_that_does_not_exist() {
        let columns = vec![column(
            "V",
            "int",
            derivation(op(
                "source",
                Tree::map(vec![
                    ("variable", Tree::text("SRC.UNKNOWN")),
                    ("absent", Tree::integer(7)),
                ]),
            )),
        )];
        let source = TableSchema::new(vec![crate::table::Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        }])
        .unwrap();
        let spec = lower(document(columns, vec![]));
        let prepared =
            PreparedSpecification::from_tree(spec, Vec::new(), None, None, None).unwrap();
        let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
            panic!("a variable that does not exist fails");
        };
        let [finding] = findings.as_slice() else {
            panic!("one finding: {findings:?}");
        };
        let [diagnostic] = finding
            .diagnostics(prepared.source())
            .unwrap()
            .try_into()
            .unwrap();
        // NAME-3's unknown field.
        assert_eq!(
            diagnostic.code,
            crate::diagnostic::ConditionCode::SourceUnknownReference
        );
        assert_eq!(diagnostic.spec_paths, ["columns.V.derivation.source"]);
        assert_eq!(
            diagnostic.context["identifier"],
            crate::json::Value::String("SRC.UNKNOWN".into())
        );
    }

    /// `V` reads `NOPE`, which no column declares, with a filter and
    /// `absent: 7`. The name fails as a reference first, so `check()` reports
    /// no filter of a single value, which binding does not reach (D67, D102).
    #[test]
    fn a_filtered_unknown_name_fails_as_a_reference_before_its_filter() {
        let columns = vec![column(
            "V",
            "int",
            derivation(op(
                "source",
                Tree::map(vec![
                    ("variable", Tree::text("NOPE")),
                    ("filter", Tree::text("NOPE > 1")),
                    ("absent", Tree::integer(7)),
                ]),
            )),
        )];
        let source = TableSchema::new(vec![crate::table::Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        }])
        .unwrap();
        let spec = lower(document(columns, vec![]));
        let prepared =
            PreparedSpecification::from_tree(spec, Vec::new(), None, None, None).unwrap();
        assert_eq!(prepared.check_declaration_diagnostics(), []);
        let Err(BindError::Invalid(findings)) = prepared.bind(&source) else {
            panic!("binding refuses the read");
        };
        let first = findings[0]
            .diagnostics(prepared.source())
            .unwrap()
            .remove(0);
        // NAME-3's unknown output column.
        assert_eq!(
            first.code,
            crate::diagnostic::ConditionCode::OutputUnknownReference
        );
    }
}
