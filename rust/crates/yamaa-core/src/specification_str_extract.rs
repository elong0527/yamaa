//! Whole-request text declaration admission, then reference-owned extraction binding.
use super::*;
use crate::text_extract::{AdmissionError, StringExtract, MAX_DECLARATION_BYTES};

pub(super) struct Prepared(BTreeMap<usize, StringExtract>);
impl Prepared {
    pub(super) fn get(&self, payload: usize) -> Result<StringExtract, PrepareError> {
        self.0.get(&payload).cloned().ok_or(PrepareError::Internal)
    }
    pub(super) fn new(
        d: &Document,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
    ) -> Result<Self, PrepareError> {
        // Complete admission precedes every owned declaration or parser arena.
        let mut bytes = 0usize;
        walk(d, &mut |payload, path, _rows| {
            let source = text(d, field(d, payload, "source")?)?;
            let pattern = text(d, field(d, payload, "pattern")?)?;
            let group = group(d, payload)?;
            let handlers = ["missing", "no_match"]
                .into_iter()
                .filter_map(|name| d.field(payload, name))
                .map(|id| match &d.nodes()[id] {
                    N::Text(text) | N::Integer(text) => text.len(),
                    _ => 0,
                });
            let amount = StringExtract::declaration_bytes(source, path, pattern, group, handlers)
                .ok_or(PrepareError::Limit("str_extract_metadata"))?;
            bytes = bytes
                .checked_add(amount)
                .filter(|n| *n <= MAX_DECLARATION_BYTES)
                .ok_or(PrepareError::Limit("str_extract_metadata"))?;
            Ok(())
        })?;
        let mut operations = BTreeMap::new();
        let mut budget = crate::regex::CompileBudget::new(Default::default());
        walk(d, &mut |payload, path, rows| {
            let source = text(d, field(d, payload, "source")?)?;
            let operation = StringExtract::with_budget(
                source.into(),
                path.into(),
                text(d, field(d, payload, "pattern")?)?.into(),
                group(d, payload)?.into(),
                d.field(payload, "missing")
                    .map(|id| literal(d, id, &format!("{path}.missing")))
                    .transpose()?,
                d.field(payload, "no_match")
                    .map(|id| literal(d, id, &format!("{path}.no_match")))
                    .transpose()?,
                &mut budget,
            )
            .map_err(|error| match error {
                AdmissionError::Shape => PrepareError::Internal,
                AdmissionError::Limit
                | AdmissionError::Regex(crate::regex::CompileError::Limit { .. }) => {
                    PrepareError::Limit("str_extract_compile")
                }
                AdmissionError::Regex(_) => PrepareError::Unsupported(vec![UnsupportedFeature {
                    operation: "str_extract_pattern".into(),
                    path: format!("{path}.pattern"),
                }]),
            })?;
            // Row availability/group scope belongs to binding. Bare declared
            // column sources have complete static types in the no-rows model.
            let known_output = if rows {
                None
            } else {
                output.columns().iter().find(|item| item.name == source)
            };
            let kind = if rows {
                None
            } else {
                known_output
                    .map(|item| item.kind)
                    .or(temporal::declared_source_type(d, source)?)
            };
            if let Some(kind) = kind.filter(|kind| *kind != ColumnType::Str) {
                findings.push(PreflightFinding::TextExtract(
                    operation.input_diagnostic(kind_name(kind)),
                ));
            } else if known_output.is_some() {
                if let Some(diagnostic) = operation.declaration_diagnostic() {
                    findings.push(PreflightFinding::TextExtract(diagnostic.clone()));
                }
            }
            operations.insert(payload, operation);
            Ok(())
        })?;
        Ok(Self(operations))
    }
}
fn group(d: &Document, payload: usize) -> Result<&str, PrepareError> {
    match d.field(payload, "group").map(|id| &d.nodes()[id]) {
        Some(N::Integer(text)) => Ok(text),
        None => Ok("0"),
        _ => Err(PrepareError::Internal),
    }
}
fn walk(
    d: &Document,
    visit: &mut impl FnMut(usize, &str, bool) -> Result<(), PrepareError>,
) -> Result<(), PrepareError> {
    fn expression(
        d: &Document,
        id: usize,
        path: &str,
        rows: bool,
        depth: usize,
        visit: &mut impl FnMut(usize, &str, bool) -> Result<(), PrepareError>,
    ) -> Result<(), PrepareError> {
        if depth > 64 || path.len() > 65_536 {
            return Err(PrepareError::Limit("str_extract_depth"));
        }
        for name in ["str_extract", "str_contains"] {
            if let Some(payload) = d.field(id, name) {
                visit(payload, &format!("{path}.{name}"), rows)?;
            }
        }
        if let Some(payload) = d.field(id, "str_concat") {
            for (index, &child) in sequence(d, field(d, payload, "sources")?)?
                .iter()
                .enumerate()
            {
                expression(
                    d,
                    child,
                    &format!("{path}.str_concat.sources[{index}]"),
                    rows,
                    depth + 1,
                    visit,
                )?;
            }
        }
        if let Some(branches) = d.field(id, "case") {
            for (index, &branch) in sequence(d, branches)?.iter().enumerate() {
                for name in ["then", "otherwise"] {
                    if let Some(child) = d.field(branch, name) {
                        expression(
                            d,
                            child,
                            &format!("{path}.case[{index}].{name}"),
                            rows,
                            depth + 1,
                            visit,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    let rows = present(d, d.root(), "rows");
    for &column in sequence(d, field(d, d.root(), "columns")?)? {
        if let Some(value) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            expression(
                d,
                value,
                &format!("columns.{}.derivation", text(d, field(d, column, "name")?)?),
                rows,
                0,
                visit,
            )?;
        }
    }
    if let Some(items) = d.field(d.root(), "rows") {
        for (index, &row) in sequence(d, items)?.iter().enumerate() {
            if let Some(items) = d.field(row, "derivations") {
                for &(name, value) in mapping(d, items)? {
                    if let Some(value) = d.field(value, "value") {
                        expression(
                            d,
                            value,
                            &format!("rows[{index}].derivations.{}", text(d, name)?),
                            true,
                            0,
                            visit,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn kind_name(kind: ColumnType) -> &'static str {
    match kind {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}
fn finish(
    operation: &StringExtract,
    source: Option<Expression>,
    start: usize,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
    column: usize,
) -> Result<Option<Expression>, BindError> {
    if source.is_none() {
        for finding in &mut findings[start..] {
            if matches!(
                finding,
                BindFinding::UnknownReference { .. }
                    | BindFinding::OutputReference {
                        finding: reference_binding::Diagnostic::UnknownField,
                        ..
                    }
                    | BindFinding::QualifiedReference {
                        finding: crate::reference_scope::Finding::UnknownField,
                        ..
                    }
            ) {
                *finding = BindFinding::TextExtract(operation.unknown_diagnostic());
            }
        }
        return Ok(None);
    }
    // Scope/availability succeeds before type and retained pattern/group facts.
    if let Some(kind) = catalog
        .type_of(operation.source())
        .map_err(BindError::Catalog)?
    {
        if kind != ColumnType::Str {
            findings.push(BindFinding::TextExtract(
                operation.input_diagnostic(kind_name(kind)),
            ));
            return Ok(None);
        }
    }
    if let Some(diagnostic) = operation.declaration_diagnostic() {
        findings.push(BindFinding::TextExtract(diagnostic.clone()));
        return Ok(None);
    }
    Ok(source.map(|expression| Expression::StrExtract {
        source: alloc::boxed::Box::new(Assignment {
            column,
            path: format!("{}.source", operation.path()),
            expression,
        }),
        operation: operation.clone(),
    }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &StringExtract,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let source = rounding::bind_operand(
        operation.source(),
        &format!("{}.source", operation.path()),
        column,
        keys,
        catalog,
        declarations,
        secondary,
        intermediates,
        edges,
        findings,
    )?;
    finish(operation, source, start, catalog, findings, column)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &StringExtract,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let source = rounding::bind_row_operand(
        operation.source(),
        &format!("{}.source", operation.path()),
        catalog,
        driver,
        row,
        phase,
        available,
        edges,
        findings,
    )?;
    finish(operation, source, start, catalog, findings, column)
}
