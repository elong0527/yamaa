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
    donor_kind: &impl Fn(usize) -> Option<ColumnType>,
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
                    kind: donor_kind(column).map(Into::into),
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
    let current = match catalog.bind(name) {
        Some(reference_binding::Binding::Output {
            column,
            column_type,
        }) => Some((CurrentRead::Output(column), Some(column_type.into()))),
        // The catalog lists the driver first and the other inputs in source
        // order, so a dataset index is the named input's record port.
        Some(reference_binding::Binding::Dataset {
            dataset,
            field,
            column_type,
        }) => Some((
            CurrentRead::Record {
                source: dataset,
                column: field,
            },
            Some(column_type.into()),
        )),
        None => {
            if let Some((qualifier, field)) = name.split_once('.') {
                // Every input binds above when it has the field.
                if sources.iter().any(|source| source.name == qualifier) {
                    None
                } else {
                    previous.iter().enumerate().find_map(|(index, item)| {
                        let item = item.as_ref()?;
                        if item.identifier != qualifier {
                            return None;
                        }
                        let column = item.source_column(secondary, field)?;
                        let kind = item.source_kind(secondary, column).map(Into::into);
                        Some((CurrentRead::Intermediate { index, column }, kind))
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
    match RelationRange::new(
        declaration.path.clone(),
        CurrentField {
            read,
            identifier: name.clone(),
        },
        kind,
        lower,
        upper,
    ) {
        Ok(range) => Ok(Some(range)),
        Err(RelationRangeError::Incomparable(types)) => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                types.diagnostic(&declaration.path, intermediate),
            )));
            Ok(None)
        }
        Err(_) => Err(BindError::Internal),
    }
}
pub(super) fn prepare_columns(
    d: &Document,
    item: usize,
) -> Result<Option<Vec<String>>, PrepareError> {
    d.field(item, "columns")
        .filter(|&node| !matches!(d.nodes()[node], N::Null))
        .map(|node| {
            let fields = sequence(d, node)?;
            if fields.len() > crate::limits::Limits::COLUMNS {
                return Err(PrepareError::limit(
                    "intermediate_columns",
                    crate::limits::Limits::COLUMNS,
                    Some(fields.len()),
                    None,
                ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(names: &[&str]) -> TableSchema {
        TableSchema::new(
            names
                .iter()
                .map(|&name| Column {
                    name: name.into(),
                    kind: ColumnType::Int,
                })
                .collect(),
        )
        .unwrap()
    }
    fn source(name: &str) -> SourceDeclaration {
        SourceDeclaration {
            name: name.into(),
            path: format!("{name}.csv"),
            types: Vec::new(),
            profile: SourceProfile::Csv,
            encoding: None,
            empty_string_present: false,
            ordinal: None,
        }
    }
    fn fields(schema: &TableSchema) -> Vec<reference_binding::Field<'_>> {
        schema
            .columns()
            .iter()
            .map(|column| reference_binding::Field {
                name: &column.name,
                column_type: column.kind,
            })
            .collect()
    }

    /// The driver is the second input, so source order differs from port
    /// order, and the driver holds another field at the named column.
    #[test]
    fn a_qualified_value_keeps_its_input_port() {
        let other = schema(&["ID", "REFDAY"]);
        let driver = schema(&["ID", "DAY"]);
        let donor = schema(&["ID", "LO", "HI"]);
        let (other_fields, driver_fields, donor_fields) =
            (fields(&other), fields(&driver), fields(&donor));
        // Intermediates bind against the driver first, then the other inputs.
        let catalog = Catalog::compile(
            &[],
            &[
                reference_binding::Dataset {
                    name: "DRIVER",
                    fields: &driver_fields,
                },
                reference_binding::Dataset {
                    name: "OTHER",
                    fields: &other_fields,
                },
                reference_binding::Dataset {
                    name: "DONOR",
                    fields: &donor_fields,
                },
            ],
        )
        .unwrap();
        let declaration = Declaration {
            value: "OTHER.REFDAY".into(),
            lower: Some("LO".into()),
            upper: Some("HI".into()),
            path: "intermediates[0].between".into(),
        };
        let mut findings = Vec::new();
        let range = bind(
            &declaration,
            "W",
            &donor,
            &|column| donor.columns().get(column).map(|column| column.kind),
            &|_| true,
            &catalog,
            &[source("OTHER"), source("DRIVER"), source("DONOR")],
            &[&other, &driver, &donor],
            1,
            &[],
            &[],
            &mut findings,
        )
        .unwrap()
        .unwrap();
        assert!(findings.is_empty());
        assert_eq!(
            range.current().read,
            CurrentRead::Record {
                source: 1,
                column: 1
            }
        );
    }
}
