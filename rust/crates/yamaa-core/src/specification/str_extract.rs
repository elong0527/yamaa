//! Whole-request text declaration admission, then reference-owned extraction binding.
//! `str_replace` shares the extraction declaration and its source binding; only
//! the bound expression differs.
use super::*;
use crate::text_extract::{AdmissionError, StringExtract};
use crate::text_replace::StringReplace;

#[derive(Debug)]
pub(super) struct Prepared {
    searches: BTreeMap<usize, StringExtract>,
    replacements: BTreeMap<usize, StringReplace>,
}
impl Prepared {
    pub(super) fn get(&self, payload: usize) -> Result<StringExtract, PrepareError> {
        self.searches
            .get(&payload)
            .cloned()
            .ok_or(PrepareError::Internal)
    }
    pub(super) fn replacement(&self, payload: usize) -> Result<StringReplace, PrepareError> {
        self.replacements
            .get(&payload)
            .cloned()
            .ok_or(PrepareError::Internal)
    }
    /// Admit each `str_extract` and `str_replace` payload at its path, in
    /// order, with whether a bare name it reads is left to binding (`rows`).
    pub(super) fn new(
        d: &Document,
        nodes: Vec<(usize, String, bool)>,
        declared: &types::DeclaredTypes<'_>,
        findings: &mut Vec<PreflightFinding>,
    ) -> Result<Self, PrepareError> {
        let mut searches = BTreeMap::new();
        let mut replacements = BTreeMap::new();
        for (payload, path, rows) in nodes {
            let path = path.as_str();
            let source = text(d, field(d, payload, "source")?)?;
            let pattern = text(d, field(d, payload, "pattern")?)?;
            let missing = d
                .field(payload, "missing")
                .map(|id| literal(d, id, &format!("{path}.missing")))
                .transpose()?;
            let replacement = d.field(payload, "replacement");
            let admitted = match replacement {
                Some(replacement) => StringReplace::new(
                    source.into(),
                    path.into(),
                    pattern.into(),
                    text(d, replacement)?.into(),
                    missing,
                )
                .map(Admitted::Replace),
                None => StringExtract::new(
                    source.into(),
                    path.into(),
                    pattern.into(),
                    group(d, payload)?.into(),
                    missing,
                    d.field(payload, "no_match")
                        .map(|id| literal(d, id, &format!("{path}.no_match")))
                        .transpose()?,
                )
                .map(Admitted::Search),
            }
            .map_err(|error| match error {
                AdmissionError::Shape => PrepareError::Internal,
                AdmissionError::Regex(crate::portable_regex::CompileError::Limit {
                    resource,
                    limit,
                }) => PrepareError::limit(
                    compiler_limits::regex_resource(resource),
                    limit,
                    None,
                    Some(&format!("{path}.pattern")),
                ),
                // An invalid pattern is a deferred finding, never an admission error.
                AdmissionError::Regex(crate::portable_regex::CompileError::Invalid { .. }) => {
                    PrepareError::Internal
                }
            })?;
            let operation = match &admitted {
                Admitted::Search(operation) => operation,
                Admitted::Replace(operation) => operation.search(),
            };
            // Row availability/group scope belongs to binding. Declared columns
            // and stored fields have their declared types in the no-rows model.
            let kind = (!rows)
                .then(|| declared.name(source).column_type())
                .flatten();
            if let Some(kind) = kind.filter(|kind| *kind != ColumnType::Str) {
                findings.push(PreflightFinding::TextExtract(
                    operation.input_diagnostic(kind_name(kind)),
                ));
            } else if kind == Some(ColumnType::Str) {
                if let Some(diagnostic) = operation.declaration_diagnostic() {
                    findings.push(PreflightFinding::TextExtract(diagnostic.clone()));
                }
            }
            match admitted {
                Admitted::Search(operation) => {
                    searches.insert(payload, operation);
                }
                Admitted::Replace(operation) => {
                    replacements.insert(payload, operation);
                }
            }
        }
        Ok(Self {
            searches,
            replacements,
        })
    }
}
enum Admitted {
    Search(StringExtract),
    Replace(StringReplace),
}
fn group(d: &Document, payload: usize) -> Result<&str, PrepareError> {
    match d.field(payload, "group").map(|id| &d.nodes()[id]) {
        Some(N::Integer(text)) => Ok(text),
        None => Ok("0"),
        _ => Err(PrepareError::Internal),
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
    if let Some(kind) = catalog.type_of(operation.source()) {
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
        source: std::boxed::Box::new(Assignment {
            column,
            path: format!("{}.source", operation.path()),
            expression,
        }),
        operation: operation.clone(),
    }))
}
/// The source binds as the scope decides; an unknown source is the text
/// operation's own finding.
pub(super) fn bind(
    operation: &StringExtract,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let start = cx.findings.len();
    let source = cx.operand(
        operation.source(),
        &format!("{}.source", operation.path()),
    )?;
    finish(operation, source, start, cx.catalog, cx.findings, column)
}
fn replaced(expression: Expression, operation: &StringReplace) -> Expression {
    match expression {
        Expression::StrExtract { source, .. } => Expression::StrReplace {
            source,
            operation: operation.clone(),
        },
        _ => unreachable!("extraction binding"),
    }
}
/// A replacement binds its search as an extraction does.
pub(super) fn bind_replace(
    operation: &StringReplace,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    Ok(bind(operation.search(), column, cx)?.map(|expression| replaced(expression, operation)))
}
