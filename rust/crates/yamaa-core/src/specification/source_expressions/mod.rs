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
pub(super) use row_sources::bind as bind_row;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub variable: String,
    /// The range an implicit join matches besides its keys (LOOKUP-9).
    range: Option<lookup_range::Declaration>,
    filter: Option<Filter>,
    order: Vec<(String, bool, bool)>,
    keep: Option<crate::dataset::Keep>,
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
                &[("identifier", &self.variable)],
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

    pub fn prepare(
        d: &Document,
        payload: usize,
        path: &str,
        extra: &mut Vec<UnsupportedFeature>,
        intermediates: &intermediates::Declarations,
    ) -> Result<Self, PrepareError> {
        if let N::Text(variable) = &d.nodes()[payload] {
            return Ok(Self {
                variable: variable.clone(),
                range: None,
                filter: None,
                order: Vec::new(),
                keep: None,
                absent: None,
                require_match: false,
                declaration_findings: Vec::new(),
            });
        }
        for &(name, _) in mapping(d, payload)? {
            let name = text(d, name)?;
            if ![
                "variable",
                "between",
                "filter",
                "order_by",
                "keep",
                "absent",
                "require_match",
            ]
            .contains(&name)
            {
                reject(extra, name, format!("{path}.{name}"));
            }
        }
        let variable: String = text(d, field(d, payload, "variable")?)?.into();
        let range = lookup_range::Declaration::prepare(d, payload, path)?;
        let filter = if let Some(id) = d
            .field(payload, "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let expression = text(d, id)?;
            let compiled = if variable
                .split_once('.')
                .is_some_and(|(name, _)| !intermediates.contains(name))
            {
                Some(compile(
                    expression,
                    ExpressionContext::Predicate,
                    &format!("{path}.filter"),
                ))
            } else {
                None
            };
            match compiled.as_ref() {
                Some(Ok(_)) | Some(Err(CompileError::Invalid { .. })) | None => {}
                Some(Err(error)) => {
                    return Err(PrepareError::expression(
                        error.clone(),
                        Some(&format!("{path}.filter")),
                    ))
                }
            }
            Some(Filter {
                expression: expression.into(),
                compiled,
            })
        } else {
            None
        };
        let keep = d
            .field(payload, "keep")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| text(d, id))
            .transpose()?
            .map(|value| match value {
                "first" => crate::dataset::Keep::First,
                "last" => crate::dataset::Keep::Last,
                _ => unreachable!("normalized keep policy"),
            });
        let mut order = Vec::new();
        if let Some(id) = d
            .field(payload, "order_by")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            for &term in sequence(d, id)? {
                order.push(order_term(d, term)?);
            }
        }
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
            lookup::selection_unpaired(path, Owner::Source(&variable), declared, missing)
        }));
        let absent = d
            .field(payload, "absent")
            .map(|id| literal(d, id, &format!("{path}.absent")))
            .transpose()?;
        let require_match = requires_match(d, payload);
        // Asserting a match and answering its absence are two absence policies.
        if require_match && absent.is_some() {
            declaration_findings.push(lookup::absence_conflict(
                format!("{path}.absent"),
                format!("{path}.require_match"),
                Owner::Source(&variable),
            ));
        }
        Ok(Self {
            variable,
            range,
            filter,
            order,
            keep,
            absent,
            require_match,
            declaration_findings,
        })
    }
    /// Scope checks precede source field resolution. Every occurrence is admitted,
    /// including identifiers behind a short-circuiting predicate branch.
    pub(crate) fn bind_filter(
        &self,
        path: &str,
        source: &SourceDeclaration,
        schema: &TableSchema,
        intermediates: &intermediates::Declarations,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<BoundPredicate>, BindError> {
        self.bind_filter_named(path, &source.name, schema, intermediates, findings)
    }
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
                &[("identifier", &self.variable)],
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
                &[("identifier", &self.variable)],
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
                &[("identifier", &self.variable)],
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
                &[("identifier", &self.variable)],
            )));
        }
        refused
    }
    /// Bind the range of this binding's implicit join over the joined
    /// relation (LOOKUP-9): `Some(None)` without a range, and `None` when a
    /// finding refuses it. Whether a record read names the driver of every row
    /// that reads the join is admitted per context ([`lookup_range::admit`]).
    /// `row` names the row template of a row-phase read, and `driver` the
    /// driver whose records build the rows the binding is read for.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind_range(
        &self,
        donor: &TableSchema,
        catalog: &Catalog,
        secondary: &[crate::dataset::SecondarySource],
        intermediates: &[Option<crate::dataset::Intermediate>],
        row: Option<&str>,
        driver: Option<&str>,
        available: Option<&[usize]>,
        edges: &mut Vec<usize>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<Option<crate::dataset::RelationRange>>, BindError> {
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
        Ok(match range.finish(read, kind, bounds, None)? {
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
    fn wrap(
        &self,
        expression: Expression,
        column: usize,
        path: &str,
        variable_absent: bool,
    ) -> Expression {
        if self.absent.is_some() || self.keep.is_some() {
            Expression::SourceBinding {
                source: std::boxed::Box::new(crate::dataset::Assignment {
                    column,
                    expression,
                    path: path.into(),
                }),
                absent: self.absent.clone(),
                variable_absent,
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
    pub fn prepare(
        d: &Document,
        payload: usize,
        path: &str,
        extra: &mut Vec<UnsupportedFeature>,
        intermediates: &intermediates::Declarations,
    ) -> Result<Self, PrepareError> {
        for &(name, _) in mapping(d, payload)? {
            let name = text(d, name)?;
            if !["sources", "missing"].contains(&name) {
                reject(extra, name, format!("{path}.{name}"));
            }
        }
        let mut sources = Vec::new();
        for (index, &operand) in sequence(d, field(d, payload, "sources")?)?
            .iter()
            .enumerate()
        {
            sources.push(Declaration::prepare(
                d,
                operand,
                &format!("{path}.sources[{index}]"),
                extra,
                intermediates,
            )?);
        }
        let missing = d
            .field(payload, "missing")
            .map(|id| literal(d, id, &format!("{path}.missing")))
            .transpose()?
            .unwrap_or(Value::Missing);
        Ok(Self { sources, missing })
    }
}

/// Bind scalar source behavior once using actual immutable input schemas.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    prepared: &PreparedSpecification,
    declaration: &Declaration,
    column: usize,
    path: &str,
    catalog: &Catalog,
    driver: &TableSchema,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    if declaration.report_declaration(findings) {
        return Ok(None);
    }
    let name = &declaration.variable;
    let mut variable_absent = false;
    let expression = if let Some((index, field)) = prepared.intermediates.reference(name) {
        if let Some(item) = &intermediates[index] {
            // A filtered-source operand naming one intermediate record cannot filter it again.
            declaration.bind_filter(
                path,
                prepared.source(),
                item.source_schema(secondary).ok_or(BindError::Internal)?,
                &prepared.intermediates,
                findings,
            )?;
            if let Some(column) = item.source_column(secondary, field) {
                edges.extend(row_intermediates::dependencies(index, intermediates)?);
                Some(Expression::Intermediate { index, column })
            } else if declaration.absent.is_some() && !item.source.is_self() {
                variable_absent = true;
                Some(Expression::Literal(Value::Missing))
            } else {
                findings.push(BindFinding::Lookup(LookupFinding::reference(
                    path,
                    name,
                    None,
                    ReferenceCause::Value,
                    None,
                )));
                None
            }
        } else {
            findings.push(BindFinding::UnknownReference {
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
            let source_declaration = prepared
                .sources
                .iter()
                .find(|s| s.name == dataset)
                .ok_or(BindError::Internal)?;
            let filter = declaration.bind_filter(
                path,
                source_declaration,
                &source.schema,
                &prepared.intermediates,
                findings,
            )?;
            let selection = declaration.bind_selection(path, dataset, &source.schema, findings);
            if let Some(source_column) =
                source.schema.columns().iter().position(|c| c.name == field)
            {
                let key_names = prepared
                    .keys
                    .iter()
                    .map(|&column| prepared.output.columns()[column].name.as_str())
                    .collect::<Vec<_>>();
                let donor_fields = source
                    .schema
                    .columns()
                    .iter()
                    .map(|field| reference_binding::Field {
                        name: &field.name,
                        column_type: field.kind,
                    })
                    .collect::<Vec<_>>();
                match crate::key_relation::infer(catalog, &key_names, &donor_fields)
                    .map_err(BindError::Catalog)?
                {
                    crate::key_relation::Inference::Keys { keys } => {
                        let keys = keys
                            .into_iter()
                            .map(|key| {
                                let output_column = prepared.keys[key];
                                let source_column = source
                                    .schema
                                    .columns()
                                    .iter()
                                    .position(|field| field.name == key_names[key])
                                    .expect("inferred applicable field");
                                edges.push(output_column);
                                crate::dataset::MatchKey {
                                    source_column,
                                    output_column,
                                }
                            })
                            .collect();
                        declaration
                            .bind_range(
                                &source.schema,
                                catalog,
                                secondary,
                                intermediates,
                                None,
                                None,
                                None,
                                edges,
                                findings,
                            )?
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
                    crate::key_relation::Inference::NoApplicableKeys => {
                        findings.push(BindFinding::Source(SourceFinding(
                            lookup::no_applicable_keys(path, dataset, &key_names, false),
                        )));
                        None
                    }
                    crate::key_relation::Inference::Incompatible {
                        key,
                        expected,
                        actual,
                    } => {
                        findings.push(BindFinding::Source(SourceFinding(lookup::key_type(
                            path,
                            Owner::Unnamed,
                            key_names[key],
                            expected.into(),
                            actual.into(),
                        ))));
                        None
                    }
                    crate::key_relation::Inference::UndeclaredOutput { .. } => {
                        return Err(BindError::Internal)
                    }
                }
            } else if declaration.absent.is_some() {
                variable_absent = true;
                Some(Expression::Literal(Value::Missing))
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.clone(),
                });
                None
            }
        } else if dataset == prepared.source().name {
            let filter = declaration.bind_filter(
                path,
                prepared.source(),
                driver,
                &prepared.intermediates,
                findings,
            )?;
            let selection = declaration.bind_selection(path, dataset, driver, findings);
            let fields = catalog.bind(name);
            if let Some(reference_binding::Binding::Dataset { field, .. }) = fields {
                if prepared.keys.contains(&column) && filter.is_none() && selection.is_none() {
                    Some(Expression::Source(field))
                } else {
                    Some(Expression::Collect {
                        column: field,
                        identifier: name.clone(),
                        filter,
                        selection,
                    })
                }
            } else if declaration.absent.is_some() {
                variable_absent = true;
                Some(Expression::Literal(Value::Missing))
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.clone(),
                });
                None
            }
        } else {
            findings.push(BindFinding::UnknownReference {
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
            if declaration.absent.is_none() {
                findings.push(BindFinding::OutputReference {
                    path: path.into(),
                    name: name.clone(),
                    row: None,
                    finding,
                });
                return Ok(None);
            }
        }
        declaration.bind_filter(
            path,
            prepared.source(),
            driver,
            &prepared.intermediates,
            findings,
        )?;
        if let Some(reference_binding::Binding::Output { column, .. }) = catalog.bind(name) {
            edges.push(column);
            Some(Expression::Column(column))
        } else if declaration.absent.is_some() {
            variable_absent = true;
            Some(Expression::Literal(Value::Missing))
        } else {
            findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.clone(),
            });
            None
        }
    };
    if expression.as_ref().is_some_and(|expression| {
        declaration.refuse_require_match(path, expression, findings)
            | declaration.refuse_range(path, expression, findings)
    }) {
        return Ok(None);
    }
    Ok(expression.map(|expression| declaration.wrap(expression, column, path, variable_absent)))
}

/// Scalar expression operands use the same inferred output-key join as a bare
/// stored-source derivation. Admission retains the donor and key types.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_implicit(
    name: &str,
    path: &str,
    keys: &[usize],
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
    let key_names = keys
        .iter()
        .map(|&key| outputs.get(key).copied().ok_or(BindError::Internal))
        .collect::<Result<Vec<_>, _>>()?;
    let donor = source
        .schema
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    match crate::key_relation::infer(catalog, &key_names, &donor).map_err(BindError::Catalog)? {
        crate::key_relation::Inference::Keys { keys: inferred } => {
            let mut bound = Vec::new();
            for key in inferred {
                let output_column = keys[key];
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
        crate::key_relation::Inference::NoApplicableKeys => {
            findings.push(BindFinding::Source(SourceFinding(
                lookup::no_applicable_keys(path, dataset, &key_names, false),
            )));
            Ok(None)
        }
        crate::key_relation::Inference::Incompatible {
            key,
            expected,
            actual,
        } => {
            findings.push(BindFinding::Source(SourceFinding(lookup::key_type(
                path,
                Owner::Unnamed,
                key_names[key],
                expected.into(),
                actual.into(),
            ))));
            Ok(None)
        }
        crate::key_relation::Inference::UndeclaredOutput { .. } => Err(BindError::Internal),
    }
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
                JoinedFinding::KeyType {
                    source,
                    expected,
                    actual,
                } => BindFinding::Source(SourceFinding(lookup::key_type(
                    path,
                    Owner::Unnamed,
                    &source,
                    expected.into(),
                    actual.into(),
                ))),
                JoinedFinding::NoApplicableKeys { dataset, keys } => {
                    let keys = keys.iter().map(String::as_str).collect::<Vec<_>>();
                    BindFinding::Source(SourceFinding(lookup::no_applicable_keys(
                        path, &dataset, &keys, false,
                    )))
                }
            };
            findings.push(finding);
            Ok(None)
        }
    }
}

impl Declaration {
    pub(super) fn check_findings(
        &self,
        path: &str,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        if self.report_declaration(findings) {
            return Ok(());
        }
        if let Some(filter) = &self.filter {
            if let Some(compiled) = &filter.compiled {
                static_checks::predicate(
                    compiled,
                    &filter.expression,
                    &format!("{path}.filter"),
                    findings,
                )?;
            }
        }
        Ok(())
    }
}
