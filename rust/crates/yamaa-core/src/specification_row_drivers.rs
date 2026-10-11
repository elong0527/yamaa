//! Build per-template stored-source metadata through the existing source-port ordering.
use super::*;
pub(super) fn catalog(
    output: &TableSchema,
    source: &TableSchema,
    name: &str,
) -> Result<Catalog, BindError> {
    let outputs = output
        .columns()
        .iter()
        .map(|c| reference_binding::Field {
            name: &c.name,
            column_type: c.kind,
        })
        .collect::<Vec<_>>();
    let fields = source
        .columns()
        .iter()
        .map(|c| reference_binding::Field {
            name: &c.name,
            column_type: c.kind,
        })
        .collect::<Vec<_>>();
    Catalog::compile(
        &outputs,
        &[reference_binding::Dataset {
            name,
            fields: &fields,
        }],
        Default::default(),
    )
    .map_err(BindError::Catalog)
}
pub(super) fn select<'a>(
    source: &'a TableSchema,
    primary: &str,
    secondary: &'a [crate::dataset::SecondarySource],
    selected: &str,
) -> Result<(usize, &'a TableSchema), BindError> {
    if selected == primary {
        return Ok((0, source));
    }
    secondary
        .iter()
        .enumerate()
        .find(|(_, item)| item.name == selected)
        .map(|(index, item)| (index + 1, &item.schema))
        .ok_or(BindError::Internal)
}

/// Preserve an explicit before-study boundary for still-pending row lookup contexts.
#[allow(clippy::too_many_arguments)]
pub(super) fn admit_reads(
    d: &Document,
    declaration: usize,
    driver: Option<&str>,
    path: &str,
    project: Option<&functions::Prepared>,
    templates: &str_template::Prepared,
    named: &intermediates::Declarations,
) -> Result<(), PrepareError> {
    fn template_names(
        d: &Document,
        value: usize,
        templates: &str_template::Prepared,
        names: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 {
            return Err(PrepareError::Limit("expression_depth"));
        }
        if let Some(payload) = d.field(value, "str_template") {
            names.extend(templates.get(payload)?.identifiers().iter().cloned());
        }
        if let Some(payload) = d.field(value, "case") {
            for &branch in sequence(d, payload)? {
                if let Some(child) = d
                    .field(branch, "then")
                    .or_else(|| d.field(branch, "otherwise"))
                {
                    template_names(d, child, templates, names, depth + 1)?;
                }
            }
        }
        if let Some(payload) = d.field(value, "str_concat") {
            for &child in sequence(d, field(d, payload, "sources")?)? {
                template_names(d, child, templates, names, depth + 1)?;
            }
        }
        Ok(())
    }
    let value = field(d, declaration, "value")?;
    let mut names = Vec::new();
    derivations::references(d, value, project, &mut names, 0)?;
    template_names(d, value, templates, &mut names, 0)?;
    let matching = names
        .iter()
        .filter_map(|name| named.reference(name))
        .flat_map(|(index, _)| named.match_references(index))
        .map(String::from)
        .collect::<Vec<_>>();
    names.extend(matching);
    let inputs = field(d, d.root(), "input")?;
    if names.iter().any(|name| {
        name.split_once('.').is_some_and(|(source, _)| {
            d.field(inputs, source).is_some()
                && driver != Some(source)
                && named.reference(name).is_none()
        })
    }) {
        return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
            operation: if driver.is_some() {
                "row_secondary_source"
            } else {
                "row_column_driver_source"
            }
            .into(),
            path: path.into(),
        }]));
    }
    Ok(())
}
