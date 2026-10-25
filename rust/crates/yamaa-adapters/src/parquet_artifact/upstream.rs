//! The 1.0 writer, parquet 55.2.0's `SerializedFileWriter` under D160's
//! settings, as the artifact encoder called it until #3878: the oracle of the
//! native writer's tests while the parquet crate is in the lock. It takes
//! only valid tables, so any failure panics.
use crate::epoch::{date_days, datetime_seconds};
use parquet::{
    basic::{Compression, LogicalType, Repetition, TimeUnit, Type as Physical},
    data_type::{ByteArray, ByteArrayType, DataType, DoubleType, Int32Type, Int64Type},
    file::{
        properties::{EnabledStatistics, WriterProperties},
        writer::{SerializedColumnWriter, SerializedFileWriter},
    },
    schema::types::Type,
};
use std::{ops::Range, sync::Arc};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

const ROW_GROUP_ROWS: usize = 1024;

fn field(name: &str, kind: ColumnType) -> Arc<Type> {
    let (physical, logical) = match kind {
        ColumnType::Str => (Physical::BYTE_ARRAY, Some(LogicalType::String)),
        ColumnType::Int => (Physical::INT64, None),
        ColumnType::Float => (Physical::DOUBLE, None),
        ColumnType::Date => (Physical::INT32, Some(LogicalType::Date)),
        ColumnType::DateTime => (
            Physical::INT64,
            Some(LogicalType::Timestamp {
                is_adjusted_to_u_t_c: false,
                unit: TimeUnit::MICROS(Default::default()),
            }),
        ),
    };
    Arc::new(
        Type::primitive_type_builder(name, physical)
            .with_repetition(Repetition::OPTIONAL)
            .with_logical_type(logical)
            .build()
            .unwrap(),
    )
}

/// One column's levels and values for the row group being filled.
enum Values {
    Text(Vec<ByteArray>),
    Long(Vec<i64>),
    Double(Vec<f64>),
    Days(Vec<i32>),
}

fn push(levels: &mut Vec<i16>, values: &mut Values, value: ValueRef<'_>) {
    match (values, value) {
        (_, ValueRef::Missing) => return levels.push(0),
        (Values::Text(values), ValueRef::Str(value)) => values.push(ByteArray::from(value)),
        (Values::Long(values), ValueRef::Int(value)) => values.push(value),
        (Values::Double(values), ValueRef::Float(value)) => values.push(value.get()),
        (Values::Days(values), ValueRef::Date(value)) => values.push(date_days(value)),
        (Values::Long(values), ValueRef::DateTime(value)) => {
            values.push(datetime_seconds(value) * 1_000_000)
        }
        _ => panic!("a value of another type"),
    }
    levels.push(1);
}

fn write(writer: &mut SerializedColumnWriter<'_>, levels: &[i16], values: &Values) {
    fn batch<D: DataType>(
        writer: &mut SerializedColumnWriter<'_>,
        values: &[D::T],
        levels: &[i16],
    ) {
        writer
            .typed::<D>()
            .write_batch(values, Some(levels), None)
            .unwrap();
    }
    match values {
        Values::Text(values) => batch::<ByteArrayType>(writer, values, levels),
        Values::Long(values) => batch::<Int64Type>(writer, values, levels),
        Values::Double(values) => batch::<DoubleType>(writer, values, levels),
        Values::Days(values) => batch::<Int32Type>(writer, values, levels),
    }
}

/// The bytes the 1.0 writer saved from `partitions` of `table`.
pub(super) fn render_partitions<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partitions: &[Range<usize>],
) -> Vec<u8> {
    let columns = table.schema().columns();
    let fields = projection
        .iter()
        .map(|&index| field(&columns[index].name, columns[index].kind))
        .collect();
    let schema = Arc::new(
        Type::group_type_builder("schema")
            .with_fields(fields)
            .build()
            .unwrap(),
    );
    let properties = Arc::new(
        WriterProperties::builder()
            .set_compression(Compression::UNCOMPRESSED)
            .set_dictionary_enabled(false)
            .set_statistics_enabled(EnabledStatistics::None)
            .set_offset_index_disabled(true)
            .set_key_value_metadata(None)
            .set_write_batch_size(ROW_GROUP_ROWS)
            .build(),
    );
    let mut file = SerializedFileWriter::new(Vec::new(), schema, properties).unwrap();
    let empty = |&index: &usize| match columns[index].kind {
        ColumnType::Str => Values::Text(Vec::new()),
        ColumnType::Int | ColumnType::DateTime => Values::Long(Vec::new()),
        ColumnType::Float => Values::Double(Vec::new()),
        ColumnType::Date => Values::Days(Vec::new()),
    };
    let mut pending: Vec<(Vec<i16>, Values)> = projection
        .iter()
        .map(|index| (Vec::new(), empty(index)))
        .collect();
    let flush = |file: &mut SerializedFileWriter<Vec<u8>>,
                 pending: &mut Vec<(Vec<i16>, Values)>| {
        let mut group = file.next_row_group().unwrap();
        for (levels, values) in pending.iter() {
            let mut column = group.next_column().unwrap().unwrap();
            write(&mut column, levels, values);
            column.close().unwrap();
        }
        group.close().unwrap();
        *pending = projection
            .iter()
            .map(|index| (Vec::new(), empty(index)))
            .collect();
    };
    let mut held = 0;
    for row in partitions.iter().flat_map(Clone::clone) {
        for ((levels, values), &index) in pending.iter_mut().zip(projection) {
            let Ok(cell) = table.cell(row, index) else {
                panic!("row {row} of column {index} is out of bounds")
            };
            push(levels, values, cell);
        }
        held += 1;
        if held == ROW_GROUP_ROWS {
            flush(&mut file, &mut pending);
            held = 0;
        }
    }
    if held > 0 {
        flush(&mut file, &mut pending);
    }
    file.into_inner().unwrap()
}

/// The native writer against the 1.0 writer: every table below saves the same
/// bytes through both, whole and a partition at a time. The tables cross the
/// definition-level encoder's choices: row counts on each side of a group of
/// eight, a bit-packed run of 63 groups and a row group of 1,024, and columns
/// missing never, always, periodically, in runs that grow past eight and at
/// random.
#[test]
fn every_null_pattern_saves_the_1_0_writers_bytes() {
    use crate::epoch::{date_from_days, datetime_from_seconds};
    use yamaa_core::{
        table::{CellError, Column, TableSchema},
        temporal::Precision,
        value::Value,
    };

    struct Table {
        schema: TableSchema,
        rows: Vec<Vec<Value>>,
    }
    impl TableAccess for Table {
        type Error = std::convert::Infallible;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            self.rows.len()
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
            Ok(ValueRef::from(&self.rows[row][column]))
        }
    }

    const KINDS: [ColumnType; 5] = [
        ColumnType::Str,
        ColumnType::Int,
        ColumnType::Float,
        ColumnType::Date,
        ColumnType::DateTime,
    ];
    // Whether row `row` is missing under pattern `pattern`.
    let missing = |pattern: usize, row: usize, state: &mut u64| -> bool {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        match pattern {
            0 => false,
            1 => true,
            2..=21 => row % pattern == 1,
            // Runs of 1 to 17 missing rows between runs of 1 to 12 present.
            22 => {
                let (mut at, mut length) = (0, 1);
                loop {
                    if row < at + length {
                        return true;
                    }
                    at += length;
                    let present = length % 12 + 1;
                    if row < at + present {
                        return false;
                    }
                    at += present;
                    length = length % 17 + 1;
                }
            }
            23 => row % 16 < 8,
            24 => row % 18 < 9,
            25 => row < 600,
            26 => *state % 10 == 0,
            27 => *state % 2 == 0,
            _ => *state % 10 != 0,
        }
    };
    let value = |kind: ColumnType, row: usize| {
        let n = row as i64 - 700;
        match kind {
            ColumnType::Str => Value::Str("x".repeat(row % 5)),
            ColumnType::Int => Value::Int(n * 1_000_003),
            ColumnType::Float => Value::float(n as f64 * 0.25),
            ColumnType::Date => Value::Date(date_from_days(n * 31, Precision::Day).unwrap()),
            ColumnType::DateTime => {
                Value::DateTime(datetime_from_seconds(n * 4_093, Precision::Second).unwrap())
            }
        }
    };
    let mut compared = 0;
    for rows in [
        0, 1, 7, 8, 9, 15, 17, 503, 504, 505, 512, 1_023, 1_024, 1_025, 2_051,
    ] {
        for patterns in (0..29).collect::<Vec<_>>().chunks(5) {
            let mut state = 0x9e37_79b9_7f4a_7c15 ^ rows as u64;
            let kinds: Vec<ColumnType> = (0..patterns.len()).map(|i| KINDS[i]).collect();
            let table = Table {
                schema: TableSchema::new(
                    kinds
                        .iter()
                        .enumerate()
                        .map(|(i, &kind)| Column {
                            name: format!("c{i}"),
                            kind,
                        })
                        .collect(),
                )
                .unwrap(),
                rows: (0..rows)
                    .map(|row| {
                        patterns
                            .iter()
                            .zip(&kinds)
                            .map(|(&pattern, &kind)| {
                                if missing(pattern, row, &mut state) {
                                    Value::Missing
                                } else {
                                    value(kind, row)
                                }
                            })
                            .collect()
                    })
                    .collect(),
            };
            let projection: Vec<usize> = (0..kinds.len()).collect();
            let whole = 0..rows;
            let expected = render_partitions(&table, &projection, std::slice::from_ref(&whole));
            for size in [rows.max(1), 7, 1_000] {
                let partitions: Vec<Range<usize>> = (0..rows.max(1))
                    .step_by(size)
                    .map(|start| start..rows.min(start + size))
                    .collect();
                let (saved, ends) =
                    super::render_partitions(&table, &projection, &partitions).unwrap();
                assert!(
                    saved == expected,
                    "{rows} rows, patterns {patterns:?}, partitions of {size}"
                );
                assert_eq!(ends.last(), Some(&saved.len()), "{rows} rows, {size}");
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 15 * 6 * 3);
}
