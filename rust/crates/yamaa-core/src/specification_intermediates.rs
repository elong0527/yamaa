//! Named stored-input and SELF selections compiled from the admitted document.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{Intermediate, IntermediateKey, Keep, OrderTerm, SecondarySource, SourceSelection},
    predicate_compiler,
    value::Value,
};

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
}
#[derive(Debug, Default)]
pub(super) struct Declarations(Vec<Declaration>);
impl Declarations {
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
                            .map(|field| field.strip_prefix("SELF.").unwrap_or(field)),
                    )
            })
    }
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|item| item.name == name)
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
        })
    }
    pub fn prepare(
        d: &Document,
        sources: &[SourceDeclaration],
        driver: usize,
        limits: CompilationLimits,
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
        let findings = intermediate_admission::declaration_findings(d, items)?;
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        for (index, &id) in items.iter().enumerate() {
            let path = format!("intermediates[{index}]");
            optional_features(
                d,
                id,
                &["between", "columns", "derivations", "verifications"],
                &path,
                extra,
            );
            let name = text(d, field(d, id, "id")?)?;
            let dataset = text(d, field(d, id, "dataset")?)?;
            let source = if dataset == "SELF" && present(d, d.root(), "rows") {
                self_relations::Relation::SelfRows
            } else if let Some(source) = sources.iter().position(|source| source.name == dataset) {
                self_relations::Relation::Input(source)
            } else {
                reject(extra, "intermediate_dataset", format!("{path}.dataset"));
                continue;
            };
            if matches!(source, self_relations::Relation::Input(index) if index == driver)
                || result.contains(name)
                || sources.iter().any(|source| source.name == name)
            {
                reject(extra, "intermediate_scope", path.clone());
                continue;
            }
            let keys = if let Some(key) = d
                .field(id, "key")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                if !matches!(d.nodes()[key], N::Mapping(_) | N::Sequence(_)) {
                    reject(extra, "intermediate_key", format!("{path}.key"));
                    continue;
                }
                if matches!(d.nodes()[key], N::Sequence(_)) {
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
                    Err(predicate_compiler::Error::UnsupportedLiteral { .. }) => {
                        reject(extra, "predicate_literal", format!("{path}.filter"))
                    }
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
            result.0.push(Declaration {
                name: name.into(),
                path,
                source,
                keys,
                filter,
                filter_text,
                order,
                keep,
                no_match,
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
        findings: &mut Vec<BindFinding>,
    ) -> Result<(Vec<SecondarySource>, Vec<Option<Intermediate>>), BindError> {
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
        for item in &self.0 {
            let mut failed = false;
            let (schema, source_name) = match item.source {
                self_relations::Relation::Input(index) => {
                    (schemas[index], sources[index].name.as_str())
                }
                self_relations::Relation::SelfRows => (output, "SELF"),
            };
            let admits = |column: usize| {
                !matches!(item.source, self_relations::Relation::SelfRows)
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
            let primary = schemas[driver];
            let output_fields = output
                .columns()
                .iter()
                .map(|column| reference_binding::Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>();
            let primary_fields = primary
                .columns()
                .iter()
                .map(|column| reference_binding::Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>();
            let catalog = Catalog::compile(
                &output_fields,
                &[reference_binding::Dataset {
                    name: &sources[driver].name,
                    fields: &primary_fields,
                }],
                Default::default(),
            )
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
            let mut pairs = Vec::new();
            for (field, declaration) in pairs_declared {
                let path = if matches!(declaration, match_values::Declaration::Variable(_)) {
                    format!("{}.key", item.path)
                } else {
                    format!("{}.key.{field}", item.path)
                };
                match declaration.bind(&path, &item.name, &catalog, findings)? {
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
                    let mut bindings = Vec::new();
                    let before = findings.len();
                    for name in plan.identifiers() {
                        if let Some(column) = resolve(name, plan.spec_path(), findings) {
                            bindings.push(Binding {
                                name: name.into(),
                                read: Read::Source(column),
                            });
                        }
                    }
                    if findings.len() == before {
                        Some(
                            BoundPredicate::new(plan.clone(), bindings)
                                .map_err(BindError::InvalidPredicateBinding)?,
                        )
                    } else {
                        failed = true;
                        None
                    }
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
            result.push((!failed).then(|| Intermediate {
                identifier: item.name.clone(),
                path: item.path.clone(),
                source: match item.source {
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
            }));
        }
        Ok((secondary, result))
    }
}
