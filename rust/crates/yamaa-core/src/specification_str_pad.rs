//! Original scalar padding shares named-reference and phase binding authority.
use super::*;
use crate::text_pad::{DeclarationError, StringPad, MAX_DECLARATION_BYTES};
#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Valid(StringPad),
    WidthError { source: String, path: String },
}
impl Declaration {
    pub(super) fn has_width_error(&self) -> bool {
        matches!(self, Self::WidthError { .. })
    }
    fn source(&self) -> &str {
        match self {
            Self::Valid(pad) => pad.source(),
            Self::WidthError { source, .. } => source,
        }
    }
    fn path(&self) -> &str {
        match self {
            Self::Valid(pad) => pad.path(),
            Self::WidthError { path, .. } => path,
        }
    }
    fn diagnostic(&self) -> Option<crate::diagnostic::Diagnostic> {
        match self {
            Self::Valid(_) => None,
            Self::WidthError { path, .. } => DeclarationError::Width.diagnostic(path),
        }
    }
}

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    driver: Option<&str>,
    intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Declaration, PrepareError> {
    for &(key, _) in mapping(d, payload)? {
        let name = text(d, key)?;
        if !["source", "width"].contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let source = text(d, field(d, payload, "source")?)?;
    let N::Integer(width) = &d.nodes()[field(d, payload, "width")?] else {
        return Err(PrepareError::Internal);
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
    match StringPad::new(source, path, width) {
        Ok(pad) => Ok(Declaration::Valid(pad)),
        Err(DeclarationError::Width) => Ok(Declaration::WidthError {
            source: source.into(),
            path: path.into(),
        }),
        Err(DeclarationError::MetadataLimit) => Err(PrepareError::Limit("str_pad_metadata")),
        Err(DeclarationError::Shape) => Err(PrepareError::Internal),
    }
}

/// Charge every occurrence in columns, rows and all nested cases before owning
/// any padding declaration. Validated source width/text is borrowed from its arena.
pub(super) fn declaration_admission(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
) -> Result<(), PrepareError> {
    fn visit(
        d: &Document,
        value: usize,
        path: &str,
        depth: usize,
        bytes: &mut usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 || path.len() > 65_536 {
            return Err(PrepareError::Limit("case_depth"));
        }
        if let Some(payload) = d.field(value, "str_pad") {
            let source = text(d, field(d, payload, "source")?)?;
            let N::Integer(width) = &d.nodes()[field(d, payload, "width")?] else {
                return Err(PrepareError::Internal);
            };
            let operation_path_bytes = path
                .len()
                .checked_add(".str_pad".len())
                .ok_or(PrepareError::Limit("str_pad_metadata"))?;
            let required = source
                .len()
                .checked_add(operation_path_bytes)
                .and_then(|n| n.checked_add(width.len()))
                .and_then(|n| n.checked_mul(8))
                .and_then(|n| n.checked_add(512))
                .ok_or(PrepareError::Limit("str_pad_metadata"))?;
            *bytes = bytes
                .checked_add(required)
                .filter(|&n| n <= MAX_DECLARATION_BYTES)
                .ok_or(PrepareError::Limit("str_pad_metadata"))?;
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
                    depth + 1,
                    bytes,
                )?;
            }
        }
        Ok(())
    }
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
                0,
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
                            0,
                            &mut bytes,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Report the operation's width defect at its enclosing derivation checkpoint,
/// before that derivation binds any predicate or source references. The complete
/// occurrence metadata was admitted before this declaration was owned.
pub(super) fn declaration_findings(operation: &Declaration, findings: &mut Vec<BindFinding>) {
    if let Some(diagnostic) = operation.diagnostic() {
        findings.push(BindFinding::Padding(diagnostic));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &Declaration,
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
    let bound = rounding::bind_operand(
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
    )?;
    if operation.has_width_error() {
        return Ok(None);
    }
    let Declaration::Valid(operation) = operation else {
        return Err(BindError::Internal);
    };
    Ok(bound.map(|expression| Expression::StrPad {
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
    operation: &Declaration,
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
    let bound = rounding::bind_row_operand(
        operation.source(),
        &path,
        catalog,
        driver,
        row,
        phase,
        available,
        edges,
        findings,
    )?;
    if operation.has_width_error() {
        return Ok(None);
    }
    let Declaration::Valid(operation) = operation else {
        return Err(BindError::Internal);
    };
    Ok(bound.map(|expression| Expression::StrPad {
        source: alloc::boxed::Box::new(Assignment {
            column,
            path,
            expression,
        }),
        operation: operation.clone(),
    }))
}
