//! Raw scalar extremes and their declared-type validation, before output conversion.
use crate::json::Value as JsonValue;
use crate::{
    diagnostic::{ConditionCode, Diagnostic},
    value::{compare_present, ColumnType, ComparisonError, Value},
};
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
