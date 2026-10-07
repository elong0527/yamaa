//! Arrow C++ diagnostic spelling retained by the source condition contract.
use arrow_schema::{DataType, Field, IntervalUnit, TimeUnit, UnionMode};

pub(super) fn field_type(field: &Field) -> String {
    type_name(
        field.data_type(),
        field.name(),
        field.dict_is_ordered().unwrap_or(false),
    )
}

fn field_name(field: &Field) -> String {
    format!(
        "{}: {}{}",
        field.name(),
        field_type(field),
        if field.is_nullable() { "" } else { " not null" }
    )
}

fn unit(unit: &TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Second => "s",
        TimeUnit::Millisecond => "ms",
        TimeUnit::Microsecond => "us",
        TimeUnit::Nanosecond => "ns",
    }
}

fn type_name(kind: &DataType, name: &str, ordered: bool) -> String {
    let simple = match kind {
        DataType::Null => "null",
        DataType::Boolean => "bool",
        DataType::Int8 => "int8",
        DataType::Int16 => "int16",
        DataType::Int32 => "int32",
        DataType::Int64 => "int64",
        DataType::UInt8 => "uint8",
        DataType::UInt16 => "uint16",
        DataType::UInt32 => "uint32",
        DataType::UInt64 => "uint64",
        DataType::Float16 => "halffloat",
        DataType::Float32 => "float",
        DataType::Float64 => "double",
        DataType::Binary => "binary",
        DataType::LargeBinary => "large_binary",
        DataType::BinaryView => "binary_view",
        DataType::Utf8 => "string",
        DataType::LargeUtf8 => "large_string",
        DataType::Utf8View => "string_view",
        DataType::Date32 => "date32[day]",
        DataType::Date64 => "date64[ms]",
        DataType::Interval(IntervalUnit::YearMonth) => "month_interval",
        DataType::Interval(IntervalUnit::DayTime) => "day_time_interval",
        DataType::Interval(IntervalUnit::MonthDayNano) => "month_day_nano_interval",
        _ => "",
    };
    if !simple.is_empty() {
        return simple.into();
    }
    match kind {
        DataType::FixedSizeBinary(size) => format!("fixed_size_binary[{size}]"),
        DataType::Timestamp(time, zone) => match zone {
            Some(zone) if !zone.is_empty() => format!("timestamp[{}, tz={zone}]", unit(time)),
            _ => format!("timestamp[{}]", unit(time)),
        },
        DataType::Time32(time) => format!("time32[{}]", unit(time)),
        DataType::Time64(time) => format!("time64[{}]", unit(time)),
        DataType::Duration(time) => format!("duration[{}]", unit(time)),
        DataType::Decimal32(p, s) => format!("decimal32({p}, {s})"),
        DataType::Decimal64(p, s) => format!("decimal64({p}, {s})"),
        DataType::Decimal128(p, s) => format!("decimal128({p}, {s})"),
        DataType::Decimal256(p, s) => format!("decimal256({p}, {s})"),
        DataType::List(child) => format!("list<{}>", field_name(child)),
        DataType::LargeList(child) => format!("large_list<{}>", field_name(child)),
        DataType::ListView(child) => format!("list_view<{}>", field_name(child)),
        DataType::LargeListView(child) => format!("large_list_view<{}>", field_name(child)),
        DataType::FixedSizeList(child, size) => {
            format!("fixed_size_list<{}>[{size}]", field_name(child))
        }
        DataType::Struct(fields) => format!(
            "struct<{}>",
            fields
                .iter()
                .map(|f| field_name(f))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DataType::Dictionary(index, value) => format!(
            "dictionary<values={}, indices={}, ordered={}>",
            type_name(value, name, false),
            type_name(index, name, false),
            u8::from(ordered)
        ),
        DataType::Map(child, sorted) => {
            let DataType::Struct(fields) = child.data_type() else {
                return "map<?>".into();
            };
            if fields.len() != 2 {
                return "map<?>".into();
            }
            format!(
                "map<{}, {}{} ('{name}')>",
                field_type(&fields[0]),
                field_type(&fields[1]),
                if *sorted { ", keys_sorted" } else { "" }
            )
        }
        DataType::Union(fields, mode) => format!(
            "{}_union<{}>",
            if *mode == UnionMode::Dense {
                "dense"
            } else {
                "sparse"
            },
            fields
                .iter()
                .map(|(id, field)| format!("{}={id}", field_name(field)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DataType::RunEndEncoded(ends, values) => format!(
            "run_end_encoded<run_ends: {}, values: {}>",
            field_type(ends),
            field_type(values)
        ),
        // Every remaining variant was handled by the simple spelling above.
        _ => unreachable!("simple Arrow type"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn diagnostic_spelling_retains_nested_nullability_and_dictionary_order() {
        let dictionary = Field::new(
            "X",
            DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8)),
            true,
        )
        .with_dict_is_ordered(true);
        assert_eq!(
            field_type(&dictionary),
            "dictionary<values=string, indices=int32, ordered=1>"
        );
        let nested = Field::new(
            "X",
            DataType::Struct(
                vec![
                    Field::new("a", DataType::Boolean, false),
                    Field::new(
                        "b",
                        DataType::List(Arc::new(Field::new("element", DataType::Utf8, true))),
                        true,
                    ),
                ]
                .into(),
            ),
            true,
        );
        assert_eq!(
            field_type(&nested),
            "struct<a: bool not null, b: list<element: string>>"
        );
    }
}
