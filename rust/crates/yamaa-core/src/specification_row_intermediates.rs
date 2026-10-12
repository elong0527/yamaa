//! Bridge named input selections into one completed record context.
//! The matching language remains owned by the shared match-value compiler.
use super::*;

const METADATA_BYTES: usize = 16_777_216;
const BINDING_WORK: usize = 100_000_000;

pub(super) fn named_reference(d: &Document, reference: &str) -> Result<bool, PrepareError> {
    let Some((name, _)) = reference.split_once('.') else {
        return Ok(false);
    };
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(false);
    };
    Ok(sequence(d, items)?.iter().any(|&item| {
        d.field(item, "id")
            .is_some_and(|id| text(d, id).is_ok_and(|id| id == name))
    }))
}

/// Refuse aggregate binding metadata/work before cloning secondary schemas.
pub(super) fn admit_sources(
    schemas: &[&TableSchema],
    output: &TableSchema,
    templates: usize,
    paths: impl Iterator<Item = usize>,
) -> Result<(), BindError> {
    let limit = |resource, limit, required| {
        BindError::Catalog(reference_binding::Error::Limit {
            resource,
            limit,
            required,
        })
    };
    let overflow = || BindError::Catalog(reference_binding::Error::SizeOverflow);
    let mut bytes = 0usize;
    let mut fields = 0usize;
    for column in schemas
        .iter()
        .flat_map(|schema| schema.columns())
        .chain(output.columns())
    {
        fields = fields.checked_add(1).ok_or_else(overflow)?;
        bytes = bytes
            .checked_add(column.name.len())
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<crate::table::Column>()))
            .ok_or_else(overflow)?;
    }
    let mut reads = 0usize;
    for path in paths {
        reads = reads.checked_add(1).ok_or_else(overflow)?;
        bytes = bytes.checked_add(path).ok_or_else(overflow)?;
    }
    // Schemas/catalogs and actual dependency/finding copies are bounded together.
    let required = bytes
        .checked_mul(reads.checked_add(templates).ok_or_else(overflow)?.max(1))
        .and_then(|bytes| bytes.checked_mul(8))
        .ok_or_else(overflow)?;
    if required > METADATA_BYTES {
        return Err(limit("row_intermediate_metadata", METADATA_BYTES, required));
    }
    let required = fields
        .checked_mul(reads)
        .and_then(|work| work.checked_mul(fields))
        .ok_or_else(overflow)?;
    if required > BINDING_WORK {
        return Err(limit("row_intermediate_work", BINDING_WORK, required));
    }
    Ok(())
}

/// Select a common authored input without introducing a root base/default.
pub(super) fn common_driver<'a>(
    d: &'a Document,
    inputs: &[(usize, usize)],
) -> Result<&'a str, PrepareError> {
    let rows = sequence(d, field(d, d.root(), "rows")?)?;
    let mut common = None;
    for (index, &row) in rows.iter().enumerate() {
        let selected = d
            .field(row, "dataset")
            .filter(|&id| !matches!(d.nodes()[id], N::Null));
        let name = match selected {
            Some(id) => text(d, id)?,
            None if inputs.len() == 1 => text(d, inputs[0].0)?,
            // Invalid/missing names have already failed model preflight.
            None => return Err(PrepareError::Internal),
        };
        if !inputs
            .iter()
            .any(|&(id, _)| text(d, id).is_ok_and(|input| input == name))
        {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "row_driver".into(),
                path: format!("rows[{index}].dataset"),
            }]));
        }
        common.get_or_insert(name);
    }
    common.ok_or(PrepareError::Internal)
}

/// Whole-occurrence admission precedes expanded defaults and owned read paths.
pub(super) fn admit(d: &Document, templates: usize) -> Result<(), PrepareError> {
    if !present(d, d.root(), "intermediates") {
        return Ok(());
    }
    let bytes = d.nodes().iter().try_fold(0usize, |bytes, node| {
        let text = match node {
            N::Text(text) | N::Integer(text) => text.len(),
            _ => 0,
        };
        bytes
            .checked_add(text)?
            .checked_add(core::mem::size_of::<N>())
    });
    let expanded = bytes
        .and_then(|bytes| bytes.checked_mul(templates.checked_add(1)?))
        .and_then(|bytes| bytes.checked_mul(8));
    if expanded.is_none_or(|bytes| bytes > METADATA_BYTES) {
        return Err(PrepareError::Limit("row_intermediate_metadata"));
    }
    let work = d
        .nodes()
        .len()
        .checked_mul(d.nodes().len())
        .and_then(|work| work.checked_mul(templates.checked_add(1)?));
    if work.is_none_or(|work| work > BINDING_WORK) {
        return Err(PrepareError::Limit("row_intermediate_work"));
    }
    Ok(())
}

/// Borrow references from admitted key occurrences, without lowering them again.
/// Only direct named source reads can promote matching output defaults.
pub(super) fn default_dependencies(
    d: &Document,
    reference: &str,
    output: &TableSchema,
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
        // Preparation has already admitted the closed map-valued key language.
        for &(_, value) in mapping(d, key)? {
            if let N::Text(name) = &d.nodes()[value] {
                register(name);
                continue;
            }
            let &[(op, payload)] = mapping(d, value)? else {
                return Err(PrepareError::Internal);
            };
            match text(d, op)? {
                "source" => register(match &d.nodes()[payload] {
                    N::Text(name) => name,
                    _ => text(d, field(d, payload, "variable")?)?,
                }),
                "str_pad" => register(text(d, field(d, payload, "source")?)?),
                _ => (),
            }
        }
    } else {
        for &key in sequence(d, field(d, d.root(), "keys")?)? {
            register(text(d, key)?);
        }
    }
    Ok(dependencies)
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
    let Some(item) = &items[index] else {
        findings.push(BindFinding::UnknownReference {
            path: path.into(),
            name: reference.into(),
        });
        return Ok(None);
    };
    let Some(column) = secondary[item.source]
        .schema
        .columns()
        .iter()
        .position(|column| column.name == field)
    else {
        findings.push(BindFinding::Lookup(LookupFinding::reference(
            path,
            reference,
            None,
            ReferenceCause::Value,
            None,
        )));
        return Ok(None);
    };
    let before = findings.len();
    for dependency in item.output_dependencies() {
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
