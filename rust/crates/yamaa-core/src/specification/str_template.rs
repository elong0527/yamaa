//! Whole-occurrence template admission followed by shared reference binding.
use super::*;
use crate::text_template::StringTemplate;
#[derive(Debug)]
pub(super) struct Prepared(BTreeMap<usize, StringTemplate>);
impl Prepared {
    pub(super) fn get(&self, payload: usize) -> Result<StringTemplate, PrepareError> {
        self.0.get(&payload).cloned().ok_or(PrepareError::Internal)
    }
    /// Admit each `str_template` payload at its path, in order, with
    /// whether a bare name it reads is left to binding (`rows`). `columns`
    /// are the declared output columns.
    pub(super) fn new(
        d: &Document,
        columns: &[&str],
        nodes: Vec<(usize, String, bool)>,
        declared: &types::DeclaredTypes<'_>,
        findings: &mut Vec<PreflightFinding>,
    ) -> Result<Self, PrepareError> {
        let mut operations = BTreeMap::new();
        for (payload, path, rows) in nodes {
            let path = path.as_str();
            let operation = StringTemplate::new(
                text(d, field(d, payload, "template")?)?.into(),
                path.into(),
                d.field(payload, "missing")
                    .map(|id| literal(d, id, &format!("{path}.missing")))
                    .transpose()?,
            )
            .map_err(|_| PrepareError::Internal)?;
            if let Some(error) = operation.declaration_diagnostic() {
                findings.push(PreflightFinding::TextTemplate(error.clone()));
            } else if !rows {
                for (index, source) in operation.identifiers().iter().enumerate() {
                    if !source.contains('.') && !columns.contains(&source.as_str()) {
                        findings.push(PreflightFinding::TextTemplate(
                            operation.unknown_diagnostic(index).expect("admitted index"),
                        ));
                    } else if let Some(kind) = declared
                        .name(source)
                        .column_type()
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
        }
        Ok(Self(operations))
    }
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
        // Bare output names have the text-operation rule; qualified names
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
    if let Some(kind) = catalog.type_of(source) {
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
/// Each placeholder binds as the scope decides, at the template's path.
pub(super) fn bind(
    operation: &StringTemplate,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let start = cx.findings.len();
    let mut operands = Vec::new();
    if let Some(diagnostic) = operation.declaration_diagnostic() {
        cx.findings
            .push(BindFinding::TextTemplate(diagnostic.clone()));
        return Ok(None);
    }
    for (index, source) in operation.identifiers().iter().enumerate() {
        let at = cx.findings.len();
        let path = format!("{}.template", operation.path());
        let expression = cx.operand(source, &path)?;
        if let Some(expression) =
            finish(operation, source, index, expression, at, cx.catalog, cx.findings)?
        {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    if cx.findings.len() != start {
        Ok(None)
    } else {
        Ok(Some(Expression::StrTemplate {
            operands,
            operation: operation.clone(),
        }))
    }
}
