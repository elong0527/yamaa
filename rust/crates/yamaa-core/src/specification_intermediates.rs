//! Input-backed named selections compiled from the admitted document, never a host plan.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{Intermediate, Keep, MatchKey, OrderTerm, SecondarySource, SourceSelection},
    predicate_compiler,
    value::Value,
};

#[derive(Debug)]
struct Declaration {
    name: String,
    path: String,
    source: usize,
    keys: Option<Vec<(String, String)>>,
    filter_text: Option<String>,
    filter: Option<Result<crate::predicate::Plan, predicate_compiler::Error>>,
    order: Vec<(String, bool, bool)>,
    keep: Option<Keep>,
    no_match: Option<Value>,
}
#[derive(Debug, Default)]
pub(super) struct Declarations(Vec<Declaration>);
impl Declarations {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
            let Some(source) = sources.iter().position(|source| source.name == dataset) else {
                reject(extra, "intermediate_dataset", format!("{path}.dataset"));
                continue;
            };
            if source == driver
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
                if !matches!(d.nodes()[key], N::Mapping(_)) {
                    reject(extra, "intermediate_key", format!("{path}.key"));
                    continue;
                }
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
                        .map(|&(a, b)| Ok((text(d, a)?.into(), text(d, b)?.into())))
                        .collect::<Result<Vec<_>, PrepareError>>()?,
                )
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
            if order.is_empty() != keep.is_none() {
                reject(extra, "intermediate_selection", path.clone());
            }
            let no_match = match d.field(id, "no_match").map(|id| &d.nodes()[id]) {
                None => None,
                Some(N::Null) => Some(Value::Missing),
                Some(N::Text(value)) => Some(Value::Str(value.clone())),
                Some(_) => {
                    reject(extra, "intermediate_handler", format!("{path}.no_match"));
                    None
                }
            };
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
    pub fn bind(
        &self,
        sources: &[SourceDeclaration],
        driver: usize,
        schemas: &[&TableSchema],
        output: &TableSchema,
        keys: &[usize],
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
            let schema = schemas[item.source];
            let source_name = &sources[item.source].name;
            let resolve = |name: &str, path: &str, findings: &mut Vec<BindFinding>| {
                let bare = name.strip_prefix(&format!("{source_name}."));
                let field = bare.and_then(|name| {
                    schema
                        .columns()
                        .iter()
                        .position(|column| column.name == name)
                });
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
            let default_keys = keys
                .iter()
                .map(|&column| {
                    let name = &output.columns()[column].name;
                    (name.clone(), name.clone())
                })
                .collect::<Vec<_>>();
            let pairs_declared = item.keys.as_ref().unwrap_or(&default_keys);
            let mut pairs = Vec::new();
            let mut record_keys = Vec::new();
            let primary = schemas[driver];
            let primary_prefix = format!("{}.", sources[driver].name);
            // Reference planning reports types before missing donor/output fields.
            for (field, name) in pairs_declared {
                let donor = schema.columns().iter().position(|c| c.name == *field);
                let target = output.columns().iter().position(|c| c.name == *name);
                if let Some(field) = name.strip_prefix(&primary_prefix) {
                    if let (Some(source_column), Some(driver_column)) = (
                        donor,
                        primary.columns().iter().position(|c| c.name == field),
                    ) {
                        let expected = primary.columns()[driver_column].kind;
                        let actual = schema.columns()[source_column].kind;
                        if expected != actual {
                            findings.push(BindFinding::Lookup(LookupFinding::key_type(
                                &format!("{}.key", item.path),
                                &item.name,
                                name,
                                expected,
                                actual,
                            )));
                            failed = true;
                        }
                        record_keys.push(crate::dataset::RecordMatchKey {
                            source_column,
                            driver_column,
                            identifier: name.clone(),
                        });
                    }
                }
                if let (Some(source_column), Some(output_column)) = (donor, target) {
                    let actual = schema.columns()[source_column].kind;
                    let expected = output.columns()[output_column].kind;
                    if !crate::key_relation::comparable(expected.into(), actual.into()) {
                        findings.push(BindFinding::Lookup(LookupFinding::key_type(
                            &format!("{}.key", item.path),
                            &item.name,
                            name,
                            expected,
                            actual,
                        )));
                        failed = true;
                    }
                    pairs.push(MatchKey {
                        source_column,
                        output_column,
                    });
                }
            }
            for (field, _) in pairs_declared {
                if !schema.columns().iter().any(|c| c.name == *field) {
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
            for (_, name) in pairs_declared {
                if !output.columns().iter().any(|c| c.name == *name)
                    && !name
                        .strip_prefix(&primary_prefix)
                        .is_some_and(|field| primary.columns().iter().any(|c| c.name == field))
                {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        &format!("{}.key", item.path),
                        name,
                        Some(&item.name),
                        ReferenceCause::OutputKey,
                        None,
                    )));
                    failed = true;
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
                    )));
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
                source: if item.source > driver {
                    item.source - 1
                } else {
                    item.source
                },
                keys: pairs,
                record_keys,
                filter,
                selection: item.keep.map(|keep| SourceSelection { order_by, keep }),
                no_match: item.no_match.clone(),
            }));
        }
        Ok((secondary, result))
    }
}
