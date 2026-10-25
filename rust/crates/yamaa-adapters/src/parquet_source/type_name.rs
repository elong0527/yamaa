//! Arrow C++ diagnostic spelling retained by the source condition contract.
use crate::storage_api::LaterType;
use arrow_schema::{DataType, Field, IntervalUnit, TimeUnit, UnionMode};

/// Spell `field`'s type as the file names it: each type in `later`, which
/// the file's embedded schema names at that path below `field` (the index of
/// each child, as `storage_api::LaterTypes` counts them), is spelled as named
/// where Arrow 55 holds its nearest type, as a later Arrow reader spells it.
pub(super) fn stored_type(field: &Field, later: &[(&[usize], LaterType)]) -> String {
    Spelling {
        later,
        path: Vec::new(),
    }
    .field_type(field)
}

struct Spelling<'a> {
    later: &'a [(&'a [usize], LaterType)],
    /// Where the spelling is, below the field it began with.
    path: Vec<usize>,
}

impl Spelling<'_> {
    fn field_type(&mut self, field: &Field) -> String {
        self.type_name(
            field.data_type(),
            field.name(),
            field.dict_is_ordered().unwrap_or(false),
        )
    }

    fn field_name(&mut self, field: &Field) -> String {
        format!(
            "{}: {}{}",
            field.name(),
            self.field_type(field),
            if field.is_nullable() { "" } else { " not null" }
        )
    }

    /// Spell the child `index` of the type being spelled with `spell`.
    fn child(&mut self, index: usize, spell: impl FnOnce(&mut Self) -> String) -> String {
        self.path.push(index);
        let spelled = spell(self);
        self.path.pop();
        spelled
    }

    /// The later type the file names here, if any.
    fn later(&self) -> Option<LaterType> {
        self.later
            .iter()
            .find(|(path, _)| *path == self.path.as_slice())
            .map(|(_, later)| *later)
    }

    fn type_name(&mut self, kind: &DataType, name: &str, ordered: bool) -> String {
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
        match (kind, self.later()) {
            (
                DataType::Decimal128(..),
                Some(LaterType::Decimal {
                    bits,
                    precision,
                    scale,
                }),
            ) => {
                return format!("decimal{bits}({precision}, {scale})");
            }
            (DataType::List(child), Some(LaterType::ListView)) => {
                return format!("list_view<{}>", self.child(0, |s| s.field_name(child)));
            }
            (DataType::LargeList(child), Some(LaterType::LargeListView)) => {
                return format!(
                    "large_list_view<{}>",
                    self.child(0, |s| s.field_name(child))
                );
            }
            _ => {}
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
            DataType::Decimal128(p, s) => format!("decimal128({p}, {s})"),
            DataType::Decimal256(p, s) => format!("decimal256({p}, {s})"),
            DataType::List(child) => format!("list<{}>", self.child(0, |s| s.field_name(child))),
            DataType::LargeList(child) => {
                format!("large_list<{}>", self.child(0, |s| s.field_name(child)))
            }
            DataType::ListView(child) => {
                format!("list_view<{}>", self.child(0, |s| s.field_name(child)))
            }
            DataType::LargeListView(child) => {
                format!(
                    "large_list_view<{}>",
                    self.child(0, |s| s.field_name(child))
                )
            }
            DataType::FixedSizeList(child, size) => format!(
                "fixed_size_list<{}>[{size}]",
                self.child(0, |s| s.field_name(child))
            ),
            DataType::Struct(fields) => format!(
                "struct<{}>",
                fields
                    .iter()
                    .enumerate()
                    .map(|(index, f)| self.child(index, |s| s.field_name(f)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            DataType::Dictionary(index, value) => format!(
                "dictionary<values={}, indices={}, ordered={}>",
                self.type_name(value, name, false),
                self.type_name(index, name, false),
                u8::from(ordered)
            ),
            DataType::Map(child, sorted) => {
                let DataType::Struct(fields) = child.data_type() else {
                    return "map<?>".into();
                };
                if fields.len() != 2 {
                    return "map<?>".into();
                }
                let key = self.child(0, |s| s.child(0, |s| s.field_type(&fields[0])));
                let value = self.child(0, |s| s.child(1, |s| s.field_type(&fields[1])));
                format!(
                    "map<{key}, {value}{} ('{name}')>",
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
                    .enumerate()
                    .map(|(index, (id, field))| {
                        format!("{}={id}", self.child(index, |s| s.field_name(field)))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            DataType::RunEndEncoded(ends, values) => format!(
                "run_end_encoded<run_ends: {}, values: {}>",
                self.child(0, |s| s.field_type(ends)),
                self.child(1, |s| s.field_type(values))
            ),
            // Every remaining variant was handled by the simple spelling above.
            _ => unreachable!("simple Arrow type"),
        }
    }
}

pub(super) fn unit(unit: &TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Second => "s",
        TimeUnit::Millisecond => "ms",
        TimeUnit::Microsecond => "us",
        TimeUnit::Nanosecond => "ns",
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
            stored_type(&dictionary, &[]),
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
            stored_type(&nested, &[]),
            "struct<a: bool not null, b: list<element: string>>"
        );
    }

    /// A later type the file names is spelled as named where Arrow 55 holds
    /// its nearest type, at the path where the file names it, below lists,
    /// structs and maps, as pyarrow 26 spells the same fields.
    #[test]
    fn later_types_are_spelled_as_the_file_names_them() {
        let decimal32 = LaterType::Decimal {
            bits: 32,
            precision: 9,
            scale: 2,
        };
        let decimal64 = LaterType::Decimal {
            bits: 64,
            precision: 18,
            scale: 2,
        };
        let held = |name, kind| Arc::new(Field::new(name, kind, true));
        let list = |kind| DataType::List(held("element", kind));
        let entries = DataType::Struct(
            vec![
                Field::new("key", DataType::Utf8, false),
                Field::new("value", DataType::Decimal128(18, 2), true),
            ]
            .into(),
        );
        type Case = (DataType, Vec<(&'static [usize], LaterType)>, &'static str);
        let cases: [Case; 7] = [
            (
                DataType::Decimal128(9, 2),
                vec![(&[], decimal32)],
                "decimal32(9, 2)",
            ),
            (
                DataType::Decimal128(18, 2),
                vec![(&[], decimal64)],
                "decimal64(18, 2)",
            ),
            (
                list(DataType::Int64),
                vec![(&[], LaterType::ListView)],
                "list_view<element: int64>",
            ),
            (
                DataType::LargeList(held("element", DataType::Int64)),
                vec![(&[], LaterType::LargeListView)],
                "large_list_view<element: int64>",
            ),
            (
                list(DataType::Decimal128(9, 2)),
                vec![(&[], LaterType::ListView), (&[0], decimal32)],
                "list_view<element: decimal32(9, 2)>",
            ),
            (
                DataType::Struct(vec![Field::new("l", list(DataType::Int64), true)].into()),
                vec![(&[0], LaterType::ListView)],
                "struct<l: list_view<element: int64>>",
            ),
            (
                DataType::Map(Arc::new(Field::new("entries", entries, false)), false),
                vec![(&[0, 1], decimal64)],
                "map<string, decimal64(18, 2) ('X')>",
            ),
        ];
        for (kind, later, expected) in cases {
            let field = Field::new("X", kind, true);
            assert_eq!(stored_type(&field, &later), expected);
        }
        // Where Arrow 55 holds another type, or at another path, the later
        // type is not spelled: the native conversion did not apply it.
        let int = Field::new("X", DataType::Int32, true);
        assert_eq!(stored_type(&int, &[(&[], decimal32)]), "int32");
        let large = Field::new(
            "X",
            DataType::LargeList(held("item", DataType::Int64)),
            true,
        );
        let view = [(&[][..], LaterType::ListView)];
        assert_eq!(stored_type(&large, &view), "large_list<item: int64>");
        let nested = Field::new("X", list(DataType::Decimal128(9, 2)), true);
        assert_eq!(
            stored_type(&nested, &[(&[], decimal32)]),
            "list<element: decimal128(9, 2)>"
        );
    }
}
