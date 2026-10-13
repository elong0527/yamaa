//! Named stored-input and SELF selections compiled from the admitted document.
use super::*;
use crate::{
    dataset::{Intermediate, IntermediateKey, Keep, OrderTerm, SecondarySource, SourceSelection},
    predicate_compiler,
    value::Value,
};

type UniqueDeclarations = Vec<(String, Vec<(String, String)>, Option<String>)>;

#[derive(Debug)]
struct Declaration {
    name: String,
    path: String,
    source: self_relations::Relation,
    keys: Option<Vec<(String, match_values::Declaration)>>,
    filter_text: Option<String>,
    filter: Option<Result<crate::predicate::Plan, predicate_compiler::Error>>,
    order: Vec<(String, bool, bool)>,
    keep: Option<Keep>,
    no_match: Option<Value>,
    range: Option<intermediate_range::Declaration>,
    columns: Option<Vec<String>>,
    unique: UniqueDeclarations,
    derivations: Vec<intermediate_derivations::Derivation>,
    driver_prohibited: Vec<String>,
}
#[derive(Debug, Default)]
pub(super) struct Declarations(alloc::sync::Arc<Vec<Declaration>>, Option<String>);
impl Declarations {
    pub(super) fn add_call_uses(&self, uses: &mut [usize]) -> Result<(), PrepareError> {
        let mut calls = Vec::new();
        for item in self.0.iter() {
            for derived in &item.derivations {
                derivations::calls(&derived.branch.operation, &mut calls);
            }
        }
        for call in calls {
            let count = uses.get_mut(call).ok_or(PrepareError::Internal)?;
            *count = count
                .checked_add(1)
                .ok_or(PrepareError::Limit("function_calls"))?;
        }
        Ok(())
    }
    pub(super) fn for_reader(&self, reader: &str) -> Self {
        Self(alloc::sync::Arc::clone(&self.0), Some(reader.into()))
    }
    pub(super) fn reader(&self) -> Option<&str> {
        self.1.as_deref()
    }

    pub fn is_self(&self, index: usize) -> bool {
        self.0
            .get(index)
            .is_some_and(|item| matches!(item.source, self_relations::Relation::SelfRows))
    }
    pub fn self_donor_fields(&self) -> impl Iterator<Item = &str> {
        self.0
            .iter()
            .filter(|item| matches!(item.source, self_relations::Relation::SelfRows))
            .flat_map(|item| {
                item.keys
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|(field, _)| field.as_str())
                    .chain(
                        item.order
                            .iter()
                            .map(|(field, _, _)| field.strip_prefix("SELF.").unwrap_or(field)),
                    )
                    .chain(
                        item.filter
                            .as_ref()
                            .and_then(|filter| filter.as_ref().ok())
                            .into_iter()
                            .flat_map(|plan| plan.identifiers())
                            .filter(|field| !field.contains('.') || field.starts_with("SELF."))
                            .map(|field| field.strip_prefix("SELF.").unwrap_or(field)),
                    )
                    .chain(
                        item.derivations
                            .iter()
                            .flat_map(|derive| &derive.references)
                            .filter(|name| !name.contains('.') || name.starts_with("SELF."))
                            .map(|name| name.strip_prefix("SELF.").unwrap_or(name)),
                    )
                    .chain(item.range.iter().flat_map(|range| range.donor_fields()))
                    .chain(item.columns.iter().flatten().map(String::as_str))
                    .chain(
                        item.unique.iter().flat_map(|(_, columns, _)| {
                            columns.iter().map(|(name, _)| name.as_str())
                        }),
                    )
            })
    }
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|item| item.name == name)
    }
    pub(super) fn kept(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|item| item.name == name && item.keep.is_some())
    }
    pub(super) fn driver_findings(
        &self,
        name: &str,
        path: &str,
        item: &Intermediate,
        secondary: &[SecondarySource],
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let declaration = self
            .0
            .iter()
            .find(|declaration| declaration.name == name)
            .ok_or(BindError::Internal)?;
        let mut prohibited = declaration.driver_prohibited.clone();
        if item.source.is_self() {
            prohibited.push("dataset".into());
        }
        if item
            .filter
            .as_ref()
            .is_some_and(crate::dataset::RelationPredicate::correlated)
        {
            prohibited.push("filter".into());
        }
        if item
            .derivations
            .as_ref()
            .is_some_and(|derived| !derived.current.is_empty())
        {
            prohibited.push("derivations".into());
        }
        if !prohibited.is_empty() {
            let mut finding = LookupFinding::new(
                path,
                crate::diagnostic::ConditionCode::IntermediateDriverInvalid,
            );
            finding.text("intermediate", name);
            let fields = prohibited.iter().map(String::as_str).collect::<Vec<_>>();
            finding.texts("fields", &fields);
            findings.push(BindFinding::Lookup(finding));
            return Ok(());
        }
        if let Some(derived) = &item.derivations {
            let schema = item.source_schema(secondary).ok_or(BindError::Internal)?;
            let unknown = schema
                .columns()
                .iter()
                .enumerate()
                .filter(|(column, _)| {
                    item.admits_column(*column)
                        && *column >= derived.stored_columns
                        && derived
                            .driver_kinds
                            .get(*column - derived.stored_columns)
                            .is_none_or(Option::is_none)
                })
                .map(|(_, column)| column.name.as_str())
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                let mut finding = LookupFinding::new(
                    path,
                    crate::diagnostic::ConditionCode::IntermediateDriverType,
                );
                finding.text("intermediate", name);
                finding.texts("columns", &unknown);
                findings.push(BindFinding::Lookup(finding));
            }
        }
        Ok(())
    }
    pub fn reference<'a>(&self, name: &'a str) -> Option<(usize, &'a str)> {
        let (relation, field) = name.split_once('.')?;
        self.0
            .iter()
            .position(|item| item.name == relation)
            .map(|index| (index, field))
    }
    pub(super) fn match_references(&self, index: usize) -> impl Iterator<Item = &str> {
        self.0.get(index).into_iter().flat_map(|item| {
            item.keys
                .as_ref()
                .into_iter()
                .flatten()
                .filter_map(|(_, value)| value.reference())
                .chain(item.range.iter().map(|range| range.value.as_str()))
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        d: &Document,
        sources: &[SourceDeclaration],
        driver: usize,
        limits: CompilationLimits,
        project: Option<&functions::Prepared>,
        extraction: &str_extract::Prepared,
        templates: &str_template::Prepared,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        let mut result = Self::default();
        let Some(id) = d
            .field(d.root(), "intermediates")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            return Ok(result);
        };
        let items = sequence(d, id)?;
        if items.len() > limits.columns {
            return Err(PrepareError::Limit("intermediates"));
        }
        let findings = self_relations::declaration_findings(d, sources, items)?;
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let findings = intermediate_admission::declaration_findings(d, items, sources)?;
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        for (index, &id) in items.iter().enumerate() {
            let path = format!("intermediates[{index}]");
            let name = text(d, field(d, id, "id")?)?;
            let range = intermediate_range::prepare(d, id, &path, name)?;
            let columns = intermediate_range::prepare_columns(d, id, limits)?;
            let dataset = text(d, field(d, id, "dataset")?)?;
            let source = if dataset == "SELF" && present(d, d.root(), "rows") {
                self_relations::Relation::SelfRows
            } else if let Some(source) = sources.iter().position(|source| source.name == dataset) {
                self_relations::Relation::Input(source)
            } else {
                reject(extra, "intermediate_dataset", format!("{path}.dataset"));
                continue;
            };
            let keys = if let Some(key) = d
                .field(id, "key")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                if !matches!(d.nodes()[key], N::Text(_) | N::Mapping(_) | N::Sequence(_)) {
                    reject(extra, "intermediate_key", format!("{path}.key"));
                    continue;
                }
                if let N::Text(name) = &d.nodes()[key] {
                    if limits.keys == 0 {
                        return Err(PrepareError::Limit("intermediate_keys"));
                    }
                    Some(vec![(
                        name.clone(),
                        match_values::Declaration::Variable(name.clone()),
                    )])
                } else if matches!(d.nodes()[key], N::Sequence(_)) {
                    let fields = sequence(d, key)?;
                    if fields.len() > limits.keys {
                        return Err(PrepareError::Limit("intermediate_keys"));
                    }
                    Some(
                        fields
                            .iter()
                            .map(|&id| {
                                text(d, id).map(|name| {
                                    (
                                        name.into(),
                                        match_values::Declaration::Variable(name.into()),
                                    )
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                } else {
                    let fields = mapping(d, key)?;
                    if fields.len() > limits.keys {
                        return Err(PrepareError::Limit("intermediate_keys"));
                    }
                    if fields.is_empty() {
                        reject(extra, "intermediate_key", format!("{path}.key"));
                    }
                    Some(
                        fields
                            .iter()
                            .map(|&(a, b)| {
                                let name = text(d, a)?;
                                Ok((
                                    name.into(),
                                    match_values::Declaration::prepare(
                                        d,
                                        b,
                                        &format!("{path}.key.{name}"),
                                        sources,
                                        driver,
                                        extra,
                                    )?,
                                ))
                            })
                            .collect::<Result<Vec<_>, PrepareError>>()?,
                    )
                }
            } else {
                None
            };
            let mut filter_text = None;
            let filter = if let Some(filter) = d
                .field(id, "filter")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let expression = text(d, filter)?;
                filter_text = Some(expression.into());
                let compiled = predicate_compiler::compile(
                    expression,
                    &format!("{path}.filter"),
                    Default::default(),
                );
                match &compiled {
                    Err(predicate_compiler::Error::Parse(
                        crate::predicate_parser::ParseError::Grammar { .. },
                    ))
                    | Ok(_) => {}
                    Err(predicate_compiler::Error::Parse(
                        crate::predicate_parser::ParseError::UnsupportedRegex { .. },
                    )) => {
                        reject(extra, "predicate_regex", format!("{path}.filter"));
                    }
                    Err(predicate_compiler::Error::Internal) => return Err(PrepareError::Internal),
                    Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
                }
                Some(compiled)
            } else {
                None
            };
            let keep = match d
                .field(id, "keep")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| text(d, id))
                .transpose()?
            {
                Some("first") => Some(Keep::First),
                Some("last") => Some(Keep::Last),
                None => None,
                _ => return Err(PrepareError::Internal),
            };
            let mut order = Vec::new();
            if let Some(order_id) = d
                .field(id, "order_by")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let terms = sequence(d, order_id)?;
                if terms.len() > limits.source_fields {
                    return Err(PrepareError::Limit("intermediate_order"));
                }
                for &term in terms {
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
            let no_match = d
                .field(id, "no_match")
                .map(|value| literal(d, value, &format!("{path}.no_match")))
                .transpose()?;
            let mut unique = Vec::new();
            if let Some(checks) = d
                .field(id, "verifications")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let mut identities = alloc::collections::BTreeSet::new();
                for (check_index, &check) in sequence(d, checks)?.iter().enumerate() {
                    let payload = field(d, check, "unique")?;
                    let check_path = format!("{path}.verifications[{check_index}].unique");
                    let identity = d
                        .field(payload, "id")
                        .map(|id| text(d, id).map(String::from))
                        .transpose()?;
                    if let Some(identity) = &identity {
                        if !identities.insert(identity.clone()) {
                            let mut finding = LookupFinding::new(
                                &format!("{check_path}.id"),
                                crate::diagnostic::ConditionCode::VerificationIdentifierRepeated,
                            );
                            finding.text("identifier", identity);
                            return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                                finding.diagnostic(),
                            )]));
                        }
                    }
                    let names = if matches!(d.nodes()[payload], N::Sequence(_)) {
                        payload
                    } else {
                        field(d, payload, "columns")?
                    };
                    let columns = sequence(d, names)?
                        .iter()
                        .enumerate()
                        .map(|(index, &id)| {
                            text(d, id).map(|name| {
                                (
                                    name.into(),
                                    if names == payload {
                                        format!("{check_path}[{index}]")
                                    } else {
                                        format!("{check_path}.columns[{index}]")
                                    },
                                )
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    unique.push((check_path, columns, identity));
                }
            }
            let driver_prohibited = ["key", "between", "order_by", "keep", "no_match"]
                .into_iter()
                .filter(|name| {
                    if *name == "no_match" {
                        d.field(id, name).is_some()
                    } else {
                        present(d, id, name)
                    }
                })
                .map(String::from)
                .collect();
            let derivations = intermediate_derivations::prepare(
                d, id, &path, dataset, limits, project, extraction, templates, &result,
            )?;
            alloc::sync::Arc::get_mut(&mut result.0)
                .ok_or(PrepareError::Internal)?
                .push(Declaration {
                    name: name.into(),
                    path,
                    source,
                    keys,
                    filter,
                    filter_text,
                    order,
                    keep,
                    no_match,
                    range,
                    columns,
                    unique,
                    derivations,
                    driver_prohibited,
                });
        }
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn bind(
        &self,
        sources: &[SourceDeclaration],
        driver: usize,
        schemas: &[&TableSchema],
        output: &TableSchema,
        keys: &[usize],
        self_columns: Option<&[usize]>,
        project: Option<&crate::project_calls::ProjectCalls>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(Vec<SecondarySource>, Vec<Option<Intermediate>>), BindError> {
        intermediate_sources::admit(self.0.iter().filter_map(|item| match item.source {
            self_relations::Relation::Input(index) if index == driver => {
                Some((schemas[index], sources[index].name.as_str()))
            }
            self_relations::Relation::SelfRows => Some((output, "SELF")),
            _ => None,
        }))?;
        let secondary = sources
            .iter()
            .zip(schemas)
            .enumerate()
            .filter(|(index, _)| *index != driver)
            .map(|(_, (declaration, schema))| SecondarySource {
                name: declaration.name.clone(),
                schema: (**schema).clone(),
            })
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for item in self.0.iter() {
            let mut failed = false;
            let (schema, source_name) = match item.source {
                self_relations::Relation::Input(index) => {
                    (schemas[index], sources[index].name.as_str())
                }
                self_relations::Relation::SelfRows => (output, "SELF"),
            };
            let mut pending_schema = None;
            let derivations = intermediate_derivations::bind(
                &item.derivations,
                source_name,
                &item.name,
                &mut pending_schema,
                schema,
                matches!(item.source, self_relations::Relation::SelfRows)
                    .then_some(self_columns.unwrap_or(&[])),
                sources,
                schemas,
                driver,
                output,
                self,
                &result,
                project,
                findings,
            )?;
            let schema = pending_schema.as_ref().unwrap_or_else(|| {
                derivations
                    .as_ref()
                    .map_or(schema, |derived| derived.plan.output())
            });
            let admits = |column: usize| {
                !matches!(item.source, self_relations::Relation::SelfRows)
                    || column >= output.columns().len()
                    || self_columns.is_some_and(|columns| columns.contains(&column))
            };
            let resolve = |name: &str, path: &str, findings: &mut Vec<BindFinding>| {
                let bare = name.strip_prefix(&format!("{source_name}.")).or_else(|| {
                    matches!(item.source, self_relations::Relation::SelfRows).then_some(name)
                });
                let field = bare.and_then(|name| {
                    schema
                        .columns()
                        .iter()
                        .position(|column| column.name == name)
                });
                let field = field.filter(|&column| admits(column));
                if field.is_none() {
                    let suggestion = (!name.contains('.')
                        && schema.columns().iter().any(|c| c.name == name))
                    .then(|| format!("{source_name}.{name}"));
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        path,
                        name,
                        Some(&item.name),
                        ReferenceCause::Selection,
                        suggestion,
                    )));
                }
                field
            };
            let output_fields = output
                .columns()
                .iter()
                .map(|column| reference_binding::Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>();
            let current_sources = core::iter::once(driver)
                .chain((0..sources.len()).filter(|&index| index != driver))
                .collect::<Vec<_>>();
            let source_fields = current_sources
                .iter()
                .map(|&index| {
                    schemas[index]
                        .columns()
                        .iter()
                        .map(|column| reference_binding::Field {
                            name: &column.name,
                            column_type: column.kind,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let datasets = current_sources
                .iter()
                .zip(&source_fields)
                .map(|(&index, fields)| reference_binding::Dataset {
                    name: &sources[index].name,
                    fields,
                })
                .collect::<Vec<_>>();
            let catalog = Catalog::compile(&output_fields, &datasets, Default::default())
                .map_err(BindError::Catalog)?;
            let default_keys = if item.keys.is_none() {
                match intermediate_admission::inferred_keys(
                    &catalog,
                    output,
                    keys,
                    schema,
                    &admits,
                    &item.path,
                    source_name,
                    findings,
                )? {
                    Some(keys) => keys,
                    None => {
                        result.push(None);
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
            let pairs_declared = item.keys.as_ref().unwrap_or(&default_keys);
            // Result types and donor fields precede expression declarations and reads.
            for (field, declaration) in pairs_declared {
                if let (Some(donor), Some(expected)) = (
                    schema.columns().iter().find(|column| column.name == *field),
                    declaration.result_type(&catalog)?,
                ) {
                    if !crate::key_relation::comparable(expected, donor.kind.into()) {
                        findings.push(BindFinding::Lookup(LookupFinding::key_type(
                            &format!("{}.key", item.path),
                            &item.name,
                            &declaration.display_name(field),
                            expected,
                            donor.kind,
                        )));
                        failed = true;
                    }
                }
            }
            for (field, _) in pairs_declared {
                if !schema
                    .columns()
                    .iter()
                    .enumerate()
                    .any(|(index, column)| column.name == *field && admits(index))
                {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        &format!("{}.key", item.path),
                        &format!("{source_name}.{field}"),
                        Some(&item.name),
                        ReferenceCause::DonorKey,
                        None,
                    )));
                    failed = true;
                }
            }
            let contextual_reader = self.0.iter().any(|reader| {
                reader.derivations.iter().any(|derive| {
                    derive.references.iter().any(|reference| {
                        reference
                            .split_once('.')
                            .is_some_and(|(owner, _)| owner == item.name)
                    })
                })
            });
            let mut pairs = Vec::new();
            for (field, declaration) in pairs_declared {
                let path = if matches!(declaration, match_values::Declaration::Variable(_)) {
                    format!("{}.key", item.path)
                } else {
                    format!("{}.key.{field}", item.path)
                };
                let value = if contextual_reader {
                    declaration.bind_deferred(&path, &item.name, &catalog, findings)?
                } else {
                    declaration.bind(&path, &item.name, &catalog, findings)?
                };
                match value {
                    Some(value) => {
                        if let Some(source_column) = schema
                            .columns()
                            .iter()
                            .position(|column| column.name == *field)
                        {
                            pairs.push(IntermediateKey {
                                source_column,
                                path,
                                value,
                            });
                        }
                    }
                    None => failed = true,
                }
            }
            let filter = match &item.filter {
                Some(Ok(plan)) => {
                    let filter = relation_predicates::bind(
                        plan,
                        source_name,
                        schema,
                        matches!(item.source, self_relations::Relation::SelfRows),
                        &admits,
                        &item.name,
                        sources,
                        schemas,
                        driver,
                        findings,
                    )?;
                    if filter.is_none() {
                        failed = true;
                    }
                    filter
                }
                Some(Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::Grammar {
                        position, failure, ..
                    },
                ))) => {
                    findings.push(BindFinding::Lookup(LookupFinding::grammar(
                        &format!("{}.filter", item.path),
                        item.filter_text.as_deref().expect("written predicate"),
                        position.character,
                        failure,
                    )?));
                    // A syntax rejection does not remove the declaration's schema.
                    None
                }
                Some(Err(error)) => return Err(BindError::PredicatePolicy(error.clone())),
                None => None,
            };
            let mut order_by = Vec::new();
            for (index, (name, descending, nulls_first)) in item.order.iter().enumerate() {
                if let Some(column) =
                    resolve(name, &format!("{}.order_by[{index}]", item.path), findings)
                {
                    order_by.push(OrderTerm {
                        column,
                        descending: *descending,
                        nulls_first: *nulls_first,
                    });
                } else {
                    failed = true;
                }
            }
            let before = findings.len();
            let columns = intermediate_range::bind_columns(
                &item.columns,
                source_name,
                schema,
                &admits,
                &item.path,
                &item.name,
                findings,
            );
            // A rejected projection retains donor names for subsequent checkpoints.
            // Its owned finding still prevents final plan admission.
            let columns = (findings.len() == before).then_some(columns).flatten();
            let range = item
                .range
                .as_ref()
                .map(|declaration| {
                    intermediate_range::bind(
                        declaration,
                        &item.name,
                        schema,
                        &admits,
                        &catalog,
                        sources,
                        schemas,
                        driver,
                        &result,
                        &secondary,
                        findings,
                    )
                })
                .transpose()?
                .flatten();
            let mut unique = Vec::new();
            for (path, names, identity) in &item.unique {
                let mut columns = Vec::new();
                for (name, field_path) in names {
                    if let Some(column) = schema
                        .columns()
                        .iter()
                        .position(|column| column.name == *name)
                        .filter(|&column| admits(column))
                    {
                        columns.push(column);
                    } else {
                        let mut finding = LookupFinding::new(
                            field_path,
                            crate::diagnostic::ConditionCode::IntermediateUniqueField,
                        );
                        finding.text("intermediate", &item.name);
                        finding.text("identifier", &format!("{source_name}.{name}"));
                        findings.push(BindFinding::Lookup(finding));
                    }
                }
                unique.push(crate::dataset::IntermediateUnique {
                    path: path.clone(),
                    columns,
                    identity: identity.clone(),
                });
            }
            for check in &unique {
                if filter
                    .as_ref()
                    .is_some_and(crate::dataset::RelationPredicate::correlated)
                {
                    let mut finding = LookupFinding::new(
                        &check.path,
                        crate::diagnostic::ConditionCode::IntermediateUniqueCorrelatedFilter,
                    );
                    finding.text("intermediate", &item.name);
                    findings.push(BindFinding::Lookup(finding));
                }
                if derivations
                    .as_ref()
                    .is_some_and(|derived| !derived.current.is_empty())
                {
                    let mut finding = LookupFinding::new(
                        &check.path,
                        crate::diagnostic::ConditionCode::IntermediateUniqueCorrelatedDerivation,
                    );
                    finding.text("intermediate", &item.name);
                    findings.push(BindFinding::Lookup(finding));
                }
            }
            // Range rejection owns its original cause without turning an existing
            // donor field into a second unknown-reference diagnostic.
            result.push((!failed).then(|| Intermediate {
                identifier: item.name.clone(),
                path: item.path.clone(),
                source: match item.source {
                    self_relations::Relation::Input(index) if index == driver => {
                        crate::dataset::IntermediateSource::Primary {
                            schema: schemas[index].clone(),
                            name: sources[index].name.clone(),
                        }
                    }
                    self_relations::Relation::Input(index) => {
                        crate::dataset::IntermediateSource::Input(if index > driver {
                            index - 1
                        } else {
                            index
                        })
                    }
                    self_relations::Relation::SelfRows => {
                        crate::dataset::IntermediateSource::SelfRows {
                            schema: output.clone(),
                            columns: self_columns.unwrap_or(&[]).to_vec(),
                        }
                    }
                },
                keys: pairs,
                filter,
                selection: item.keep.map(|keep| SourceSelection { order_by, keep }),
                no_match: item.no_match.clone(),
                unique,
                derivations,
                pending_schema,

                range,
                columns,
            }));
        }
        Ok((secondary, result))
    }
}

impl Declarations {
    pub(super) fn check_findings(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        for declaration in self.0.iter() {
            if let (Some(expression), Some(compiled)) =
                (&declaration.filter_text, &declaration.filter)
            {
                static_checks::predicate(
                    compiled,
                    expression,
                    &format!("{}.filter", declaration.path),
                    findings,
                )?;
            }
            for derived in &declaration.derivations {
                static_checks::branch(&derived.branch, findings)?;
            }
        }
        Ok(())
    }
}
