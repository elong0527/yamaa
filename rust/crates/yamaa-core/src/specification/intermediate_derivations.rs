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
    /// The derivation's expression and the path of its operation.
    pub expr: ExprId,
    operation_path: String,
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
        .map(|derivation| derivation.operation_path.as_str())
}
impl Derivation {
    /// The declared type of a typed entry.
    pub(super) fn typed(&self) -> Option<ColumnType> {
        self.typed
    }
    /// The entry's path, at which a conversion of its result is named.
    pub(super) fn path(&self) -> &str {
        &self.path
    }
    /// The literal answering a result its declared type cannot hold.
    pub(super) fn handler(&self) -> Option<&crate::conversion::LiteralHandler> {
        self.handler.as_ref()
    }
    pub(super) fn assignment_path(&self) -> &str {
        if self.typed.is_some() {
            &self.path
        } else {
            &self.operation_path
        }
    }
    /// The derived field's type: a typed entry's declared type, and otherwise
    /// `result`, its expression's result type (EXPR-14).
    pub(super) fn field_type(&self, result: ResultType) -> ResultType {
        self.typed.map_or(result, ResultType::column)
    }
}
/// Each derivation of the intermediate `item`, admitted in written order:
/// its expression as any derivation's, the named intermediates a project
/// call's argument reads, the names it reads and its handler. `admission`
/// holds the intermediates derived before this one.
pub(super) fn prepare(
    spec: &ast::Spec,
    item: &ast::Intermediate,
    admission: &derivations::Admission<'_>,
) -> Result<Vec<Derivation>, PrepareError> {
    if item.bindings.is_empty() {
        return Ok(Vec::new());
    }
    if item.bindings.len() > crate::limits::Limits::COLUMNS {
        return Err(PrepareError::limit(
            "intermediate_derivations",
            crate::limits::Limits::COLUMNS,
            Some(item.bindings.len()),
            Some(&format!("{}.derivations", checks::render(spec, item.meta))),
        ));
    }
    let mut result = Vec::new();
    for binding in &item.bindings {
        let expr = binding.derivation.expr;
        admission.admit(spec, expr, 0)?;
        if let Some(project) = admission.project {
            let refused = functions::donor_keep_findings(project, expr, admission.intermediates)?;
            if !refused.is_empty() {
                return Err(PrepareError::Invalid(refused));
            }
        }
        let scan = reads::Scan {
            relation: true,
            windows: true,
        };
        let references = reads::names(spec, expr, scan, admission.project.is_some())?;
        let handler = binding
            .derivation
            .unconvertible
            .as_ref()
            .map(|handler| {
                let spec_path = checks::render(spec, handler.meta);
                Ok(crate::conversion::LiteralHandler {
                    value: constant(&handler.value, &spec_path)?,
                    spec_path,
                })
            })
            .transpose()?;
        result.push(Derivation {
            name: binding.name.name.clone(),
            expr,
            operation_path: checks::path(spec, expr),
            path: checks::render(spec, binding.meta),
            typed: binding.declared,
            handler,
            references,
        });
    }
    Ok(result)
}
/// The donor layout, each derived field's result type, which may be decided
/// per value (EXPR-14), and each derivation's expression result type for its
/// conversion check.
type FieldKinds = (TableSchema, Vec<ResultType>, Vec<ResultType>);

/// A lookup's derivations as they bind: the lookup's name, its derivations
/// in written order, and the records they extend: the dataset's name (`SELF`
/// for the output rows), its stored schema, and the columns of it a SELF
/// lookup reads (`None`: every stored column).
#[derive(Clone, Copy)]
pub(super) struct Lookup<'a> {
    pub reader: &'a str,
    pub declarations: &'a [Derivation],
    pub dataset: &'a str,
    pub stored: &'a TableSchema,
    /// Each stored field's kind, where the schema source decides it.
    pub stored_kinds: &'a [Option<ColumnType>],
    pub stored_available: Option<&'a [usize]>,
}

/// The plan a lookup's derivations bind in: the input it reads (`None` for
/// SELF) and that dataset's name, its ports (the stored fields with each
/// current read after them, then every other input), and its output, the
/// stored fields with each derived field.
#[derive(Clone, Copy)]
struct DonorPlan<'a> {
    donor: Option<usize>,
    name: &'a str,
    source: &'a TableSchema,
    secondary: &'a [SecondarySource],
    output: &'a TableSchema,
    record_types: &'a [ResultType],
}

fn resolve_field_kinds(
    cx: intermediates::Context<'_>,
    lookup: Lookup<'_>,
    pending_schema: &mut Option<TableSchema>,
    before: usize,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<FieldKinds>, BindError> {
    let intermediates::Context {
        named,
        inputs,
        tree,
        previous,
        ..
    } = cx;
    let Lookup {
        reader,
        declarations,
        dataset,
        stored,
        stored_kinds,
        ..
    } = lookup;
    let scope = tree.donor(reader)?;
    // Shared earlier donor plans do not recursively copy their expression trees.
    // Admit aggregate retained schema/catalog copies before making the first copy.
    let original_secondary = inputs.secondary();
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
                            ResultType::declared(stored_kinds[column])
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
                        inputs.source_kind(item, &original_secondary, column)
                    },
                ))
            } else {
                ResultType::declared(
                    inputs
                        .schema(owner)
                        .and_then(|schema| schema.kind(schema.position(field)?)),
                )
            }
        };
        let operation_type = tree.result_type(declaration.expr, scope, &input);
        let field_type = declaration.field_type(operation_type);
        if fields.iter().any(|field| field.name == declaration.name) {
            let mut finding = LookupFinding::new(
                &declaration.path,
                crate::diagnostic::ConditionCode::IntermediateDerivationDuplicate,
            );
            finding.text("identifier", &format!("{dataset}.{}", declaration.name));
            finding.text("intermediate", reader);
            finding.text("derivation", &declaration.name);
            // A donor field only read, which the input may not store, leaves
            // the repetition undecided.
            let finding = BindFinding::Lookup(finding);
            findings.push(
                if inputs.decides_through(named, dataset, &declaration.name) {
                    finding
                } else {
                    BindFinding::Withheld(std::boxed::Box::new(finding))
                },
            );
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
    Ok(Some((output, field_types, operation_types)))
}

fn collect_current_fields(cx: intermediates::Context<'_>, lookup: Lookup<'_>) -> Vec<CurrentField> {
    let named = cx.named;
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = cx.inputs;
    let Lookup {
        declarations,
        dataset,
        ..
    } = lookup;
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
                .then(|| schemas[index].position(field).map(|column| (index, column)))
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

fn build_catalog_context(
    inputs: binder::Inputs<'_>,
    lookup: Lookup<'_>,
    current: &[CurrentField],
    output: &TableSchema,
    record_types: &[ResultType],
) -> Result<(Option<usize>, Vec<SecondarySource>, TableSchema, Catalog), BindError> {
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = inputs;
    let Lookup { dataset, .. } = lookup;
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
            schema: schemas[index].layout().clone(),
        })
        .collect::<Vec<_>>();
    // The record's kinds: each stored field's, each derived field's one
    // type, then each current read's.
    let mut source_kinds = record_types
        .iter()
        .map(|kind| kind.column_type())
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
            kind: schemas[original].layout().columns()[column].kind,
        });
        source_kinds.push(schemas[original].kind(column));
    }
    let source = TableSchema::new(source_fields).map_err(|_| BindError::Internal)?;
    let outputs = output
        .columns()
        .iter()
        .map(|column| reference_binding::Field {
            name: &column.name,
            column_type: Some(column.kind),
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
            .map(|(index, _)| (format!("\\0slot{index}"), source_kinds[index]))
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
        .with_record_types(record_types.to_vec());
    Ok((donor, secondary, source, catalog))
}

type EarlierSelection = (
    Vec<Option<Intermediate>>,
    std::collections::BTreeMap<usize, Unavailable>,
    Catalog,
);

/// The name a lookup reads that a donor record cannot answer, and whether
/// the schemas decide that it cannot: a field the donor's input may store,
/// which its schema leaves open, does not (#3805).
#[derive(Clone, Debug)]
struct Unavailable {
    identifier: String,
    decided: bool,
}

fn select_earlier(
    cx: intermediates::Context<'_>,
    lookup: Lookup<'_>,
    plan: DonorPlan<'_>,
    catalog: Catalog,
    findings: &mut Vec<BindFinding>,
) -> Result<EarlierSelection, BindError> {
    let (previous, declarations, secondary) = (cx.previous, lookup.declarations, plan.secondary);
    let references = declarations
        .iter()
        .flat_map(|item| &item.references)
        .filter_map(|name| name.split_once('.').map(|(owner, _)| owner))
        .collect::<std::collections::BTreeSet<_>>();
    // A pair relation is read as an input is, never through its
    // intermediate (LOOKUP-27): a name it qualifies is a current-record read.
    let pairs = previous
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.as_ref()
                .is_some_and(|item| item.source.pairs().is_some())
        })
        .map(|(index, _)| index)
        .collect::<std::collections::BTreeSet<_>>();
    let mut required = previous
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item.as_ref()
                .filter(|item| references.contains(item.identifier.as_str()))
                .filter(|_| !pairs.contains(&index))
                .map(|_| index)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let direct = required.clone();
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
        cx,
        plan,
        &required,
        &direct,
        &mut unavailable,
        &catalog,
        findings,
    )?;
    let mut selected = Vec::new();
    for (index, item) in earlier.iter().enumerate() {
        let Some(item) = item else {
            continue;
        };
        if !references.contains(item.identifier.as_str()) || pairs.contains(&index) {
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
                    (column, cx.inputs.source_kind(item, secondary, column)),
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

/// Bind a lookup's derivations over its donor's records, each in written
/// order and reading those before it, into the donor plan the lookup reads
/// (LOOKUP-16). `pending_schema` keeps the derived layout while a finding
/// refuses the plan.
pub(super) fn bind(
    cx: intermediates::Context<'_>,
    lookup: Lookup<'_>,
    pending_schema: &mut Option<TableSchema>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<IntermediateDerivations>, BindError> {
    let intermediates::Context {
        named,
        project,
        tree,
        previous,
        ..
    } = cx;
    let Lookup {
        reader,
        declarations,
        dataset,
        stored,
        stored_available,
        ..
    } = lookup;
    if declarations.is_empty() {
        return Ok(None);
    }
    let before = findings.len();
    let Some((output, field_types, operation_types)) =
        resolve_field_kinds(cx, lookup, pending_schema, before, findings)?
    else {
        return Ok(None);
    };
    // Each field's result type: each stored field's kind, then each
    // derived field's type.
    let record_types = lookup
        .stored_kinds
        .iter()
        .map(|&kind| ResultType::declared(kind))
        .chain(field_types)
        .collect::<Vec<_>>();
    let current = collect_current_fields(cx, lookup);
    let (donor, secondary, source, catalog) =
        build_catalog_context(cx.inputs, lookup, &current, &output, &record_types)?;
    let plan = DonorPlan {
        donor,
        name: dataset,
        source: &source,
        secondary: &secondary,
        output: &output,
        record_types: &record_types,
    };
    let (earlier, unavailable, mut catalog) = select_earlier(cx, lookup, plan, catalog, findings)?;
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
                inputs: cx.inputs,
            },
            catalog: &catalog,
            output: &output,
            project,
            tree,
            edges: &mut edges,
            findings: &mut *findings,
            reported: false,
        }
        .bind(declaration.expr, column)?;
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
        record_types,
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
    unavailable: &BTreeMap<usize, Unavailable>,
) {
    for finding in findings {
        // The donor's own cause replaces a withheld one, whose wording it
        // does not keep, so it is decided.
        if matches!(finding, BindFinding::Withheld(inner) if matches!(
            **inner,
            BindFinding::UnknownReference { .. }
                | BindFinding::OutputReference { .. }
                | BindFinding::QualifiedReference { .. }
        )) {
            finding.release();
        }
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
                if let Some(missing) = unavailable.get(&index) {
                    let mut result = LookupFinding::new(
                        path,
                        crate::diagnostic::ConditionCode::IntermediateDerivationDependency,
                    );
                    result.text("intermediate", reader);
                    result.text("identifier", &missing.identifier);
                    result.text(
                        "read",
                        reference
                            .split_once('.')
                            .map_or(reference, |(owner, _)| owner),
                    );
                    let result = BindFinding::Lookup(result);
                    *finding = if missing.decided {
                        result
                    } else {
                        BindFinding::Withheld(std::boxed::Box::new(result))
                    };
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
/// The slot of `item` in a donor plan that never reads it: its identifier,
/// path and absence handler, and no matching work.
fn unused_slot_placeholder(
    item: &Intermediate,
    donor_name: &str,
    source: &TableSchema,
) -> Intermediate {
    let path = &item.path;
    Intermediate {
        identifier: item.identifier.clone(),
        path: path.clone(),
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
        nearest: None,
        range: None,
        columns: None,
        no_match: Some(Value::Missing),
        require_match: false,
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,
    }
}

/// The first key value of `item`, a lookup that a range value reads for the
/// donor record, or its nearest reference, that reads no field of that
/// record (LOOKUP-16, LOOKUP-26): a field of another input's record, or an
/// output column or contextual name the donor record does not store.
fn chained_key(
    item: &Intermediate,
    donor: Option<usize>,
    driver: usize,
    donor_name: &str,
    authored: &dyn Fn(usize) -> Result<usize, BindError>,
    donor_field: &dyn Fn(&str) -> Option<usize>,
    outer: &TableSchema,
) -> Option<String> {
    let reference = item.nearest.iter().map(|nearest| &nearest.reference);
    item.keys.iter().chain(reference).find_map(|key| {
        if let crate::dataset::MatchValue::Contextual(declaration) = &key.value {
            let name = declaration.identifier();
            let local = name
                .strip_prefix(&format!("{donor_name}."))
                .or_else(|| (!name.contains('.')).then_some(name));
            return local
                .and_then(donor_field)
                .is_none()
                .then(|| name.to_owned());
        }
        key.value.reads().find_map(|read| {
            let readable = match read {
                crate::dataset::MatchRead::Output { column, .. } => {
                    donor_field(&outer.columns()[*column].name).is_some()
                }
                crate::dataset::MatchRead::SecondaryRecord { source, .. } => {
                    authored(source + 1).is_ok_and(|authored| Some(authored) == donor)
                }
                crate::dataset::MatchRead::Record { .. } => Some(driver) == donor,
            };
            (!readable).then(|| read.identifier().to_owned())
        })
    })
}

/// A predicate's contextual names now have donor types. Reuse initial
/// binding's type judgement, preserving deferred key-cardinality precedence.
fn rebase_filter(
    filter: &crate::dataset::RelationPredicate,
    current: Vec<CurrentField>,
    donor_kinds: &[Option<ColumnType>],
    record_types: &[ResultType],
    static_types: bool,
    findings: &mut Vec<BindFinding>,
) -> Result<crate::dataset::RelationPredicate, BindError> {
    let (predicate, deferred) = if filter
        .current()
        .iter()
        .any(|field| field.read == CurrentRead::Contextual)
    {
        let kind = |name: &str| {
            let binding = filter
                .bindings()
                .iter()
                .find(|binding| binding.name == name)?;
            match binding.read {
                Read::Source(column) => donor_kinds.get(column).copied().flatten().map(Into::into),
                Read::Column(slot) => match current.get(slot)?.read {
                    CurrentRead::Output(column) | CurrentRead::Record { source: 0, column } => {
                        record_types.get(column)?.single()
                    }
                    _ => None,
                },
                _ => None,
            }
        };
        let (typed, deferred) =
            relation_predicates::typed(filter.plan(), kind, static_types, findings);
        (
            crate::bound_expression::BoundPredicate::new(typed, filter.bindings().to_vec())
                .map_err(BindError::InvalidPredicateBinding)?,
            deferred,
        )
    } else {
        (
            filter.predicate().clone(),
            filter.deferred_type_findings().to_vec(),
        )
    };
    Ok(crate::dataset::RelationPredicate::new(predicate, current)
        .map_err(BindError::InvalidPredicateBinding)?
        .with_deferred_type_findings(deferred))
}

/// A deferred range gains the reading donor's known value type before it
/// is admitted against its bounds, by the range's shared constructor.
fn rebase_range(
    range: &crate::dataset::RelationRange,
    current: CurrentField,
    record_types: &[ResultType],
    intermediate: &str,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<crate::dataset::RelationRange>, BindError> {
    let kind = if range.current().read == CurrentRead::Contextual {
        match current.read {
            CurrentRead::Output(column) => record_types.get(column).and_then(|kind| kind.single()),
            _ => range.value_kind(),
        }
    } else {
        range.value_kind()
    };
    match crate::dataset::RelationRange::new(
        range.path().into(),
        current,
        kind,
        range.lower().cloned(),
        range.upper().cloned(),
        range.closed(),
    ) {
        Ok(range) => Ok(Some(range)),
        Err(crate::dataset::RelationRangeError::Incomparable(types)) => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                types.diagnostic(range.path(), intermediate),
            )));
            Ok(None)
        }
        Err(_) => Err(BindError::InvalidPlan(
            crate::dataset::PlanError::InvalidIntermediate,
        )),
    }
}

/// The lookups bound before this one, as its donor plan reads them: each
/// one the derivations need keeps its slot, its reads rebased onto the
/// donor plan's ports ([`rebase_keys`]); one they do not need keeps a
/// placeholder; and one whose reads the donor record cannot answer is
/// unavailable, with the name it cannot read.
fn rebase(
    cx: intermediates::Context<'_>,
    plan: DonorPlan<'_>,
    required: &std::collections::BTreeSet<usize>,
    direct: &std::collections::BTreeSet<usize>,
    unavailable: &mut BTreeMap<usize, Unavailable>,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
) -> Result<Vec<Option<Intermediate>>, BindError> {
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = cx.inputs;
    let (previous, outer) = (cx.previous, cx.output);
    let DonorPlan {
        donor,
        name: donor_name,
        source,
        output,
        ..
    } = plan;
    let mut result = previous.to_vec();
    let original_secondary = cx.inputs.secondary();
    // The authored input whose record a current read at record port `owner`
    // reads in the outer plan.
    let authored = |owner: usize| -> Result<usize, BindError> {
        if owner == 0 {
            Ok(driver)
        } else {
            sources
                .iter()
                .position(|source| source.name == original_secondary[owner - 1].name)
                .ok_or(BindError::Internal)
        }
    };
    let donor_field = |name: &str| {
        let name = name
            .strip_prefix(&format!("{donor_name}."))
            .or_else(|| (!name.contains('.')).then_some(name))?;
        output.columns().iter().position(|field| field.name == name)
    };
    // Whether `identifier`, written bare or qualified by the donor, names a
    // field of the donor's input that its schema leaves open.
    let open = |identifier: &str| {
        let field = identifier
            .strip_prefix(&format!("{donor_name}."))
            .or_else(|| (!identifier.contains('.')).then_some(identifier));
        field
            .zip(donor.and_then(|donor| schemas.get(donor)))
            .is_some_and(|(field, schema)| schema.stores(field).is_none())
    };
    let current_field = |field: &CurrentField| -> Result<CurrentField, BindError> {
        let read = match field.read {
            CurrentRead::Record {
                source: owner,
                column,
            } => {
                let authored = authored(owner)?;
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
                                field.name == schemas[authored].layout().columns()[column].name
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
            // A contextual name reads the donor record, bare or qualified.
            CurrentRead::Contextual => {
                CurrentRead::Output(donor_field(&field.identifier).ok_or(BindError::Internal)?)
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
            let placeholder = unused_slot_placeholder(item, donor_name, source);
            *item = placeholder;
            continue;
        }
        // A deferred filter reference is judged against the reading donor,
        // and reports at the filter that declared it. Keep the selector's
        // schema so this cause does not become an unknown selected field.
        if let Some(filter) = &item.filter {
            let mut failed = false;
            for field in filter.current() {
                if field.read == CurrentRead::Contextual && donor_field(&field.identifier).is_none()
                {
                    let finding = BindFinding::Lookup(LookupFinding::reference(
                        filter.plan().spec_path(),
                        &field.identifier,
                        Some(&item.identifier),
                        ReferenceCause::Selection,
                        None,
                    ));
                    findings.push(if open(&field.identifier) {
                        BindFinding::Withheld(Box::new(finding))
                    } else {
                        finding
                    });
                    failed = true;
                }
            }
            if failed {
                item.filter = None;
            }
        }
        // Retain field names before moving this selector's source into the
        // donor catalog, whose secondary coordinates differ from the outer plan.
        let donor_schema = item
            .source_schema(&original_secondary)
            .ok_or(BindError::Internal)?
            .clone();
        let decided = |identifier: String| Unavailable {
            decided: !open(&identifier),
            identifier,
        };
        let missing = item
            .current_fields()
            .find_map(|field| match field.read {
                CurrentRead::Output(column) => (!output
                    .columns()
                    .iter()
                    .any(|field| field.name == outer.columns()[column].name))
                .then(|| decided(field.identifier.clone())),
                CurrentRead::Intermediate { index, .. } => unavailable.get(&index).cloned(),
                // A donor read gives the donor record alone (LOOKUP-16): the
                // record of any other input names no value of it.
                CurrentRead::Record { source: owner, .. } => authored(owner)
                    .is_ok_and(|authored| Some(authored) != donor)
                    .then(|| Unavailable {
                        identifier: field.identifier.clone(),
                        decided: true,
                    }),
                CurrentRead::Contextual => donor_field(&field.identifier)
                    .is_none()
                    .then(|| decided(field.identifier.clone())),
            })
            .or_else(|| {
                // A lookup a range value reads runs for the donor record too,
                // so its key values read that record alone. The read of a
                // lookup a derivation reads refuses its own key values.
                (!direct.contains(&index))
                    .then(|| {
                        chained_key(
                            item,
                            donor,
                            driver,
                            donor_name,
                            &authored,
                            &donor_field,
                            outer,
                        )
                    })
                    .flatten()
                    .map(decided)
            });
        if let Some(identifier) = missing {
            // Retain the unavailable name until its actual read is bound, so
            // the finding belongs to the reading derivation's operand path.
            unavailable.insert(index, identifier);
            *slot = None;
            continue;
        }
        let donor_kinds = donor_schema
            .columns()
            .iter()
            .enumerate()
            .map(|(column, _)| cx.inputs.source_kind(item, &original_secondary, column))
            .collect::<Vec<_>>();
        let name = match &item.source {
            IntermediateSource::Input(index) => Some(original_secondary[*index].name.clone()),
            IntermediateSource::Primary { name, .. } => Some(name.clone()),
            IntermediateSource::SelfRows { .. } | IntermediateSource::Pairs(_) => None,
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
            item.filter = Some(rebase_filter(
                filter,
                filter
                    .current()
                    .iter()
                    .map(&current_field)
                    .collect::<Result<Vec<_>, _>>()?,
                &donor_kinds,
                plan.record_types,
                relation_predicates::static_filter_types(&item.keys),
                findings,
            )?);
        }
        if let Some(range) = &item.range {
            item.range = rebase_range(
                range,
                current_field(range.current())?,
                plan.record_types,
                &item.identifier,
                findings,
            )?;
        }
        if let Some(derived) = &mut item.derivations {
            derived.current = derived
                .current
                .iter()
                .map(&current_field)
                .collect::<Result<Vec<_>, _>>()?;
        }
        rebase_keys(
            cx,
            plan,
            &original_secondary,
            item,
            &donor_schema,
            catalog,
            findings,
        )?;
    }
    Ok(result)
}

/// The key values and nearest reference of a lookup the donor plan reads,
/// rebased onto the donor plan's ports: a value the outer plan binds as an
/// expression binds again over the donor record, deferred; a contextual
/// name reads the donor record's field of that name; and each other read
/// moves to the port of its record in the donor plan, or stays contextual
/// where the donor plan does not hold it.
fn rebase_keys(
    cx: intermediates::Context<'_>,
    plan: DonorPlan<'_>,
    original_secondary: &[SecondarySource],
    item: &mut Intermediate,
    donor_schema: &TableSchema,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    let intermediates::Context {
        named,
        inputs,
        output: outer,
        project,
        tree,
        previous,
        ..
    } = cx;
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = inputs;
    let DonorPlan {
        donor,
        name: donor_name,
        source,
        secondary,
        output,
        ..
    } = plan;
    // The reference of a nearest selection reads the donor record as a
    // key's match value does (LOOKUP-26), after the keys.
    let reference = item.nearest.as_mut().map(|nearest| &mut nearest.reference);
    let keys = item.keys.iter_mut().map(|key| (key, false));
    for (key, reference) in keys.chain(reference.map(|key| (key, true))) {
        if matches!(
            key.value,
            crate::dataset::MatchValue::Expression(_)
                | crate::dataset::MatchValue::Contextual(
                    crate::dataset::ContextualMatch::Expression { .. }
                )
        ) {
            let declaration = if reference {
                named.reference_declaration(&item.identifier)
            } else {
                let field = &donor_schema.columns()[key.source_column].name;
                named.match_declaration(&item.identifier, field)
            }
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
                    donors: binder::Donors {
                        named,
                        secondary,
                        items: previous,
                        inputs,
                    },
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
            if let Some(column) =
                local.and_then(|name| output.columns().iter().position(|field| field.name == name))
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
                                    field.name == schemas[authored].layout().columns()[column].name
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
    Ok(())
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
            crate::expression::Context::Derivation,
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
