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
            let filter = if let Some(filter) = d
                .field(id, "filter")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let expression = text(d, filter)?;
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
                            text(d, field(d, term, "direction")?)? == "desc",
                            text(d, field(d, term, "nulls")?)? == "first",
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
    ) -> Result<(Vec<SecondarySource>, Vec<Intermediate>), BindError> {
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
                    findings.push(BindFinding::UnknownReference {
                        path: path.into(),
                        name: name.into(),
                    });
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
            let mut pairs = Vec::new();
            for (field, name) in item.keys.as_ref().unwrap_or(&default_keys) {
                let source_column = resolve(
                    &format!("{source_name}.{field}"),
                    &format!("{}.key", item.path),
                    findings,
                );
                let output_column = output
                    .columns()
                    .iter()
                    .position(|column| &column.name == name);
                if output_column.is_none() {
                    findings.push(BindFinding::UnknownReference {
                        path: format!("{}.key.{field}", item.path),
                        name: name.clone(),
                    });
                }
                if let (Some(source_column), Some(output_column)) = (source_column, output_column) {
                    pairs.push(MatchKey {
                        source_column,
                        output_column,
                    });
                }
            }
            let filter = if let Some(compiled) = &item.filter {
                let plan = compiled
                    .as_ref()
                    .map_err(|error| BindError::PredicatePolicy(error.clone()))?;
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
                    None
                }
            } else {
                None
            };
            let mut order_by = Vec::new();
            for (name, descending, nulls_first) in &item.order {
                if let Some(column) = resolve(name, &format!("{}.order_by", item.path), findings) {
                    order_by.push(OrderTerm {
                        column,
                        descending: *descending,
                        nulls_first: *nulls_first,
                    });
                }
            }
            result.push(Intermediate {
                identifier: item.name.clone(),
                path: item.path.clone(),
                source: if item.source > driver {
                    item.source - 1
                } else {
                    item.source
                },
                keys: pairs,
                filter,
                selection: item.keep.map(|keep| SourceSelection { order_by, keep }),
                no_match: item.no_match.clone(),
            });
        }
        Ok((secondary, result))
    }
}
