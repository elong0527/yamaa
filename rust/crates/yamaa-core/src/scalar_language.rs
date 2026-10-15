//! Raw scalar extremes and concatenation validation, before output conversion.
use crate::{
    diagnostic::{ConditionCode, Diagnostic},
    value::{compare_present, ColumnType, ComparisonError, Value},
};
use serde_json::Value as JsonValue;
use std::cmp::Ordering;
use std::vec;
/// Missing operands are skipped; equality keeps the first authored value.
pub fn replace_extreme(
    selected: &Value,
    value: &Value,
    largest: bool,
) -> Result<bool, ComparisonError> {
    if matches!(value, Value::Missing) {
        return Ok(false);
    }
    if matches!(selected, Value::Missing) {
        return Ok(true);
    }
    Ok(compare_present(value, selected)?
        == if largest {
            Ordering::Greater
        } else {
            Ordering::Less
        })
}
pub fn concat_diagnostic(path: &str, index: usize, value: &Value) -> Option<Diagnostic> {
    if matches!(value, Value::Str(_)) {
        return None;
    }
    let source = format!("sources[{index}]");
    let (code, site, context) = match value {
        Value::Str(_) => return None,
        Value::Missing => (
            ConditionCode::StrCaseMissing,
            String::from(path),
            vec![("variable", JsonValue::String(source))],
        ),
        other => {
            let actual = match other {
                Value::Int(_) => "int",
                Value::Float(_) => "float",
                Value::Bool(_) => "bool",
                Value::Date(_) => "date",
                Value::DateTime(_) => "datetime",
                _ => unreachable!(),
            };
            (
                ConditionCode::StrCaseInput,
                format!("{path}.{source}"),
                vec![
                    ("source", JsonValue::String(source)),
                    ("expected", JsonValue::String("str".into())),
                    ("actual", JsonValue::String(actual.into())),
                ],
            )
        }
    };
    Some(Diagnostic {
        code,
        spec_paths: vec![site],
        context: context
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        source_span: None,
        operand_route: None,
    })
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
pub fn incomparable_diagnostic(sources: &[String], types: &[ColumnType], path: &str) -> Diagnostic {
    let texts =
        |items: Vec<String>| JsonValue::Array(items.into_iter().map(JsonValue::String).collect());
    Diagnostic {
        code: ConditionCode::ExtremeIncomparable,
        spec_paths: vec![path.into()],
        context: [
            ("sources".into(), texts(sources.to_vec())),
            (
                "types".into(),
                texts(types.iter().map(|&kind| kind_name(kind).into()).collect()),
            ),
        ]
        .into_iter()
        .collect(),
        source_span: None,
        operand_route: None,
    }
}
