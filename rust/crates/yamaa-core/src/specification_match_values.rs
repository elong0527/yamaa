//! Original key expressions bind in current-row scope, without a target column.
use super::*;
use crate::{
    dataset::{MatchRead, MatchValue},
    key_relation::ComparableType,
    match_value::{self, LiteralKind},
};

#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Variable(String),
    Source(String),
    Literal(crate::value::Value),
    StrPad(str_pad::Declaration),
}
impl Declaration {
    pub(super) fn prepare(
        d: &Document,
        value: usize,
        path: &str,
        sources: &[SourceDeclaration],
        driver: usize,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        if let N::Text(name) = &d.nodes()[value] {
            return Ok(Self::Variable(name.clone()));
        }
        let &[(operation, payload)] = mapping(d, value)? else {
            return Err(PrepareError::Internal);
        };
        let op = text(d, operation)?;
        let operation_path = format!("{path}.{op}");
        let declaration = match op {
            "literal" => Self::Literal(literal(d, payload, &operation_path)?),
            "source" => {
                let name = if let N::Text(name) = &d.nodes()[payload] {
                    name.as_str()
                } else {
                    for &(key, value) in mapping(d, payload)? {
                        let key = text(d, key)?;
                        if key != "variable" && !matches!(d.nodes()[value], N::Null) {
                            reject(
                                extra,
                                "intermediate_key_source",
                                format!("{operation_path}.{key}"),
                            );
                        }
                    }
                    text(d, field(d, payload, "variable")?)?
                };
                Self::Source(name.into())
            }
            "str_pad" => Self::StrPad(str_pad::prepare(
                d,
                payload,
                &operation_path,
                None,
                None,
                extra,
            )?),
            _ => {
                reject(extra, op, operation_path);
                // A rejected operation never becomes an executable plan.
                Self::Literal(crate::value::Value::Missing)
            }
        };
        if let Some(name) = declaration.expression_reference() {
            if let Some((relation, _)) = name.split_once('.') {
                if sources
                    .iter()
                    .enumerate()
                    .any(|(index, source)| index != driver && source.name == relation)
                {
                    reject(extra, "secondary_source_expression", path.into());
                }
                if d.field(d.root(), "intermediates").is_some_and(|items| {
                    sequence(d, items).is_ok_and(|items| {
                        items.iter().any(|&item| {
                            d.field(item, "id")
                                .is_some_and(|id| text(d, id).is_ok_and(|name| name == relation))
                        })
                    })
                }) {
                    reject(extra, "intermediate_key_expression", path.into());
                }
            }
        }
        Ok(declaration)
    }
    fn expression_reference(&self) -> Option<&str> {
        match self {
            Self::Source(name) => Some(name),
            Self::StrPad(pad) => Some(pad.source()),
            _ => None,
        }
    }
    pub(super) fn reference(&self) -> Option<&str> {
        match self {
            Self::Variable(name) | Self::Source(name) => Some(name),
            Self::StrPad(pad) => Some(pad.source()),
            Self::Literal(_) => None,
        }
    }
    pub(super) fn display_name(&self, field: &str) -> String {
        match self {
            Self::Variable(name) => name.clone(),
            _ => format!("key[{field}]"),
        }
    }
    pub(super) fn result_type(
        &self,
        catalog: &Catalog,
    ) -> Result<Option<ComparableType>, BindError> {
        let expression = match self {
            Self::Variable(name) | Self::Source(name) => match_value::Expression::Source(name),
            Self::StrPad(_) => match_value::Expression::Operation("str_pad"),
            Self::Literal(value) => match_value::Expression::Literal(match value {
                crate::value::Value::Missing => LiteralKind::Missing,
                crate::value::Value::Str(_) => LiteralKind::Str,
                crate::value::Value::Int(_) => LiteralKind::Int,
                crate::value::Value::Float(_) => LiteralKind::Float,
                crate::value::Value::Bool(_) => LiteralKind::Bool,
                _ => LiteralKind::Other,
            }),
        };
        match_value::result_type(catalog, expression).map_err(BindError::Catalog)
    }
    pub(super) fn bind(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<MatchValue>, BindError> {
        if let Self::StrPad(operation) = self {
            str_pad::declaration_findings(operation, findings);
        }
        let Some(name) = self.reference() else {
            let Self::Literal(value) = self else {
                return Err(BindError::Internal);
            };
            return Ok(Some(MatchValue::Literal(value.clone())));
        };
        let read = match catalog.bind(name).map_err(BindError::Catalog)? {
            Some(reference_binding::Binding::Output { column, .. }) => MatchRead::Output {
                column,
                identifier: name.into(),
            },
            Some(reference_binding::Binding::Dataset { field, .. }) => MatchRead::Record {
                column: field,
                identifier: name.into(),
            },
            None => {
                let suggestion = name
                    .strip_prefix("SELF.")
                    .filter(|bare| {
                        catalog.bind(bare).is_ok_and(|item| {
                            matches!(item, Some(reference_binding::Binding::Output { .. }))
                        })
                    })
                    .map(String::from);
                findings.push(BindFinding::Lookup(LookupFinding::reference(
                    path,
                    name,
                    Some(intermediate),
                    ReferenceCause::OutputKey,
                    suggestion,
                )));
                return Ok(None);
            }
        };
        Ok(Some(match self {
            Self::Variable(_) => MatchValue::Variable(read),
            Self::Source(_) => MatchValue::Source(read),
            Self::StrPad(operation) => MatchValue::StrPad {
                read,
                operation: match operation {
                    str_pad::Declaration::Valid(operation) => Some(operation.clone()),
                    str_pad::Declaration::WidthError { .. } => None,
                },
            },
            Self::Literal(_) => return Err(BindError::Internal),
        }))
    }
}

/// Reject aggregate key counts before the compiler owns paths or expressions.
pub(super) fn declaration_admission(
    d: &Document,
    limits: CompilationLimits,
) -> Result<(), PrepareError> {
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(());
    };
    let items = sequence(d, items)?;
    if items.len() > limits.columns {
        return Err(PrepareError::Limit("intermediates"));
    }
    for &item in items {
        if let Some(key) = d.field(item, "key") {
            if let N::Mapping(fields) = &d.nodes()[key] {
                if fields.len() > limits.keys {
                    return Err(PrepareError::Limit("intermediate_keys"));
                }
            }
        }
    }
    Ok(())
}
