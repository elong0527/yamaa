//! Donor derivations share the scalar compiler and explicitly declared conversion.
use super::*;
use crate::dataset::{
    CurrentField, CurrentRead, Intermediate, IntermediateDerivations, IntermediateSource,
    SecondarySource, SourceSchemas,
};
use crate::{result_type::ResultType, value::Value};

#[derive(Clone, Debug)]
pub(super) struct Derivation {
    pub name: String,
    pub branch: cases::Branch,
    path: String,
    typed: Option<ColumnType>,
    handler: Option<crate::conversion::LiteralHandler>,
    pub(super) references: Vec<String>,
}
/// The operation path of the first derivation that reads `identifier`.
pub(super) fn reading_path<'a>(derivations: &'a [Derivation], identifier: &str) -> Option<&'a str> {
    derivations
        .iter()
        .find(|derivation| derivation.references.iter().any(|name| name == identifier))
        .map(|derivation| derivation.branch.path.as_str())
}
impl Derivation {
    /// The entry's path, `intermediates[i].derivations.<name>`.
    pub(super) fn path(&self) -> &str {
        &self.path
    }
    fn assignment_path(&self) -> &str {
        if self.typed.is_some() {
            &self.path
        } else {
            &self.branch.path
        }
    }
    /// The derived field's type: a typed entry's declared type, and otherwise
    /// `result`, its expression's result type (EXPR-14).
    pub(super) fn field_type(&self, result: ResultType) -> ResultType {
        self.typed.map_or(result, ResultType::column)
    }
}
/// Both admitted entry forms expose the same normalized expression boundary.
pub(super) fn value(
    d: &Document,
    entry: usize,
    path: &str,
) -> Result<(usize, String), PrepareError> {
    if let Some(derivation) = d.field(entry, "derivation") {
        Ok((field(d, derivation, "value")?, format!("{path}.derivation")))
    } else {
        Ok((field(d, entry, "value")?, path.into()))
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    d: &Document,
    id: usize,
    path: &str,
    dataset: &str,
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
    if items.len() > crate::limits::Limits::COLUMNS {
        return Err(PrepareError::limit(
            "intermediate_derivations",
            crate::limits::Limits::COLUMNS,
            Some(items.len()),
            Some(&format!("{path}.derivations")),
        ));
    }
    let mut result = Vec::new();
    for &(name, handled) in items {
        let name = text(d, name)?;
        let path = format!("{path}.derivations.{name}");
        let (value, expression_path) = value(d, handled, &path)?;
        let typed = d
            .field(handled, "type")
            .map(|id| {
                Ok(match text(d, id)? {
                    "str" => ColumnType::Str,
                    "int" => ColumnType::Int,
                    "float" => ColumnType::Float,
                    "date" => ColumnType::Date,
                    "datetime" => ColumnType::DateTime,
                    _ => return Err(PrepareError::Internal),
                })
            })
            .transpose()?;
        let branch = derivations::prepare_value_with_driver(
            d,
            value,
            &expression_path,
            project,
            named,
            true,
            0,
            extraction,
            templates,
            Some(dataset),
        )?;
        if let Some(project) = project {
            let refused = functions::donor_keep_findings(project, value, named)?;
            if !refused.is_empty() {
                return Err(PrepareError::Invalid(refused));
            }
        }
        let mut references = Vec::new();
        derivations::references(d, value, project, &mut references, 0)?;
        self_relations::template_references(d, value, templates, &mut references, 0)?;
        let handler = literal_handler(d, handled, &path)?;
        result.push(Derivation {
            name: name.into(),
            branch,
            path,
            typed,
            handler,
            references,
        });
    }
    Ok(result)
}
/// The donor layout, each derived field's record type where it has one, and
/// each derivation's expression result type for its conversion check.
type FieldKinds = (TableSchema, Vec<Option<ColumnType>>, Vec<ResultType>);

#[allow(clippy::too_many_arguments)]
fn resolve_field_kinds(
    declarations: &[Derivation],
    dataset: &str,
    reader: &str,
    pending_schema: &mut Option<TableSchema>,
    stored: &TableSchema,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    tree: &types::Tree,
    named: &intermediates::Declarations,
    previous: &[Option<Intermediate>],
    before: usize,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<FieldKinds>, BindError> {
    let scope = tree.donor(reader)?;
    // Shared earlier donor plans do not recursively copy their expression trees.
    // Admit aggregate retained schema/catalog copies before making the first copy.
    let original_secondary = outer_secondary(sources, schemas, driver);
    let mut fields = stored.columns().to_vec();
    let mut field_types = Vec::new();
    let mut operation_types = Vec::new();
    for declaration in declarations {
        let input = |name: &str| {
            let (owner, field) = name.split_once('.').unwrap_or((dataset, name));
            if owner == dataset {
                fields
                    .iter()
                    .position(|column| column.name == field)
                    .map_or(ResultType::UNKNOWN, |column| {
                        if column < stored.columns().len() {
                            fields[column].kind.into()
                        } else {
                            field_types
                                .get(column - stored.columns().len())
                                .copied()
                                .unwrap_or(ResultType::UNKNOWN)
                        }
                    })
            } else if let Some((index, field)) = named.reference(name) {
                ResultType::declared(previous.get(index).and_then(Option::as_ref).and_then(
                    |item| {
                        let column = item.source_column(&original_secondary, field)?;
                        item.source_kind(&original_secondary, column)
                    },
                ))
            } else {
                ResultType::declared(
                    sources
                        .iter()
                        .position(|source| source.name == owner)
                        .and_then(|index| {
                            schemas[index]
                                .columns()
                                .iter()
                                .find(|column| column.name == field)
                        })
                        .map(|column| column.kind),
                )
            }
        };
        let operation_type = tree.result_type(declaration.branch.value, scope, &input)?;
        let field_type = declaration.field_type(operation_type);
        if fields.iter().any(|field| field.name == declaration.name) {
            let mut finding = LookupFinding::new(
                &declaration.path,
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
            kind: field_type.layout(),
        });
        field_types.push(field_type);
        operation_types.push(operation_type);
    }
    let output = TableSchema::new(fields).map_err(|_| BindError::Internal)?;
    *pending_schema = Some(output.clone());
    if findings.len() != before {
        return Ok(None);
    }
    Ok(Some((
        output,
        field_types.iter().map(|kind| kind.column_type()).collect(),
        operation_types,
    )))
}

#[allow(clippy::too_many_arguments)]
fn collect_current_fields(
    declarations: &[Derivation],
    dataset: &str,
    named: &intermediates::Declarations,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
) -> Vec<CurrentField> {
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
    current
}

#[allow(clippy::too_many_arguments)]
fn build_catalog_context(
    dataset: &str,
    current: &[CurrentField],
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    stored: &TableSchema,
    output: &TableSchema,
    result_kinds: &[Option<ColumnType>],
) -> Result<(Option<usize>, Vec<SecondarySource>, TableSchema, Catalog), BindError> {
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
    for declaration in std::iter::once(dataset).chain(
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
    let catalog = Catalog::compile(&outputs, &datasets)
        .map_err(BindError::Catalog)?
        .with_record_derivation()
        .with_record_kinds(
            stored
                .columns()
                .iter()
                .map(|column| Some(column.kind))
                .chain(result_kinds.iter().copied())
                .collect(),
        );
    Ok((donor, secondary, source, catalog))
}

type EarlierSelection = (
    Vec<Option<Intermediate>>,
    std::collections::BTreeMap<usize, String>,
    Catalog,
);

#[allow(clippy::too_many_arguments)]
fn select_earlier(
    declarations: &[Derivation],
    previous: &[Option<Intermediate>],
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    donor: Option<usize>,
    dataset: &str,
    source: &TableSchema,
    outer_output: &TableSchema,
    output: &TableSchema,
    tree: &types::Tree,
    named: &intermediates::Declarations,
    project: Option<&crate::project_calls::ProjectCalls>,
    catalog: Catalog,
    secondary: &[SecondarySource],
    findings: &mut Vec<BindFinding>,
) -> Result<EarlierSelection, BindError> {
    let references = declarations
        .iter()
        .flat_map(|item| &item.references)
        .filter_map(|name| name.split_once('.').map(|(owner, _)| owner))
        .collect::<std::collections::BTreeSet<_>>();
    let mut required = previous
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item.as_ref()
                .filter(|item| references.contains(item.identifier.as_str()))
                .map(|_| index)
        })
        .collect::<std::collections::BTreeSet<_>>();
    // A referenced selector may itself read an earlier selector. Retain its
    // transitive current-record dependencies, preserving their original slots.
    for index in (0..previous.len()).rev() {
        if !required.contains(&index) {
            continue;
        }
        for field in previous[index]
            .iter()
            .flat_map(Intermediate::current_fields)
        {
            if let CurrentRead::Intermediate { index: child, .. } = field.read {
                if child >= index {
                    return Err(BindError::Internal);
                }
                required.insert(child);
            }
        }
    }
    let mut unavailable = BTreeMap::new();
    let earlier = rebase(
        previous,
        &required,
        &mut unavailable,
        sources,
        schemas,
        driver,
        donor,
        dataset,
        source,
        outer_output,
        output,
        tree,
        named,
        project,
        &catalog,
        secondary,
        findings,
    )?;
    let mut selected = Vec::new();
    for (index, item) in earlier.iter().enumerate() {
        let Some(item) = item else {
            continue;
        };
        if !references.contains(item.identifier.as_str()) {
            continue;
        }
        let schema = item.source_schema(secondary).ok_or(BindError::Internal)?;
        let fields = schema
            .columns()
            .iter()
            .enumerate()
            .filter(|(_, field)| item.source_column(secondary, &field.name).is_some())
            .map(|(column, field)| {
                (
                    field.name.clone(),
                    (column, item.source_kind(secondary, column)),
                )
            })
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
    let catalog = catalog
        .with_selected(selected)
        .map_err(BindError::Catalog)?
        .with_record_derivation();
    Ok((earlier, unavailable, catalog))
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
    tree: &types::Tree,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<IntermediateDerivations>, BindError> {
    if declarations.is_empty() {
        return Ok(None);
    }
    let before = findings.len();
    let Some((output, result_kinds, operation_types)) = resolve_field_kinds(
        declarations,
        dataset,
        reader,
        pending_schema,
        stored,
        sources,
        schemas,
        driver,
        tree,
        named,
        previous,
        before,
        findings,
    )?
    else {
        return Ok(None);
    };
    let current = collect_current_fields(declarations, dataset, named, sources, schemas, driver);
    let (donor, secondary, source, catalog) = build_catalog_context(
        dataset,
        &current,
        sources,
        schemas,
        driver,
        stored,
        &output,
        &result_kinds,
    )?;
    let (earlier, unavailable, mut catalog) = select_earlier(
        declarations,
        previous,
        sources,
        schemas,
        driver,
        donor,
        dataset,
        &source,
        outer_output,
        &output,
        tree,
        named,
        project,
        catalog,
        &secondary,
        findings,
    )?;
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
        let expression = BindContext {
            scope: binder::Scope {
                template: binder::Template::Any,
                phase: crate::reference_scope::Phase::Row { group_by: None },
                driver: dataset,
                available: Some(&available),
            },
            donors: binder::Donors {
                named: &contextual_names,
                secondary: &secondary,
                items: &earlier,
            },
            catalog: &catalog,
            output: &output,
            project,
            tree,
            edges: &mut edges,
            findings: &mut *findings,
            reported: false,
        }
        .bind(
            &declaration.branch.operation,
            &declaration.branch.path,
            column,
        )?;
        donor_scope_findings(&mut findings[start..], reader, named, &unavailable);
        if let Some(finding) = declaration
            .typed
            .filter(|_| expression.is_some() && declaration.handler.is_none())
            .and_then(|typed| {
                types::conversion_finding(&declaration.path, operation_types[index], typed)
            })
        {
            findings.push(BindFinding::Scalar(finding));
        }
        if let Some(expression) = expression {
            assignments.push(Assignment {
                column,
                path: declaration.assignment_path().into(),
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
    let handlers = declarations
        .iter()
        .filter_map(|declaration| {
            declaration
                .handler
                .as_ref()
                .map(|handler| crate::dataset::ConversionHandler {
                    assignment_path: declaration.assignment_path().into(),
                    handler: handler.clone(),
                })
        })
        .collect();
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
        stored
            .columns()
            .iter()
            .map(|column| Some(column.kind))
            .chain(result_kinds)
            .collect(),
    )
    .with_conversion_handlers(handlers);
    *pending_schema = None;
    Ok(Some(IntermediateDerivations {
        plan: std::sync::Arc::new(plan),
        current,
        stored_columns: stored.columns().len(),
        typed_columns: declarations
            .iter()
            .enumerate()
            .filter_map(|(index, declaration)| {
                declaration.typed.map(|_| stored.columns().len() + index)
            })
            .collect(),
    }))
}
/// Shared scalar binders report at the authored operand path; donor ownership
/// changes only the normative cause and its context.
fn donor_scope_findings(
    findings: &mut [BindFinding],
    reader: &str,
    named: &intermediates::Declarations,
    unavailable: &BTreeMap<usize, String>,
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
            if let Some((index, _)) = named.reference(reference) {
                if let Some(identifier) = unavailable.get(&index) {
                    let mut result = LookupFinding::new(
                        path,
                        crate::diagnostic::ConditionCode::IntermediateDerivationDependency,
                    );
                    result.text("intermediate", reader);
                    result.text("identifier", identifier);
                    result.text(
                        "read",
                        reference
                            .split_once('.')
                            .map_or(reference, |(owner, _)| owner),
                    );
                    *finding = BindFinding::Lookup(result);
                    continue;
                }
            }
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
fn unused_slot_placeholder(
    identifier: &str,
    path: &str,
    donor_name: &str,
    source: &TableSchema,
) -> Intermediate {
    Intermediate {
        identifier: identifier.into(),
        path: path.into(),
        source: IntermediateSource::Primary {
            name: donor_name.into(),
            schema: source.clone(),
        },
        keys: vec![crate::dataset::IntermediateKey {
            source_column: 0,
            path: format!("{path}.key"),
            value: crate::dataset::MatchValue::Literal(Value::Missing),
        }],
        filter: None,
        selection: None,
        range: None,
        columns: None,
        no_match: Some(Value::Missing),
        require_match: false,
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn rebase(
    previous: &[Option<Intermediate>],
    required: &std::collections::BTreeSet<usize>,
    unavailable: &mut BTreeMap<usize, String>,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    donor: Option<usize>,
    donor_name: &str,
    source: &TableSchema,
    outer: &TableSchema,
    output: &TableSchema,
    tree: &types::Tree,
    named: &intermediates::Declarations,
    project: Option<&crate::project_calls::ProjectCalls>,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    findings: &mut Vec<BindFinding>,
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
    for (index, slot) in result.iter_mut().enumerate() {
        let Some(item) = slot else {
            continue;
        };
        if !required.contains(&index) {
            // Donor plans keep original selector indices. An unused slot owns
            // no matching work, derivations or verifications in this context.
            let placeholder =
                unused_slot_placeholder(&item.identifier, &item.path, donor_name, source);
            *item = placeholder;
            continue;
        }
        // Retain field names before moving this selector's source into the
        // donor catalog, whose secondary coordinates differ from the outer plan.
        let donor_schema = item
            .source_schema(&original_secondary)
            .ok_or(BindError::Internal)?
            .clone();
        let missing = item.current_fields().find_map(|field| match field.read {
            CurrentRead::Output(column) => (!output
                .columns()
                .iter()
                .any(|field| field.name == outer.columns()[column].name))
            .then(|| field.identifier.clone()),
            CurrentRead::Intermediate { index, .. } => unavailable.get(&index).cloned(),
            CurrentRead::Record { .. } => None,
        });
        if let Some(identifier) = missing {
            // Retain the unavailable name until its actual read is bound, so
            // the finding belongs to the reading derivation's operand path.
            unavailable.insert(index, identifier);
            *slot = None;
            continue;
        }
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
            if matches!(
                key.value,
                crate::dataset::MatchValue::Expression(_)
                    | crate::dataset::MatchValue::Contextual(
                        crate::dataset::ContextualMatch::Expression { .. }
                    )
            ) {
                let field = &donor_schema.columns()[key.source_column].name;
                let declaration = named
                    .match_declaration(&item.identifier, field)
                    .ok_or(BindError::Internal)?;
                let bound = declaration.bind_deferred(
                    &key.path,
                    &item.identifier,
                    catalog,
                    findings,
                    match_values::Context {
                        output,
                        driver: donor_name,
                        row: None,
                        phase: crate::reference_scope::Phase::Row { group_by: None },
                        project,
                        tree,
                        named,
                        secondary,
                        intermediates: previous,
                    },
                )?;
                // A declaration finding already owns this refusal. Do not
                // invent an additional missing-dependency finding for a known
                // operand whose scalar operation rejected its type.
                key.value = bound.unwrap_or(crate::dataset::MatchValue::Literal(Value::Missing));
                continue;
            }
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
            let fallback = key.value.contextual();
            let mut missing = None;
            for bound in key.value.reads_mut() {
                let read = bound.clone();
                let read = match read {
                    crate::dataset::MatchRead::Output { column, identifier } => {
                        let name = &outer.columns()[column].name;
                        let Some(column) = output
                            .columns()
                            .iter()
                            .position(|field| field.name == *name)
                        else {
                            missing = Some(identifier);
                            break;
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
                *bound = read;
            }
            if let Some(identifier) = missing {
                key.value = crate::dataset::MatchValue::Contextual(
                    fallback.unwrap_or(crate::dataset::ContextualMatch::Source(identifier)),
                );
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod inference_tests {
    use super::*;
    use crate::{expression::Resolver, numeric::Number, value::Selection};
    use std::convert::Infallible;

    struct Inputs {
        integer: Value,
        float: Value,
    }
    impl Resolver for Inputs {
        type Error = Infallible;
        fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
            Ok(Selection::Present(match name {
                "INT_COL" => self.integer.clone(),
                "FLOAT_COL" => self.float.clone(),
                _ => panic!("unexpected numeric operand"),
            }))
        }
    }

    #[test]
    fn nullif_over_an_int_and_a_float_is_a_float_even_when_the_float_is_missing() {
        let expression = "NULLIF(INT_COL, FLOAT_COL)";
        let compiled = crate::expression::compile(
            expression,
            crate::expression::Context::Compute,
            "donor.compute",
        )
        .unwrap();
        let types = |name: &str| match name {
            "INT_COL" => ResultType::INT,
            "FLOAT_COL" => ResultType::FLOAT,
            _ => ResultType::UNKNOWN,
        };
        let kind = compiled.result_type(types);
        assert_eq!(kind, ResultType::FLOAT, "VALUE-10: float when either is");
        assert_eq!(kind.column_type(), Some(ColumnType::Float));
        // The bound site widens the int it returns, so the value has the type.
        let plan = compiled.typed(types);
        for (integer, float, expected) in [
            (Value::Int(7), Value::Missing, Number::float(7.0)),
            (Value::Int(7), Value::float(2.5), Number::float(7.0)),
            (Value::Int(7), Value::float(7.0), Number::Missing),
            (
                Value::Int(9_007_199_254_740_993),
                Value::Missing,
                Number::float(9_007_199_254_740_992.0),
            ),
            (Value::Missing, Value::Missing, Number::Missing),
        ] {
            assert_eq!(
                plan.evaluate_number(&mut Inputs { integer, float })
                    .unwrap(),
                expected
            );
        }
    }
}
