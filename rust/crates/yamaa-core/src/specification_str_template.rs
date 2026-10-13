//! Whole-occurrence template admission followed by shared reference binding.
use super::*;
use crate::text_template::{AdmissionError, CompileBudget, StringTemplate, MAX_DECLARATION_BYTES};
pub(super) struct Prepared(BTreeMap<usize, StringTemplate>);
impl Prepared {
    pub(super) fn get(&self, payload: usize) -> Result<StringTemplate, PrepareError> {
        self.0.get(&payload).cloned().ok_or(PrepareError::Internal)
    }
    pub(super) fn new(
        d: &Document,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
    ) -> Result<Self, PrepareError> {
        let mut bytes = 0usize;
        let mut budget = CompileBudget::new();
        // Admit all effective row-default copies and all catalogue queries before parsing.
        // A catalogue is bounded by the existing portable default reference limits.
        // The multiplier conservatively covers scope/binding's repeated tree searches.
        walk(d, &mut |payload, path, _rows, multiplicity| {
            let template = template(d, payload)?;
            let handler = d
                .field(payload, "missing")
                .map(|id| match &d.nodes()[id] {
                    N::Text(t) | N::Integer(t) => t.len(),
                    _ => 0,
                })
                .unwrap_or(0);
            let amount = StringTemplate::declaration_bytes(template, path, handler)
                .and_then(|n| n.checked_mul(multiplicity))
                .ok_or(PrepareError::Limit("str_template_metadata"))?;
            bytes = bytes
                .checked_add(amount)
                .filter(|n| *n <= MAX_DECLARATION_BYTES)
                .ok_or(PrepareError::Limit("str_template_metadata"))?;
            Ok(())
        })?;
        walk(d, &mut |payload, _path, _rows, multiplicity| {
            let template = template(d, payload)?;
            let query = template
                .len()
                .checked_mul(4096)
                .and_then(|n| n.checked_mul(multiplicity))
                .ok_or(PrepareError::Limit("str_template_work"))?;
            budget
                .charge(query)
                .map_err(|_| PrepareError::Limit("str_template_work"))?;
            Ok(())
        })?;
        let mut operations = BTreeMap::new();
        walk(d, &mut |payload, path, rows, _multiplicity| {
            let operation = StringTemplate::with_budget(
                template(d, payload)?.into(),
                path.into(),
                d.field(payload, "missing")
                    .map(|id| literal(d, id, &format!("{path}.missing")))
                    .transpose()?,
                &mut budget,
            )
            .map_err(|error| match error {
                AdmissionError::Shape => PrepareError::Internal,
                AdmissionError::Limit => PrepareError::Limit("str_template_compile"),
            })?;
            if let Some(error) = operation.declaration_diagnostic() {
                findings.push(PreflightFinding::TextTemplate(error.clone()));
            } else if !rows {
                for (index, source) in operation.identifiers().iter().enumerate() {
                    let output_kind = output
                        .columns()
                        .iter()
                        .find(|item| item.name == *source)
                        .map(|item| item.kind);
                    if !source.contains('.') && output_kind.is_none() {
                        findings.push(PreflightFinding::TextTemplate(
                            operation.unknown_diagnostic(index).expect("admitted index"),
                        ));
                    } else if let Some(kind) = output_kind
                        .or(temporal::declared_source_type(d, source)?)
                        .filter(|kind| *kind != ColumnType::Str)
                    {
                        findings.push(PreflightFinding::TextTemplate(
                            operation
                                .input_diagnostic(index, kind_name(kind))
                                .expect("admitted index"),
                        ));
                    }
                }
            }
            operations.insert(payload, operation);
            Ok(())
        })?;
        Ok(Self(operations))
    }
}
fn template(d: &Document, payload: usize) -> Result<&str, PrepareError> {
    if matches!(d.nodes()[payload], N::Text(_)) {
        text(d, payload)
    } else {
        text(d, field(d, payload, "template")?)
    }
}
fn walk(
    d: &Document,
    visit: &mut impl FnMut(usize, &str, bool, usize) -> Result<(), PrepareError>,
) -> Result<(), PrepareError> {
    fn expression(
        d: &Document,
        id: usize,
        path: &str,
        rows: bool,
        multiplicity: usize,
        depth: usize,
        visit: &mut impl FnMut(usize, &str, bool, usize) -> Result<(), PrepareError>,
    ) -> Result<(), PrepareError> {
        if depth > 64 || path.len() > 65_536 {
            return Err(PrepareError::Limit("str_template_depth"));
        }
        if let Some(payload) = d.field(id, "str_template") {
            visit(payload, &format!("{path}.str_template"), rows, multiplicity)?;
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
                    multiplicity,
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
                            multiplicity,
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
    let multiplicity = d
        .field(d.root(), "rows")
        .map(|id| sequence(d, id).map(|items| items.len().max(1)))
        .transpose()?
        .unwrap_or(1);
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
                multiplicity,
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
                            1,
                            0,
                            visit,
                        )?;
                    }
                }
            }
        }
    }
    if let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        for (index, &item) in sequence(d, items)?.iter().enumerate() {
            if let Some(items) = d
                .field(item, "derivations")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                for &(name, handled) in mapping(d, items)? {
                    expression(
                        d,
                        field(d, handled, "value")?,
                        &format!("intermediates[{index}].derivations.{}", text(d, name)?),
                        true,
                        1,
                        0,
                        visit,
                    )?;
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
    operation: &StringTemplate,
    source: &str,
    index: usize,
    expression: Option<Expression>,
    start: usize,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    if expression.is_none() {
        // Bare output names have the text-operation requirement; qualified names
        // retain their original resource/reference-scope owner and complete facts.
        if !source.contains('.') {
            for finding in &mut findings[start..] {
                if matches!(
                    finding,
                    BindFinding::UnknownReference { .. }
                        | BindFinding::OutputReference {
                            finding: reference_binding::Diagnostic::UnknownField,
                            ..
                        }
                ) {
                    *finding = BindFinding::TextTemplate(
                        operation.unknown_diagnostic(index).expect("admitted index"),
                    );
                }
            }
        }
        return Ok(None);
    }
    if let Some(kind) = catalog.type_of(source).map_err(BindError::Catalog)? {
        if kind != ColumnType::Str {
            findings.push(BindFinding::TextTemplate(
                operation
                    .input_diagnostic(index, kind_name(kind))
                    .expect("admitted index"),
            ));
            return Ok(None);
        }
    }
    Ok(expression)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &StringTemplate,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    bind_all(
        operation,
        column,
        catalog,
        findings,
        |source, path, findings| {
            rounding::bind_operand(
                source,
                path,
                column,
                keys,
                catalog,
                declarations,
                secondary,
                intermediates,
                edges,
                findings,
            )
        },
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &StringTemplate,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    bind_all(
        operation,
        column,
        catalog,
        findings,
        |source, path, findings| {
            rounding::bind_row_operand(
                source, path, catalog, driver, row, phase, available, edges, findings,
            )
        },
    )
}
fn bind_all(
    operation: &StringTemplate,
    column: usize,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
    mut bind: impl FnMut(&str, &str, &mut Vec<BindFinding>) -> Result<Option<Expression>, BindError>,
) -> Result<Option<Expression>, BindError> {
    let start = findings.len();
    let mut operands = Vec::new();
    if let Some(diagnostic) = operation.declaration_diagnostic() {
        findings.push(BindFinding::TextTemplate(diagnostic.clone()));
        return Ok(None);
    }
    for (index, source) in operation.identifiers().iter().enumerate() {
        let at = findings.len();
        let path = format!("{}.template", operation.path());
        let expression = bind(source, &path, findings)?;
        if let Some(expression) =
            finish(operation, source, index, expression, at, catalog, findings)?
        {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    if findings.len() != start {
        Ok(None)
    } else {
        Ok(Some(Expression::StrTemplate {
            operands,
            operation: operation.clone(),
        }))
    }
}
