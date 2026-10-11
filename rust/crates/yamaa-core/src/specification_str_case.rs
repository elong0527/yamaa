//! Original string-case declarations bind named operands without coercion.
use super::*;
use crate::text_case::{Case, StringCase, MAX_DECLARATION_BYTES};

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    driver: Option<&str>,
    intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<StringCase, PrepareError> {
    for &(key, _) in mapping(d, payload)? {
        let name = text(d, key)?;
        if !["source", "to", "missing"].contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let source = text(d, field(d, payload, "source")?)?;
    let missing = d.field(payload, "missing");
    let missing_text = missing.and_then(|id| match &d.nodes()[id] {
        N::Text(text) => Some(text.as_str()),
        _ => None,
    });
    if !StringCase::metadata_fits(source, path, missing_text) {
        return Err(PrepareError::Limit("str_case_metadata"));
    }
    let mode = match text(d, field(d, payload, "to")?)? {
        "upper" => Case::Upper,
        "lower" => Case::Lower,
        "sentence" => Case::Sentence,
        "title" => Case::Title,
        // The original schema owns enum rejection, before compiler admission.
        _ => return Err(PrepareError::Internal),
    };
    if source.split_once('.').is_some_and(|(relation, _)| {
        driver.is_some_and(|driver| relation != driver)
            && !intermediates.is_some_and(|items| items.contains(relation))
    }) {
        reject(
            extra,
            "secondary_source_expression",
            format!("{path}.source"),
        );
    }
    StringCase::new(
        source.into(),
        path.into(),
        mode,
        missing
            .map(|id| literal(d, id, &format!("{path}.missing")))
            .transpose()?,
    )
    .ok_or(PrepareError::Internal)
}

pub(super) fn declaration_findings(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        d: &Document,
        value: usize,
        path: &str,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
        depth: usize,
        rows: bool,
        bytes: &mut usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 || path.len() > 65_536 {
            return Err(PrepareError::Limit("case_depth"));
        }
        if let Some(payload) = d.field(value, "str_case") {
            *bytes = bytes
                .checked_add(path.len())
                .and_then(|n| n.checked_add(512))
                .filter(|n| *n <= MAX_DECLARATION_BYTES)
                .ok_or(PrepareError::Limit("str_case_metadata"))?;
            for &(key, value) in mapping(d, payload)? {
                for id in [key, value] {
                    if let N::Text(text) | N::Integer(text) = &d.nodes()[id] {
                        *bytes = bytes
                            .checked_add(text.len())
                            .filter(|n| *n <= MAX_DECLARATION_BYTES)
                            .ok_or(PrepareError::Limit("str_case_metadata"))?;
                    }
                }
            }
            let operation = prepare(
                d,
                payload,
                &format!("{path}.str_case"),
                None,
                None,
                &mut Vec::new(),
            )?;
            let kind = if rows {
                None
            } else {
                output
                    .columns()
                    .iter()
                    .find(|item| item.name == operation.source())
                    .map(|item| item.kind)
            }
            .or(temporal::declared_source_type(d, operation.source())?);
            if let Some(kind) = kind.filter(|kind| *kind != ColumnType::Str) {
                let actual = match kind {
                    ColumnType::Int => "int",
                    ColumnType::Float => "float",
                    ColumnType::Date => "date",
                    ColumnType::DateTime => "datetime",
                    ColumnType::Str => unreachable!(),
                };
                findings.push(PreflightFinding::TextCase(
                    operation.input_diagnostic(actual),
                ));
            }
        }
        if let Some(case) = d.field(value, "case") {
            for (index, &branch) in sequence(d, case)?.iter().enumerate() {
                let (name, child) = d
                    .field(branch, "then")
                    .map(|id| ("then", id))
                    .or_else(|| d.field(branch, "otherwise").map(|id| ("otherwise", id)))
                    .ok_or(PrepareError::Internal)?;
                visit(
                    d,
                    child,
                    &format!("{path}.case[{index}].{name}"),
                    output,
                    findings,
                    depth + 1,
                    rows,
                    bytes,
                )?;
            }
        }
        Ok(())
    }
    let has_rows = present(d, d.root(), "rows");
    let mut bytes = 0;
    for (index, &column) in columns.iter().enumerate() {
        if let Some(value) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            visit(
                d,
                value,
                &format!("columns.{}.derivation", output.columns()[index].name),
                output,
                findings,
                0,
                has_rows,
                &mut bytes,
            )?;
        }
    }
    if let Some(rows) = d.field(d.root(), "rows") {
        for (index, &row) in sequence(d, rows)?.iter().enumerate() {
            if let Some(derivations) = d.field(row, "derivations") {
                for &(name, derivation) in mapping(d, derivations)? {
                    if let Some(value) = d.field(derivation, "value") {
                        visit(
                            d,
                            value,
                            &format!("rows[{index}].derivations.{}", text(d, name)?),
                            output,
                            findings,
                            0,
                            true,
                            &mut bytes,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &StringCase,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", operation.path());
    Ok(rounding::bind_operand(
        operation.source(),
        &path,
        column,
        keys,
        catalog,
        declarations,
        secondary,
        intermediates,
        edges,
        findings,
    )?
    .map(|expression| Expression::StrCase {
        source: alloc::boxed::Box::new(Assignment {
            column,
            path,
            expression,
        }),
        operation: operation.clone(),
    }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &StringCase,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", operation.path());
    Ok(rounding::bind_row_operand(
        operation.source(),
        &path,
        catalog,
        driver,
        row,
        phase,
        available,
        edges,
        findings,
    )?
    .map(|expression| Expression::StrCase {
        source: alloc::boxed::Box::new(Assignment {
            column,
            path,
            expression,
        }),
        operation: operation.clone(),
    }))
}
