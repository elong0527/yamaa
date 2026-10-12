//! Source eligibility is compiled in core and bound after actual source ingestion.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{ConditionCode as C, ContextValue, Diagnostic},
    predicate_compiler,
    value::Value,
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub variable: String,
    filter: Option<Filter>,
    order: Vec<(String, bool, bool)>,
    keep: Option<crate::dataset::Keep>,
    absent: Option<Value>,
}
#[derive(Clone, Debug)]
struct Filter {
    expression: String,
    compiled: Option<Result<crate::predicate::Plan, predicate_compiler::Error>>,
}
#[derive(Debug)]
pub struct SourceFinding(Diagnostic);
impl SourceFinding {
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
                .map(|(name, value)| {
                    (
                        (*name).into(),
                        ContextValue::Scalar(Value::Str((*value).into())),
                    )
                })
                .collect(),
            source_span: None,
            operand_route: None,
        })
    }
}
impl Declaration {
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
                filter: None,
                order: Vec::new(),
                keep: None,
                absent: None,
            });
        }
        for &(name, _) in mapping(d, payload)? {
            let name = text(d, name)?;
            if !["variable", "filter", "order_by", "keep", "absent"].contains(&name) {
                reject(extra, name, format!("{path}.{name}"));
            }
        }
        let variable: String = text(d, field(d, payload, "variable")?)?.into();
        let filter = if let Some(id) = d
            .field(payload, "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let expression = text(d, id)?;
            let compiled = if variable
                .split_once('.')
                .is_some_and(|(name, _)| !intermediates.contains(name))
            {
                Some(predicate_compiler::compile(
                    expression,
                    &format!("{path}.filter"),
                    Default::default(),
                ))
            } else {
                None
            };
            match compiled.as_ref() {
                Some(Ok(_))
                | Some(Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::Grammar { .. },
                )))
                | None => {}
                Some(Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::UnsupportedRegex { .. },
                ))) => reject(extra, "predicate_regex", format!("{path}.filter")),
                Some(Err(predicate_compiler::Error::Internal)) => {
                    return Err(PrepareError::Internal)
                }
                Some(Err(_)) => return Err(PrepareError::Limit("predicate_compilation")),
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
                let (name, descending, nulls_first) = if let N::Text(name) = &d.nodes()[term] {
                    (name.as_str(), false, false)
                } else {
                    (
                        text(d, field(d, term, "variable")?)?,
                        d.field(term, "direction")
                            .map(|id| text(d, id))
                            .transpose()?
                            .unwrap_or("asc")
                            == "desc",
                        d.field(term, "nulls")
                            .map(|id| text(d, id))
                            .transpose()?
                            .unwrap_or("last")
                            == "first",
                    )
                };
                order.push((name.into(), descending, nulls_first));
            }
        }
        if keep.is_some()
            && (intermediates.reference(&variable).is_some() || !variable.contains('.'))
        {
            reject(extra, "single_value_source_selection", path.into());
        }
        if order.is_empty() != keep.is_none() {
            reject(extra, "source_selection", path.into());
        }
        let absent = d
            .field(payload, "absent")
            .map(|id| literal(d, id, &format!("{path}.absent")))
            .transpose()?;
        Ok(Self {
            variable,
            filter,
            order,
            keep,
            absent,
        })
    }
    /// Scope checks precede source field resolution. Every occurrence is admitted,
    /// including identifiers behind a short-circuiting predicate branch.
    pub fn bind_filter(
        &self,
        path: &str,
        source: &SourceDeclaration,
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
            Err(predicate_compiler::Error::Parse(
                crate::predicate_parser::ParseError::Grammar {
                    position, failure, ..
                },
            )) => {
                findings.push(BindFinding::Source(SourceFinding(
                    failure
                        .original_diagnostic(&path, expression, position.character)
                        .map_err(|_| BindError::PredicateDiagnosticLimit)?,
                )));
                return Ok(None);
            }
            Err(error) => return Err(BindError::PredicatePolicy(error.clone())),
        };
        let start = findings.len();
        let mut bindings = Vec::new();
        let mut seen = alloc::collections::BTreeSet::new();
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
            if dataset != source.name {
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
        BoundPredicate::new(plan.clone(), bindings)
            .map(Some)
            .map_err(BindError::InvalidPredicateBinding)
    }
    pub fn has_selection(&self) -> bool {
        self.keep.is_some()
    }
    pub fn has_absent(&self) -> bool {
        self.absent.is_some()
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
    pub fn has_filter(&self) -> bool {
        self.filter.is_some()
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
                edges.extend(item.output_dependencies());
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
                match crate::key_relation::infer(
                    catalog,
                    &key_names,
                    &donor_fields,
                    Default::default(),
                )
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
                        Some(Expression::SourceLookup {
                            lookup: crate::dataset::Lookup {
                                source: source_index,
                                column: source_column,
                                keys,
                            },
                            filter,
                            selection,
                        })
                    }
                    crate::key_relation::Inference::NoApplicableKeys => {
                        let mut finding = SourceFinding::new(
                            C::ImplicitSourceNoApplicableKeys,
                            path,
                            &[
                                ("dataset", dataset),
                                (
                                    "hint",
                                    "declare an explicit `intermediate:` with `source`/`key` pairs",
                                ),
                            ],
                        );
                        finding.0.context.insert(
                            "keys".into(),
                            ContextValue::Sequence(
                                key_names
                                    .iter()
                                    .map(|name| ContextValue::Scalar(Value::Str((*name).into())))
                                    .collect(),
                            ),
                        );
                        findings.push(BindFinding::Source(finding));
                        None
                    }
                    crate::key_relation::Inference::Incompatible {
                        key,
                        expected,
                        actual,
                    } => {
                        findings.push(BindFinding::Source(SourceFinding::new(
                            C::ImplicitSourceKeyType,
                            path,
                            &[
                                ("source", key_names[key]),
                                ("expected", type_name(expected)),
                                ("actual", type_name(actual)),
                            ],
                        )));
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
            let fields = catalog.bind(name).map_err(BindError::Catalog)?;
            if let Some(reference_binding::Binding::Dataset { field, .. }) = fields {
                if prepared.keys.contains(&column) {
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
        if let Some(reference_binding::Binding::Output { column, .. }) =
            catalog.bind(name).map_err(BindError::Catalog)?
        {
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
    Ok(expression.map(|expression| {
        if declaration.absent.is_some() || declaration.keep.is_some() {
            Expression::SourceBinding {
                source: alloc::boxed::Box::new(crate::dataset::Assignment {
                    column,
                    expression,
                    path: path.into(),
                }),
                absent: declaration.absent.clone(),
                variable_absent,
                variable: name.clone(),
                path: path.into(),
            }
        } else {
            expression
        }
    }))
}

fn type_name(kind: crate::value::ColumnType) -> &'static str {
    use crate::value::ColumnType::*;
    match kind {
        Str => "str",
        Int => "int",
        Float => "float",
        Date => "date",
        DateTime => "datetime",
    }
}
