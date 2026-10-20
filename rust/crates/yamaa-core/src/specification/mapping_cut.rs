//! One mapping/cut declaration and binder shared by root, row and case dispatch.
use super::*;
use crate::{
    mapping_cut::{AdmissionError, Cut, Mapping, MappingCut},
    value::ColumnType,
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub source: source_expressions::Declaration,
    pub operation: MappingCut,
}
fn admission<T>(result: Result<T, AdmissionError>) -> Result<T, PrepareError> {
    result.map_err(|error| match error {
        AdmissionError::Shape => PrepareError::Internal,
        AdmissionError::Diagnostic(finding) => {
            PrepareError::Invalid(vec![PreflightFinding::Scalar(finding)])
        }
    })
}
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    named: &intermediates::Declarations,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Declaration, PrepareError> {
    let fields = if op == "mapping" {
        ["source", "dict", "case_sensitive", "missing", "unmapped"].as_slice()
    } else {
        ["source", "breaks", "labels", "right", "missing"].as_slice()
    };
    for &(key, _) in mapping(d, payload)? {
        let name = text(d, key)?;
        if !fields.contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let source = source_expressions::Declaration::prepare(
        d,
        field(d, payload, "source")?,
        &format!("{path}.source"),
        extra,
        named,
    )?;
    let missing = d
        .field(payload, "missing")
        .map(|id| literal(d, id, &format!("{path}.missing")))
        .transpose()?;
    let operation = if op == "mapping" {
        let dictionary = field(d, payload, "dict")?;
        if matches!(d.nodes()[dictionary], N::Text(_)) {
            reject(extra, "mapping_dictionary_resource", format!("{path}.dict"));
        }
        let entries = if matches!(d.nodes()[dictionary], N::Mapping(_)) {
            mapping(d, dictionary)?
                .iter()
                .map(|&(key, value)| {
                    Ok((
                        String::from(text(d, key)?),
                        literal(d, value, &format!("{path}.dict.{}", text(d, key)?))?,
                    ))
                })
                .collect::<Result<Vec<_>, PrepareError>>()?
        } else {
            vec![]
        };
        let case_sensitive = match d.field(payload, "case_sensitive").map(|id| &d.nodes()[id]) {
            Some(N::Boolean(value)) => *value,
            None => true,
            _ => return Err(PrepareError::Internal),
        };
        let unmapped = d
            .field(payload, "unmapped")
            .map(|id| literal(d, id, &format!("{path}.unmapped")))
            .transpose()?;
        MappingCut::Mapping(admission(Mapping::new(
            source.variable.clone(),
            path.into(),
            entries,
            case_sensitive,
            missing,
            unmapped,
        ))?)
    } else {
        let breaks = sequence(d, field(d, payload, "breaks")?)?
            .iter()
            .map(|&id| match &d.nodes()[id] {
                N::Float(value) => Ok(*value),
                N::Integer(value) => value.parse::<f64>().map_err(|_| PrepareError::Internal),
                _ => Err(PrepareError::Internal),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let labels = sequence(d, field(d, payload, "labels")?)?
            .iter()
            .map(|&id| text(d, id).map(String::from))
            .collect::<Result<Vec<_>, _>>()?;
        let right = match d.field(payload, "right").map(|id| &d.nodes()[id]) {
            Some(N::Boolean(value)) => *value,
            None => false,
            _ => return Err(PrepareError::Internal),
        };
        let findings = Cut::declaration_findings(path, &breaks, &labels);
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(
                findings.into_iter().map(PreflightFinding::Scalar).collect(),
            ));
        }
        MappingCut::Cut(admission(Cut::new(
            source.variable.clone(),
            path.into(),
            breaks,
            labels,
            right,
            missing,
        ))?)
    };
    Ok(Declaration { source, operation })
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
fn checked_wrap(
    declaration: &Declaration,
    column: usize,
    expression: Expression,
    kind: Option<ColumnType>,
    findings: &mut Vec<BindFinding>,
) -> Option<Expression> {
    if let Some(kind) = kind.filter(|&kind| !declaration.operation.accepts(kind)) {
        findings.push(BindFinding::Scalar(
            declaration.operation.input_diagnostic(kind_name(kind)),
        ));
        return None;
    }
    Some(Expression::MappingCut {
        source: std::boxed::Box::new(Assignment {
            column,
            // Collection multiplicity belongs to the enclosing mapping/cut operation.
            // Static source/type binding still uses the authored `.source` leaf.
            path: declaration.operation.path().into(),
            expression,
        }),
        operation: declaration.operation.clone(),
    })
}
/// The structured source binds as the scope decides; its input type is a
/// lookup read's donor type or the catalog's type of the name.
pub(super) fn bind(
    declaration: &Declaration,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", declaration.operation.path());
    let name = &declaration.source.variable;
    let expression = source_expressions::bind(cx, &declaration.source, column, &path)?;
    let kind = if let Some((index, field)) = cx.donors.named.reference(name) {
        cx.donors
            .items
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|item| {
                let column = item.source_column(cx.donors.secondary, field)?;
                item.source_kind(cx.donors.secondary, column)
            })
    } else {
        cx.catalog.type_of(name)
    };
    Ok(expression
        .and_then(|expression| checked_wrap(declaration, column, expression, kind, cx.findings)))
}
