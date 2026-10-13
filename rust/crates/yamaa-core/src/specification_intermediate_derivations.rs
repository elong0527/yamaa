//! Donor derivations share the original scalar compiler and retain raw values.
use super::*;
use crate::dataset::{
    CurrentField, CurrentRead, Intermediate, IntermediateDerivations, IntermediateSource,
    SecondarySource, SourceSchemas,
};

#[derive(Clone, Debug)]
pub(super) struct Derivation {
    pub name: String,
    pub branch: cases::Branch,
    kind: Kind,
    driver_kind: Kind,
    pub(super) references: Vec<String>,
}
#[derive(Clone, Debug)]
enum Kind {
    Literal(Option<ColumnType>),
    Reference(String),
    Numeric(crate::numeric_parser::ParsedNumeric),
    Choice(Vec<Kind>),
    StrictChoice(Vec<Kind>),
    Unavailable,
}
fn value_kind(value: &N) -> Option<ColumnType> {
    match value {
        N::Text(_) => Some(ColumnType::Str),
        N::Integer(_) => Some(ColumnType::Int),
        N::Float(_) => Some(ColumnType::Float),
        _ => None,
    }
}
fn prepare_kind(
    d: &Document,
    value: usize,
    depth: usize,
    project: Option<&functions::Prepared>,
) -> Result<Kind, PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("intermediate_type_depth"));
    }
    let &[(op, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    let op = text(d, op)?;
    Ok(match op {
        "function" => {
            Kind::Literal(project.and_then(|project| project.returns.get(&payload).copied()))
        }
        "literal" => Kind::Literal(value_kind(&d.nodes()[payload])),
        "source" => Kind::Reference(match &d.nodes()[payload] {
            N::Text(name) => name.clone(),
            _ => text(d, field(d, payload, "variable")?)?.into(),
        }),
        "compute" => match crate::numeric_parser::parse_numeric(
            text(d, field(d, payload, "expr")?)?,
            Default::default(),
        ) {
            Ok(plan) => Kind::Numeric(plan),
            Err(_) => Kind::Literal(None),
        },
        "mapping" => Kind::Choice(
            mapping(d, field(d, payload, "dict")?)?
                .iter()
                .map(|&(_, value)| Kind::Literal(value_kind(&d.nodes()[value])))
                .collect(),
        ),
        "cut" => Kind::Choice(
            sequence(d, field(d, payload, "labels")?)?
                .iter()
                .map(|&value| Kind::Literal(value_kind(&d.nodes()[value])))
                .collect(),
        ),
        "case" => Kind::Choice(
            sequence(d, payload)?
                .iter()
                .map(|&item| {
                    prepare_kind(
                        d,
                        d.field(item, "then")
                            .or_else(|| d.field(item, "otherwise"))
                            .ok_or(PrepareError::Internal)?,
                        depth + 1,
                        project,
                    )
                })
                .collect::<Result<_, _>>()?,
        ),
        "row_number" | "rank" | "date_diff" | "study_day" | "to_epoch_day" => {
            Kind::Literal(Some(ColumnType::Int))
        }
        "str_pad" | "str_case" | "str_concat" | "str_template" | "str_extract"
        | "date_precision" | "datetime_precision" | "baseline_flag" => {
            Kind::Literal(Some(ColumnType::Str))
        }
        "flag" => Kind::Literal(
            d.field(payload, "true_value")
                .map(|id| value_kind(&d.nodes()[id]))
                .unwrap_or(Some(ColumnType::Str)),
        ),
        "date_impute" | "to_date" => Kind::Literal(Some(ColumnType::Date)),
        "datetime_impute" => Kind::Literal(Some(ColumnType::DateTime)),
        "round_half_away_from_zero" => Kind::Literal(Some(ColumnType::Float)),
        "row_value" | "previous_non_missing" | "locf" => {
            Kind::Reference(text(d, field(d, payload, "source")?)?.into())
        }
        _ => Kind::Literal(None),
    })
}
fn prepare_driver_kind(
    d: &Document,
    value: usize,
    depth: usize,
    project: Option<&functions::Prepared>,
) -> Result<Kind, PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("intermediate_type_depth"));
    }
    let &[(operation, payload)] = mapping(d, value)? else {
        return Err(PrepareError::Internal);
    };
    Ok(match text(d, operation)? {
        "literal" if matches!(d.nodes()[payload], N::Boolean(_)) => Kind::Unavailable,
        "source" | "literal" | "row_number" | "rank" | "str_case" => {
            prepare_kind(d, value, depth, project)?
        }
        "case" => Kind::StrictChoice(
            sequence(d, payload)?
                .iter()
                .map(|&item| {
                    prepare_driver_kind(
                        d,
                        d.field(item, "then")
                            .or_else(|| d.field(item, "otherwise"))
                            .ok_or(PrepareError::Internal)?,
                        depth + 1,
                        project,
                    )
                })
                .collect::<Result<_, _>>()?,
        ),
        _ => Kind::Unavailable,
    })
}
fn unify(items: impl Iterator<Item = Option<ColumnType>>) -> Option<ColumnType> {
    let mut kind = None;
    for item in items.flatten() {
        kind = match kind {
            None => Some(item),
            Some(previous) if previous == item => Some(item),
            Some(ColumnType::Int | ColumnType::Float)
                if matches!(item, ColumnType::Int | ColumnType::Float) =>
            {
                Some(ColumnType::Float)
            }
            _ => return None,
        };
    }
    kind
}
impl Kind {
    fn infer(&self, lookup: &impl Fn(&str) -> Option<ColumnType>) -> Option<ColumnType> {
        match self {
            Self::Unavailable => None,
            Self::StrictChoice(items) => {
                let mut kind = None;
                for item in items {
                    if matches!(item, Self::Literal(None)) {
                        continue;
                    }
                    let next = item.infer(lookup)?;
                    if kind.is_some_and(|previous| previous != next) {
                        return None;
                    }
                    kind = Some(next);
                }
                kind
            }
            Self::Literal(kind) => *kind,
            Self::Reference(name) => lookup(name),
            Self::Choice(items) => unify(items.iter().map(|item| item.infer(lookup))),
            Self::Numeric(parsed) => {
                use crate::numeric_parser::{NumericFunction as F, ParsedKind as K};
                let mut kinds = Vec::new();
                for node in parsed.nodes() {
                    let kind = match &node.kind {
                        K::Number { fractional } => Some(if *fractional {
                            ColumnType::Float
                        } else {
                            ColumnType::Int
                        }),
                        K::Null => None,
                        K::Identifier => {
                            lookup(&parsed.expression()[node.span.start..node.span.end])
                        }
                        K::Unary { operand, .. } | K::Group { operand } => kinds[*operand],
                        K::Binary {
                            operator: crate::numeric::BinaryOperator::Divide,
                            ..
                        } => Some(ColumnType::Float),
                        K::Binary { left, right, .. } => {
                            unify([kinds[*left], kinds[*right]].into_iter())
                        }
                        K::Call {
                            function: F::Sqrt | F::Exp | F::Ln | F::Power | F::RoundHalfAwayFromZero,
                            ..
                        } => Some(ColumnType::Float),
                        K::Call { arguments, .. } => {
                            unify(arguments.iter().map(|&index| kinds[index]))
                        }
                    };
                    kinds.push(kind);
                }
                kinds[parsed.root()]
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    d: &Document,
    id: usize,
    path: &str,
    dataset: &str,
    limits: CompilationLimits,
    project: Option<&functions::Prepared>,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
    named: &intermediates::Declarations,
) -> Result<Vec<Derivation>, PrepareError> {
    let Some(items) = d
        .field(id, "derivations")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(Vec::new());
    };
    let items = mapping(d, items)?;
    if items.len() > limits.columns {
        return Err(PrepareError::Limit("intermediate_derivations"));
    }
    let mut result = Vec::new();
    let mut bytes = 0;
    for &(name, handled) in items {
        let name = text(d, name)?;
        let value = field(d, handled, "value")?;
        let path = format!("{path}.derivations.{name}");
        cases::charge(d, value, &mut bytes, limits.numeric_bytes, 0)?;
        let branch = derivations::prepare_value_with_driver(
            d,
            value,
            &path,
            project,
            named,
            true,
            0,
            extraction,
            templates,
            Some(dataset),
            limits,
        )?;
        let mut references = Vec::new();
        derivations::references(d, value, project, &mut references, 0)?;
        self_relations::template_references(d, value, templates, &mut references, 0)?;
        let kind = prepare_kind(d, value, 0, project)?;
        let driver_kind = prepare_driver_kind(d, value, 0, project)?;
        result.push(Derivation {
            name: name.into(),
            branch,
            kind,
            driver_kind,
            references,
        });
    }
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    declarations: &[Derivation],
    dataset: &str,
    reader: &str,
    pending_schema: &mut Option<TableSchema>,
    stored: &TableSchema,
    stored_available: Option<&[usize]>,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    outer_output: &TableSchema,
    named: &intermediates::Declarations,
    previous: &[Option<Intermediate>],
    project: Option<&crate::project_calls::ProjectCalls>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<IntermediateDerivations>, BindError> {
    if declarations.is_empty() {
        return Ok(None);
    }
    let before = findings.len();
    // Shared earlier donor plans do not recursively copy their expression trees.
    // Admit aggregate retained schema/catalog copies before making the first copy.
    intermediate_sources::admit(
        sources
            .iter()
            .zip(schemas)
            .flat_map(|(source, schema)| core::iter::repeat_n((*schema, source.name.as_str()), 4))
            .chain(core::iter::repeat_n((outer_output, "output"), 2))
            .chain(
                previous
                    .iter()
                    .flatten()
                    .filter_map(|item| match &item.source {
                        IntermediateSource::Primary { schema, name } => {
                            Some((schema, name.as_str()))
                        }
                        IntermediateSource::SelfRows { schema, .. } => Some((schema, "SELF")),
                        _ => None,
                    }),
            ),
    )?;
    let original_secondary = outer_secondary(sources, schemas, driver);
    let mut fields = stored.columns().to_vec();
    let mut driver_kinds = Vec::new();
    for declaration in declarations {
        let lookup = |name: &str| {
            let (owner, field) = name.split_once('.').unwrap_or((dataset, name));
            if owner == dataset {
                fields
                    .iter()
                    .find(|column| column.name == field)
                    .map(|column| column.kind)
            } else if let Some((index, field)) = named.reference(name) {
                previous
                    .get(index)
                    .and_then(Option::as_ref)
                    .and_then(|item| item.source_schema(&original_secondary))
                    .and_then(|schema| schema.columns().iter().find(|column| column.name == field))
                    .map(|column| column.kind)
            } else {
                sources
                    .iter()
                    .position(|source| source.name == owner)
                    .and_then(|index| {
                        schemas[index]
                            .columns()
                            .iter()
                            .find(|column| column.name == field)
                    })
                    .map(|column| column.kind)
            }
        };
        let kind = declaration.kind.infer(&lookup).unwrap_or(ColumnType::Str);
        driver_kinds.push(declaration.driver_kind.infer(&|name| {
            let (owner, field) = name.split_once('.').unwrap_or((dataset, name));
            if owner == dataset {
                let index = fields.iter().position(|column| column.name == field)?;
                if index < stored.columns().len() {
                    Some(fields[index].kind)
                } else {
                    driver_kinds
                        .get(index - stored.columns().len())
                        .copied()
                        .flatten()
                }
            } else if let Some((index, field)) = named.reference(name) {
                let item = previous.get(index).and_then(Option::as_ref)?;
                let schema = item.source_schema(&original_secondary)?;
                let column = schema
                    .columns()
                    .iter()
                    .position(|column| column.name == field)?;
                item.derivations
                    .as_ref()
                    .filter(|derived| column >= derived.stored_columns)
                    .map_or(Some(schema.columns()[column].kind), |derived| {
                        derived
                            .driver_kinds
                            .get(column - derived.stored_columns)
                            .copied()
                            .flatten()
                    })
            } else {
                None
            }
        }));
        if fields.iter().any(|field| field.name == declaration.name) {
            let mut finding = LookupFinding::new(
                declaration
                    .branch
                    .path
                    .rsplit_once('.')
                    .map_or(declaration.branch.path.as_str(), |(path, _)| path),
                crate::diagnostic::ConditionCode::IntermediateDerivationDuplicate,
            );
            finding.text("identifier", &format!("{dataset}.{}", declaration.name));
            finding.text("intermediate", reader);
            finding.text("derivation", &declaration.name);
            findings.push(BindFinding::Lookup(finding));
            continue;
        }
        fields.push(Column {
            name: declaration.name.clone(),
            kind,
        });
    }
    let output = TableSchema::new(fields).map_err(|_| BindError::Internal)?;
    *pending_schema = Some(output.clone());
    if findings.len() != before {
        return Ok(None);
    }
    let mut current = Vec::new();
    for reference in declarations
        .iter()
        .flat_map(|declaration| &declaration.references)
    {
        let Some((owner, field)) = reference.split_once('.') else {
            continue;
        };
        if owner == dataset
            || named.reference(reference).is_some()
            || current
                .iter()
                .any(|field: &CurrentField| field.identifier == *reference)
        {
            continue;
        }
        if let Some((source, column)) = sources.iter().enumerate().find_map(|(index, source)| {
            (source.name == owner)
                .then(|| {
                    schemas[index]
                        .columns()
                        .iter()
                        .position(|column| column.name == field)
                        .map(|column| (index, column))
                })
                .flatten()
        }) {
            current.push(CurrentField {
                identifier: reference.clone(),
                read: CurrentRead::Record {
                    source: if source == driver {
                        0
                    } else if source < driver {
                        source + 1
                    } else {
                        source
                    },
                    column,
                },
            });
        }
    }
    let donor = sources.iter().position(|source| source.name == dataset);
    if donor.is_none() && dataset != "SELF" {
        return Err(BindError::Internal);
    }
    let secondary = sources
        .iter()
        .enumerate()
        .filter(|&(index, _)| Some(index) != donor)
        .map(|(index, source)| SecondarySource {
            name: source.name.clone(),
            schema: schemas[index].clone(),
        })
        .collect::<Vec<_>>();
    let mut source_fields = output.columns().to_vec();
    for (index, field) in current.iter().enumerate() {
        let CurrentRead::Record { source, column } = field.read else {
            unreachable!()
        };
        let original = if source == 0 {
            driver
        } else if source <= driver {
            source - 1
        } else {
            source
        };
        source_fields.push(Column {
            name: format!("\\0current{index}"),
            kind: schemas[original].columns()[column].kind,
        });
    }
    let source = TableSchema::new(source_fields).map_err(|_| BindError::Internal)?;
    let outputs = output
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: column.kind,
        })
        .collect::<Vec<_>>();
    let mut dataset_fields = Vec::new();
    let mut dataset_names = Vec::new();
    for declaration in core::iter::once(dataset).chain(
        sources
            .iter()
            .enumerate()
            .filter(|&(index, _)| Some(index) != donor)
            .map(|(_, source)| source.name.as_str()),
    ) {
        let mut fields = source
            .columns()
            .iter()
            .enumerate()
            .map(|(index, column)| (format!("\\0slot{index}"), column.kind))
            .collect::<Vec<_>>();
        if declaration == dataset {
            for (index, column) in output.columns().iter().enumerate() {
                fields[index].0 = column.name.clone();
            }
        }
        for (index, current) in current.iter().enumerate() {
            if let Some((owner, name)) = current.identifier.split_once('.') {
                if owner == declaration {
                    fields[output.columns().len() + index].0 = name.into();
                }
            }
        }
        dataset_names.push(declaration);
        dataset_fields.push(fields);
    }
    let fields = dataset_fields
        .iter()
        .map(|fields| {
            fields
                .iter()
                .map(|(name, kind)| reference_binding::Field {
                    name,
                    column_type: *kind,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let datasets = dataset_names
        .iter()
        .zip(&fields)
        .map(|(name, fields)| reference_binding::Dataset { name, fields })
        .collect::<Vec<_>>();
    let catalog = Catalog::compile(&outputs, &datasets, Default::default())
        .map_err(BindError::Catalog)?
        .with_record_derivation();
    let earlier = rebase(
        previous,
        sources,
        schemas,
        driver,
        donor,
        dataset,
        &source,
        outer_output,
        &output,
    )?;
    let references = declarations
        .iter()
        .flat_map(|item| &item.references)
        .filter_map(|name| name.split_once('.').map(|(owner, _)| owner))
        .collect::<alloc::collections::BTreeSet<_>>();
    let mut selected = Vec::new();
    for (index, item) in earlier.iter().enumerate() {
        let Some(item) = item else {
            continue;
        };
        if !references.contains(item.identifier.as_str()) {
            continue;
        }
        let schema = item.source_schema(&secondary).ok_or(BindError::Internal)?;
        let fields = schema
            .columns()
            .iter()
            .enumerate()
            .filter(|(_, field)| item.source_column(&secondary, &field.name).is_some())
            .map(|(column, field)| (field.name.clone(), (column, field.kind)))
            .collect();
        selected.push((
            item.identifier.clone(),
            reference_binding::SelectedRelation {
                index,
                fields,
                dependencies: row_intermediates::dependencies(index, &earlier)?,
            },
        ));
    }
    let mut catalog = catalog
        .with_selected(selected)
        .map_err(BindError::Catalog)?
        .with_record_derivation();
    let mut assignments = Vec::new();
    let mut available = stored_available.map_or_else(
        || (0..stored.columns().len()).collect::<Vec<_>>(),
        |columns| columns.to_vec(),
    );
    catalog.restrict_record_fields(&available);
    let contextual_names = named.for_reader(reader);
    for (index, declaration) in declarations.iter().enumerate() {
        let column = stored.columns().len() + index;
        let mut edges = Vec::new();
        let start = findings.len();
        let expression = derivations::bind_row(
            &declaration.branch.operation,
            &declaration.branch.path,
            column,
            &catalog,
            dataset,
            None,
            crate::reference_scope::Phase::Row { group_by: None },
            Some(&available),
            &mut edges,
            findings,
            false,
            &output,
            project,
            &contextual_names,
            &secondary,
            &earlier,
        )?;
        donor_scope_findings(&mut findings[start..], reader, named);
        if let Some(expression) = expression {
            assignments.push(Assignment {
                column,
                path: declaration.branch.path.clone(),
                expression,
            });
        }
        available.push(column);
        catalog.complete_record_field(column, &output.columns()[column]);
    }
    if findings.len() != before || previous.iter().any(Option::is_none) {
        return Ok(None);
    }
    let rows = vec![RowTemplate {
        mode: RowMode::Records,
        filter: None,
        assignments: stored
            .columns()
            .iter()
            .enumerate()
            .map(|(column, _)| Assignment {
                column,
                path: format!("donor.{column}"),
                expression: Expression::Source(column),
            })
            .collect(),
    }];
    let plan = DatasetPlan::new_donor(
        SourceSchemas {
            primary: source,
            secondary,
        },
        earlier
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(BindError::Internal)?,
        output,
        rows,
        assignments,
    )
    .map_err(BindError::InvalidPlan)?;
    *pending_schema = None;
    Ok(Some(IntermediateDerivations {
        plan: alloc::rc::Rc::new(plan),
        current,
        stored_columns: stored.columns().len(),
        driver_kinds,
    }))
}
/// Shared scalar binders report at the authored operand path; donor ownership
/// changes only the normative cause and its context.
fn donor_scope_findings(
    findings: &mut [BindFinding],
    reader: &str,
    named: &intermediates::Declarations,
) {
    for finding in findings {
        let reference = match finding {
            BindFinding::UnknownReference { path, name }
            | BindFinding::OutputReference { path, name, .. }
            | BindFinding::QualifiedReference { path, name, .. } => {
                Some((path.as_str(), name.as_str()))
            }
            BindFinding::ProjectFunction(function) => match &function.cause {
                functions::Cause::UnknownReference(name) => {
                    Some((function.path.as_str(), name.as_str()))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((path, reference)) = reference {
            let mut result = LookupFinding::new(
                path,
                if named.reference(reference).is_some() {
                    crate::diagnostic::ConditionCode::IntermediateDerivationWindowReference
                } else {
                    crate::diagnostic::ConditionCode::IntermediateDerivationReference
                },
            );
            result.text("intermediate", reader);
            result.text("identifier", reference);
            *finding = BindFinding::Lookup(result);
        }
    }
}
fn outer_secondary(
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
) -> Vec<SecondarySource> {
    sources
        .iter()
        .enumerate()
        .filter(|&(index, _)| index != driver)
        .map(|(index, source)| SecondarySource {
            name: source.name.clone(),
            schema: schemas[index].clone(),
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
fn rebase(
    previous: &[Option<Intermediate>],
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    donor: Option<usize>,
    donor_name: &str,
    source: &TableSchema,
    outer: &TableSchema,
    output: &TableSchema,
) -> Result<Vec<Option<Intermediate>>, BindError> {
    let mut result = previous.to_vec();
    let original_secondary = outer_secondary(sources, schemas, driver);
    let current_field = |field: &CurrentField| -> Result<CurrentField, BindError> {
        let read = match field.read {
            CurrentRead::Record {
                source: owner,
                column,
            } => {
                let authored = if owner == 0 {
                    driver
                } else {
                    sources
                        .iter()
                        .position(|source| source.name == original_secondary[owner - 1].name)
                        .ok_or(BindError::Internal)?
                };
                CurrentRead::Record {
                    source: if Some(authored) == donor {
                        0
                    } else if donor.is_none_or(|donor| authored < donor) {
                        authored + 1
                    } else {
                        authored
                    },
                    column: if Some(authored) == donor {
                        source
                            .columns()
                            .iter()
                            .position(|field| {
                                field.name == schemas[authored].columns()[column].name
                            })
                            .ok_or(BindError::Internal)?
                    } else {
                        column
                    },
                }
            }
            CurrentRead::Output(column) => CurrentRead::Output(
                output
                    .columns()
                    .iter()
                    .position(|field| field.name == outer.columns()[column].name)
                    .ok_or(BindError::Internal)?,
            ),
            CurrentRead::Intermediate { index, column } => {
                CurrentRead::Intermediate { index, column }
            }
        };
        Ok(CurrentField {
            identifier: field.identifier.clone(),
            read,
        })
    };
    for item in result.iter_mut().flatten() {
        let name = match &item.source {
            IntermediateSource::Input(index) => Some(original_secondary[*index].name.clone()),
            IntermediateSource::Primary { name, .. } => Some(name.clone()),
            IntermediateSource::SelfRows { .. } => None,
        };
        if let IntermediateSource::SelfRows { schema, .. } = &mut item.source {
            *schema = output.clone();
        }
        if let Some(name) = name {
            let index = sources
                .iter()
                .position(|source| source.name == name)
                .ok_or(BindError::Internal)?;
            item.source = if Some(index) == donor {
                IntermediateSource::Primary {
                    name,
                    schema: source.clone(),
                }
            } else {
                IntermediateSource::Input(if donor.is_none_or(|donor| index < donor) {
                    index
                } else {
                    index - 1
                })
            };
        }
        if let Some(filter) = &item.filter {
            item.filter = Some(
                crate::dataset::RelationPredicate::new(
                    filter.predicate().clone(),
                    filter
                        .current()
                        .iter()
                        .map(&current_field)
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(BindError::InvalidPredicateBinding)?,
            );
        }
        if let Some(range) = &item.range {
            item.range = Some(
                crate::dataset::RelationRange::new(
                    range.path().into(),
                    current_field(range.current())?,
                    range.value_kind(),
                    range.lower().cloned(),
                    range.upper().cloned(),
                    16_777_216,
                )
                .map_err(|_| {
                    BindError::InvalidPlan(crate::dataset::PlanError::InvalidIntermediate)
                })?,
            );
        }
        if let Some(derived) = &mut item.derivations {
            derived.current = derived
                .current
                .iter()
                .map(&current_field)
                .collect::<Result<Vec<_>, _>>()?;
        }
        for key in &mut item.keys {
            if let crate::dataset::MatchValue::Contextual(declaration) = &key.value {
                let name = declaration.identifier();
                let local = name
                    .strip_prefix(&format!("{donor_name}."))
                    .or_else(|| (!name.contains('.')).then_some(name));
                if let Some(column) = local
                    .and_then(|name| output.columns().iter().position(|field| field.name == name))
                {
                    key.value = declaration.bind(crate::dataset::MatchRead::Output {
                        column,
                        identifier: name.into(),
                    });
                }
                continue;
            }
            if let Some(read) = key.value.read().cloned() {
                let read = match read {
                    crate::dataset::MatchRead::Output { column, identifier } => {
                        let name = &outer.columns()[column].name;
                        let Some(column) = output
                            .columns()
                            .iter()
                            .position(|field| field.name == *name)
                        else {
                            key.value = crate::dataset::MatchValue::Contextual(
                                key.value.contextual().ok_or(BindError::Internal)?,
                            );
                            continue;
                        };
                        crate::dataset::MatchRead::Output { column, identifier }
                    }
                    crate::dataset::MatchRead::SecondaryRecord {
                        source: owner,
                        column,
                        identifier,
                    } => {
                        let name = &original_secondary[owner].name;
                        let authored = sources
                            .iter()
                            .position(|source| source.name == *name)
                            .ok_or(BindError::Internal)?;
                        if Some(authored) == donor {
                            crate::dataset::MatchRead::Record {
                                column: source
                                    .columns()
                                    .iter()
                                    .position(|field| {
                                        field.name == schemas[authored].columns()[column].name
                                    })
                                    .ok_or(BindError::Internal)?,
                                identifier,
                            }
                        } else {
                            crate::dataset::MatchRead::SecondaryRecord {
                                source: if donor.is_none_or(|donor| authored < donor) {
                                    authored
                                } else {
                                    authored - 1
                                },
                                column,
                                identifier,
                            }
                        }
                    }
                    crate::dataset::MatchRead::Record { column, identifier } => {
                        if Some(driver) == donor {
                            crate::dataset::MatchRead::Record { column, identifier }
                        } else {
                            crate::dataset::MatchRead::SecondaryRecord {
                                source: if donor.is_none_or(|donor| driver < donor) {
                                    driver
                                } else {
                                    driver - 1
                                },
                                column,
                                identifier,
                            }
                        }
                    }
                };
                match &mut key.value {
                    crate::dataset::MatchValue::Variable(bound)
                    | crate::dataset::MatchValue::Source(bound)
                    | crate::dataset::MatchValue::StrPad { read: bound, .. } => *bound = read,
                    _ => (),
                }
            }
        }
    }
    Ok(result)
}
