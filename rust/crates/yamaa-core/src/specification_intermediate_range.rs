//! Pure range/projection binding; donor data and native effects remain in execution.
use super::*;
use crate::{
    dataset::{
        CurrentField, CurrentRead, Intermediate, RelationRange, RelationRangeBound,
        RelationRangeError,
    },
    diagnostic::ConditionCode as C,
};
#[derive(Debug)]
pub(super) struct Declaration {
    pub value: String,
    lower: Option<String>,
    upper: Option<String>,
    path: String,
}
impl Declaration {
    pub(super) fn donor_fields(&self) -> impl Iterator<Item = &str> {
        self.lower
            .iter()
            .chain(self.upper.iter())
            .map(String::as_str)
    }
}
pub(super) fn prepare(
    d: &Document,
    node: usize,
    path: &str,
    intermediate: &str,
) -> Result<Option<Declaration>, PrepareError> {
    let Some(node) = d
        .field(node, "between")
        .filter(|&node| !matches!(d.nodes()[node], N::Null))
    else {
        return Ok(None);
    };
    let value = text(d, field(d, node, "value")?)?.into();
    let bound = |name| {
        d.field(node, name)
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
            .map(|node| text(d, node).map(String::from))
            .transpose()
    };
    let lower = bound("lower")?;
    let upper = bound("upper")?;
    let path = format!("{path}.between");
    if lower.is_none() && upper.is_none() {
        let mut finding = LookupFinding::new(&path, C::IntermediateRangeDeclaration);
        finding.text("intermediate", intermediate);
        finding.text("expected", "at least one of lower, upper");
        return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
            finding.diagnostic(),
        )]));
    }
    Ok(Some(Declaration {
        value,
        lower,
        upper,
        path,
    }))
}
fn reference(path: &str, name: &str, intermediate: &str, code: C, findings: &mut Vec<BindFinding>) {
    let mut finding = LookupFinding::new(path, code);
    finding.text("intermediate", intermediate);
    finding.text("identifier", name);
    findings.push(BindFinding::Lookup(finding));
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    declaration: &Declaration,
    intermediate: &str,
    donor: &TableSchema,
    admits: &impl Fn(usize) -> bool,
    catalog: &Catalog,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    previous: &[Option<Intermediate>],
    secondary: &[crate::dataset::SecondarySource],
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationRange>, BindError> {
    let mut failed = false;
    let mut bound = |name: &Option<String>| {
        name.as_ref().and_then(|name| {
            let field = donor
                .columns()
                .iter()
                .enumerate()
                .find(|(column, field)| field.name == *name && admits(*column));
            if let Some((column, field)) = field {
                Some(RelationRangeBound {
                    column,
                    identifier: field.name.clone(),
                    kind: Some(field.kind.into()),
                })
            } else {
                reference(
                    &declaration.path,
                    name,
                    intermediate,
                    C::IntermediateRangeReference,
                    findings,
                );
                failed = true;
                None
            }
        })
    };
    // All stated donor bounds precede current-reference validation.
    let lower = bound(&declaration.lower);
    let upper = bound(&declaration.upper);
    if failed {
        return Ok(None);
    }
    let name = &declaration.value;
    let current = match catalog.bind(name).map_err(BindError::Catalog)? {
        Some(reference_binding::Binding::Output {
            column,
            column_type,
        }) => Some((CurrentRead::Output(column), Some(column_type.into()))),
        Some(reference_binding::Binding::Dataset {
            field, column_type, ..
        }) => Some((
            CurrentRead::Record {
                source: 0,
                column: field,
            },
            Some(column_type.into()),
        )),
        None => {
            if let Some((qualifier, field)) = name.split_once('.') {
                if let Some(source) = sources.iter().position(|source| source.name == qualifier) {
                    schemas[source]
                        .columns()
                        .iter()
                        .position(|column| column.name == field)
                        .map(|column| {
                            let source_port = if source == driver {
                                0
                            } else if source < driver {
                                source + 1
                            } else {
                                source
                            };
                            (
                                CurrentRead::Record {
                                    source: source_port,
                                    column,
                                },
                                Some(schemas[source].columns()[column].kind.into()),
                            )
                        })
                } else {
                    previous.iter().enumerate().find_map(|(index, item)| {
                        let item = item.as_ref()?;
                        if item.identifier != qualifier {
                            return None;
                        }
                        let column = item.source_column(secondary, field)?;
                        let kind = item.source_schema(secondary)?.columns()[column].kind.into();
                        Some((CurrentRead::Intermediate { index, column }, Some(kind)))
                    })
                }
            } else {
                schemas[driver]
                    .columns()
                    .iter()
                    .position(|column| column.name == *name)
                    .map(|column| {
                        (
                            CurrentRead::Record { source: 0, column },
                            Some(schemas[driver].columns()[column].kind.into()),
                        )
                    })
            }
        }
    };
    let Some((read, kind)) = current else {
        reference(
            &format!("{}.value", declaration.path),
            name,
            intermediate,
            C::IntermediateRangeReference,
            findings,
        );
        return Ok(None);
    };
    let initial = declaration
        .path
        .len()
        .checked_add(name.len())
        .and_then(|n| n.checked_add(core::mem::size_of::<RelationRange>()))
        .ok_or(BindError::Catalog(reference_binding::Error::SizeOverflow))?;
    let required = [lower.as_ref(), upper.as_ref()]
        .into_iter()
        .flatten()
        .try_fold(initial, |n, bound| n.checked_add(bound.identifier.len()));
    let required = required.ok_or(BindError::Catalog(reference_binding::Error::SizeOverflow))?;
    match RelationRange::new(
        declaration.path.clone(),
        CurrentField {
            read,
            identifier: name.clone(),
        },
        kind,
        lower,
        upper,
        16_777_216,
    ) {
        Ok(range) => Ok(Some(range)),
        Err(RelationRangeError::Incomparable(types)) => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                types.diagnostic(&declaration.path, intermediate),
            )));
            Ok(None)
        }
        Err(RelationRangeError::Limit) => {
            Err(BindError::Catalog(reference_binding::Error::Limit {
                resource: "intermediate_range_metadata",
                limit: 16_777_216,
                required,
            }))
        }
        Err(_) => Err(BindError::Internal),
    }
}
pub(super) fn prepare_columns(
    d: &Document,
    item: usize,
    limits: CompilationLimits,
) -> Result<Option<Vec<String>>, PrepareError> {
    d.field(item, "columns")
        .filter(|&node| !matches!(d.nodes()[node], N::Null))
        .map(|node| {
            let fields = sequence(d, node)?;
            if fields.len() > limits.source_fields {
                return Err(PrepareError::Limit("intermediate_columns"));
            }
            fields
                .iter()
                .map(|&node| text(d, node).map(String::from))
                .collect()
        })
        .transpose()
}
pub(super) fn bind_columns(
    names: &Option<Vec<String>>,
    donor_name: &str,
    schema: &TableSchema,
    admits: &impl Fn(usize) -> bool,
    path: &str,
    intermediate: &str,
    findings: &mut Vec<BindFinding>,
) -> Option<Vec<usize>> {
    names.as_ref().map(|names| {
        let mut columns = names
            .iter()
            .filter_map(|name| {
                if let Some(column) = schema
                    .columns()
                    .iter()
                    .position(|field| field.name == *name)
                    .filter(|&column| admits(column))
                {
                    Some(column)
                } else {
                    reference(
                        &format!("{path}.columns"),
                        &format!("{donor_name}.{name}"),
                        intermediate,
                        C::IntermediateColumnReference,
                        findings,
                    );
                    None
                }
            })
            .collect::<Vec<_>>();
        // A repeated readable name expresses the same restriction once.
        columns.sort_unstable();
        columns.dedup();
        columns
    })
}
