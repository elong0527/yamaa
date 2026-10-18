//! Bridge named input selections into one completed record context.
//! The matching language remains owned by the shared match-value compiler.
use super::*;

/// Select a common authored input without introducing a root base/default.
pub(super) fn common_driver<'a>(
    d: &'a Document,
    inputs: &[(usize, usize)],
) -> Result<&'a str, PrepareError> {
    let rows = sequence(d, field(d, d.root(), "rows")?)?;
    let mut common = None;
    for (row_index, &row) in rows.iter().enumerate() {
        let selected = d
            .field(row, "dataset")
            .filter(|&id| !matches!(d.nodes()[id], N::Null));
        let name = match selected {
            Some(id) => text(d, id)?,
            None if inputs.len() == 1 => text(d, inputs[0].0)?,
            // Invalid/missing names have already failed model preflight.
            None => return Err(PrepareError::Internal),
        };
        let name = if inputs
            .iter()
            .any(|&(id, _)| text(d, id).is_ok_and(|input| input == name))
        {
            name
        } else {
            let declarations = sequence(d, field(d, d.root(), "intermediates")?)?;
            let declaration = declarations
                .iter()
                .find(|&&item| {
                    d.field(item, "id")
                        .is_some_and(|id| text(d, id).is_ok_and(|id| id == name))
                })
                .ok_or(PrepareError::Internal)?;
            let selected = text(d, field(d, *declaration, "dataset")?)?;
            let mut prohibited = [
                "key",
                "between",
                "order_by",
                "keep",
                "no_match",
                "require_match",
            ]
            .into_iter()
            .filter(|field| {
                if *field == "no_match" {
                    d.field(*declaration, field).is_some()
                } else {
                    present(d, *declaration, field)
                }
            })
            .collect::<Vec<_>>();
            if !inputs
                .iter()
                .any(|&(id, _)| text(d, id).is_ok_and(|name| name == selected))
            {
                prohibited.push("dataset");
            }
            if !prohibited.is_empty() {
                let mut finding = LookupFinding::new(
                    &format!("rows[{row_index}].dataset"),
                    crate::diagnostic::ConditionCode::IntermediateDriverInvalid,
                );
                finding.text("intermediate", name);
                finding.texts("fields", &prohibited);
                return Err(PrepareError::Invalid(vec![PreflightFinding::Scalar(
                    finding.diagnostic(),
                )]));
            }
            selected
        };
        common.get_or_insert(name);
    }
    common.ok_or(PrepareError::Internal)
}

/// Borrow references from admitted key occurrences, without lowering them again.
/// Named reads in every expression promote their matching output defaults.
pub(super) fn default_dependencies(
    d: &Document,
    reference: &str,
    output: &TableSchema,
    templates: &str_template::Prepared,
) -> Result<Vec<usize>, PrepareError> {
    let Some((name, _)) = reference.split_once('.') else {
        return Ok(Vec::new());
    };
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(Vec::new());
    };
    let Some(&item) = sequence(d, items)?.iter().find(|&&item| {
        d.field(item, "id")
            .is_some_and(|id| text(d, id).is_ok_and(|id| id == name))
    }) else {
        return Ok(Vec::new());
    };
    let mut dependencies = Vec::new();
    let mut register = |name: &str| {
        if !name.contains('.') {
            if let Some(column) = output
                .columns()
                .iter()
                .position(|column| column.name == name)
            {
                dependencies.push(column);
            }
        }
    };
    if let Some(key) = d
        .field(item, "key")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        if let N::Text(name) = &d.nodes()[key] {
            register(name);
        } else if matches!(d.nodes()[key], N::Sequence(_)) {
            for &value in sequence(d, key)? {
                register(text(d, value)?);
            }
        } else {
            // Preparation has already admitted the closed map-valued key language.
            for &(_, value) in mapping(d, key)? {
                if let N::Text(name) = &d.nodes()[value] {
                    register(name);
                    continue;
                }
                let mut names = Vec::new();
                derivations::current_references(d, value, None, &mut names, 0)?;
                self_relations::template_references(d, value, templates, &mut names, 0)?;
                for name in &names {
                    register(name);
                }
            }
        }
    } else {
        for &key in sequence(d, field(d, d.root(), "keys")?)? {
            register(text(d, key)?);
        }
    }
    if let Some(range) = d
        .field(item, "between")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        register(text(d, field(d, range, "value")?)?);
    }
    Ok(dependencies)
}

pub(super) fn dependencies(
    index: usize,
    items: &[Option<crate::dataset::Intermediate>],
) -> Result<Vec<usize>, BindError> {
    let mut columns = Vec::new();
    crate::dataset::visit_output_dependencies(
        index,
        |index| items.get(index).and_then(Option::as_ref),
        |column| columns.push(column),
    )
    .map_err(|_| BindError::Internal)?;
    columns.sort_unstable();
    columns.dedup();
    Ok(columns)
}

/// Bind one direct read using the same selector and actual completed outputs.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    reference: &str,
    path: &str,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    items: &[Option<crate::dataset::Intermediate>],
    output: &TableSchema,
    catalog: &Catalog,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let Some((index, field)) = declarations.reference(reference) else {
        return Ok(None);
    };
    let Some(item) = items.get(index).and_then(Option::as_ref) else {
        findings.push(BindFinding::UnknownReference {
            path: path.into(),
            name: reference.into(),
        });
        return Ok(None);
    };
    if let Some(reader) = declarations.reader() {
        if item.source.is_self() {
            let mut finding = LookupFinding::new(
                path,
                crate::diagnostic::ConditionCode::IntermediateDerivationSelfPhase,
            );
            finding.text("intermediate", reader);
            finding.text("identifier", reference);
            findings.push(BindFinding::Lookup(finding));
            return Ok(None);
        }
    }
    let Some(column) = item.source_column(secondary, field) else {
        findings.push(BindFinding::Lookup(LookupFinding::reference(
            path,
            reference,
            None,
            ReferenceCause::Value,
            None,
        )));
        return Ok(None);
    };
    let missing_dependency = |identifier: &str, key_path: &str| {
        if let Some(reader) = declarations.reader() {
            let mut finding = LookupFinding::new(
                path,
                crate::diagnostic::ConditionCode::IntermediateDerivationDependency,
            );
            finding.text("intermediate", reader);
            finding.text("identifier", identifier);
            finding.text("read", &item.identifier);
            BindFinding::Lookup(finding)
        } else {
            BindFinding::Lookup(LookupFinding::reference(
                key_path,
                identifier,
                Some(&item.identifier),
                ReferenceCause::OutputKey,
                None,
            ))
        }
    };
    let before = findings.len();
    for key in &item.keys {
        if let crate::dataset::MatchValue::Contextual(declaration) = &key.value {
            findings.push(missing_dependency(declaration.identifier(), &key.path));
            continue;
        }
        for read in key.value.reads() {
            if let crate::dataset::MatchRead::Output { column, identifier } = read {
                if available.is_some_and(|available| !available.contains(column)) {
                    findings.push(missing_dependency(identifier, &key.path));
                    continue;
                }
            }
            if let crate::dataset::MatchRead::SecondaryRecord { identifier, .. } = read {
                if catalog.bind(identifier).is_some_and(|read| {
                    matches!(read, reference_binding::Binding::Dataset { dataset: 0, .. })
                }) {
                    continue;
                }
                findings.push(missing_dependency(identifier, &key.path));
            }
        }
    }
    if findings.len() != before {
        return Ok(None);
    }
    let donor = item.source_schema(secondary).ok_or(BindError::Internal)?;
    if item.keys.is_empty() {
        if let Some(root_keys) = declarations.driver_keys(index) {
            let keys = (0..root_keys.columns().len()).collect::<Vec<_>>();
            let fields = root_keys
                .columns()
                .iter()
                .map(|column| reference_binding::Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>();
            let root_catalog = Catalog::compile(&fields, &[]).map_err(BindError::Catalog)?;
            // A scalar read still needs lookup keys even when this relation also drives rows.
            let dataset = match &item.source {
                crate::dataset::IntermediateSource::Input(index) => secondary
                    .get(*index)
                    .map(|source| source.name.as_str())
                    .ok_or(BindError::Internal)?,
                crate::dataset::IntermediateSource::Primary { name, .. } => name.as_str(),
                crate::dataset::IntermediateSource::SelfRows { .. } => "SELF",
            };
            intermediate_admission::inferred_keys(
                &root_catalog,
                root_keys,
                &keys,
                donor,
                &|column| item.source_kind(secondary, column),
                &|column| item.admits_donor_column(column),
                &item.path,
                dataset,
                findings,
            )?;
            return Ok(None);
        }
    }
    for key in &item.keys {
        let read_kind = key
            .value
            .read()
            .and_then(|read| catalog.type_of(read.identifier()));
        let field = donor
            .columns()
            .get(key.source_column)
            .ok_or(BindError::Internal)?;
        let kind = key.value.result_type(read_kind);
        if let Some(donor_kind) = item.source_kind(secondary, key.source_column) {
            if !kind.comparable(donor_kind.into()) {
                let name = if key.value.is_expression() {
                    format!("key[{}]", field.name)
                } else {
                    key.value
                        .read()
                        .map_or(field.name.clone(), |read| read.identifier().into())
                };
                findings.push(BindFinding::Lookup(LookupFinding::key_type(
                    &format!("{}.key", item.path),
                    &item.identifier,
                    &name,
                    kind,
                    donor_kind,
                )));
            }
        }
    }
    if findings.len() != before {
        return Ok(None);
    }
    for dependency in dependencies(index, items)? {
        let name = &output.columns()[dependency].name;
        if let Some(finding) = catalog
            .validate_output(name, None, available, &[0])
            .map_err(BindError::Catalog)?
        {
            findings.push(BindFinding::OutputReference {
                path: path.into(),
                name: name.clone(),
                finding,
            });
        } else {
            edges.push(dependency);
        }
    }
    Ok((findings.len() == before).then_some(Expression::Intermediate { index, column }))
}
