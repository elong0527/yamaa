use parquet::{
    basic::{Compression, LogicalType, Repetition, TimeUnit, Type},
    file::{reader::FileReader, serialized_reader::SerializedFileReader},
    record::{Field, RowAccessor},
};
use std::{
    convert::Infallible,
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::parquet_artifact::{render, Error, Limits};
use yamaa_core::{
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};

struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Table {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.rows
            .get(row)
            .and_then(|r| r.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
fn limits() -> Limits {
    Limits {
        output_bytes: 1_048_576,
        staged_bytes: 131_072,
        cells: 1_048_576,
        columns: 64,
    }
}
fn table(columns: &[(&str, ColumnType)], rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: TableSchema::new(
            columns
                .iter()
                .map(|&(name, kind)| Column {
                    name: name.into(),
                    kind,
                })
                .collect(),
        )
        .unwrap(),
        rows,
    }
}
struct Source(PathBuf);
impl Source {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "yamaa-parquet-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        Self(path)
    }
    fn reader(&self) -> SerializedFileReader<File> {
        SerializedFileReader::new(File::open(&self.0).unwrap()).unwrap()
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn closed_schema_and_exact_values_include_full_integers_temporals_nulls_and_zero_sign() {
    use ColumnType::*;
    let table = table(
        &[
            ("S", Str),
            ("I", Int),
            ("F", Float),
            ("D", Date),
            ("T", DateTime),
        ],
        vec![
            vec![
                Value::Str("".into()),
                Value::Int(i64::MIN),
                Value::float(-0.0),
                Value::Date("0001-01-01".parse().unwrap()),
                Value::DateTime("1969-12-31T23:59:59".parse().unwrap()),
            ],
            vec![
                Value::Missing,
                Value::Int(i64::MAX),
                Value::float(f64::from_bits(1)),
                Value::Date("9999-12-31".parse().unwrap()),
                Value::DateTime("9999-12-31T23:59:59".parse().unwrap()),
            ],
            vec![
                Value::Str("a\u{e9}\n".into()),
                Value::Int(9_007_199_254_740_993),
                Value::float(1.2345678901234567),
                Value::Missing,
                Value::Missing,
            ],
        ],
    );
    let bytes = render(&table, &[0, 1, 2, 3, 4], limits()).unwrap();
    let source = Source::new(&bytes);
    let reader = source.reader();
    let meta = reader.metadata().file_metadata();
    assert!(meta.key_value_metadata().is_none());
    let schema = meta.schema_descr();
    let expected = [
        (Type::BYTE_ARRAY, Some(LogicalType::String)),
        (Type::INT64, None),
        (Type::DOUBLE, None),
        (Type::INT32, Some(LogicalType::Date)),
        (
            Type::INT64,
            Some(LogicalType::timestamp(false, TimeUnit::MICROS)),
        ),
    ];
    for (i, (physical, logical)) in expected.into_iter().enumerate() {
        let field = schema.column(i);
        assert_eq!(field.physical_type(), physical);
        assert_eq!(field.logical_type_ref(), logical.as_ref());
        assert_eq!(
            field.self_type().get_basic_info().repetition(),
            Repetition::OPTIONAL
        );
        assert_eq!(
            reader.metadata().row_group(0).column(i).compression(),
            Compression::UNCOMPRESSED
        );
    }
    let rows = reader
        .get_row_iter(None)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(rows[0].get_string(0).unwrap(), "");
    assert!(rows[1].is_null(0).unwrap());
    assert_eq!(rows[2].get_string(0).unwrap(), "a\u{e9}\n");
    assert_eq!(rows[0].get_long(1).unwrap(), i64::MIN);
    assert_eq!(rows[1].get_long(1).unwrap(), i64::MAX);
    assert_eq!(rows[2].get_long(1).unwrap(), 9_007_199_254_740_993);
    assert_eq!(
        rows[0].get_double(2).unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(rows[1].get_double(2).unwrap().to_bits(), 1);
    assert_eq!(
        rows[2].get_double(2).unwrap().to_bits(),
        1.2345678901234567f64.to_bits()
    );
    assert_eq!(
        rows[0].get_column_iter().nth(3).unwrap().1,
        &Field::Date(-719_162)
    );
    assert_eq!(
        rows[1].get_column_iter().nth(3).unwrap().1,
        &Field::Date(2_932_896)
    );
    assert_eq!(rows[0].get_timestamp_micros(4).unwrap(), -1_000_000);
    assert_eq!(
        rows[1].get_timestamp_micros(4).unwrap(),
        253_402_300_799_000_000
    );
    assert!(rows[2].is_null(3).unwrap() && rows[2].is_null(4).unwrap());
    let mut budget = limits();
    budget.output_bytes = bytes.len();
    assert_eq!(
        render(&table, &[0, 1, 2, 3, 4], budget).unwrap().len(),
        bytes.len()
    );
    budget.output_bytes = bytes.len() - 1;
    assert!(matches!(
        render(&table, &[0, 1, 2, 3, 4], budget),
        Err(Error::Limit)
    ));
}

#[test]
fn opaque_cell_failures_retain_the_original_payload() {
    struct Failed {
        schema: TableSchema,
        token: std::sync::Arc<()>,
    }
    impl TableAccess for Failed {
        type Error = std::sync::Arc<()>;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            1
        }
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
            Err(CellError::Access(self.token.clone()))
        }
    }
    let failed = Failed {
        schema: table(&[("S", ColumnType::Str)], vec![]).schema,
        token: std::sync::Arc::new(()),
    };
    let Err(Error::Cell(CellError::Access(original))) = render(&failed, &[0], limits()) else {
        panic!("opaque cell error must survive encoding");
    };
    assert!(std::sync::Arc::ptr_eq(&original, &failed.token));
}

#[test]
fn logical_records_preserve_json_spelling_without_rounding_or_partial_dates() {
    let table = table(
        &[
            ("S", ColumnType::Str),
            ("F", ColumnType::Float),
            ("I", ColumnType::Int),
        ],
        vec![
            vec![
                Value::Str("\u{1f600}\u{7f}\n\"".into()),
                Value::float(-0.0),
                Value::Int(i64::MIN),
            ],
            vec![
                Value::Missing,
                Value::float(1e-7),
                Value::Int(9_007_199_254_740_993),
            ],
        ],
    );
    let rows = yamaa_adapters::parquet_artifact::records(&table, &[0, 1, 2], 1000).unwrap();
    assert_eq!(
        rows,
        [
            r#"["S", "F", "I"]"#,
            r#"["\ud83d\ude00\u007f\n\"", -0.0, -9223372036854775808]"#,
            r#"[null, 1e-07, 9007199254740993]"#,
        ]
    );
    let size = rows.iter().map(String::len).sum();
    assert!(yamaa_adapters::parquet_artifact::records(&table, &[0, 1, 2], size).is_ok());
    assert!(matches!(
        yamaa_adapters::parquet_artifact::records(&table, &[0, 1, 2], size - 1),
        Err(Error::Limit)
    ));
}

#[test]
fn ordered_row_groups_and_projection_survive_an_empty_source() {
    let table = table(
        &[("A", ColumnType::Int), ("B", ColumnType::Int)],
        (0..2051)
            .map(|i| vec![Value::Int(i), Value::Int(-i)])
            .collect(),
    );
    let source = Source::new(&render(&table, &[1, 0], limits()).unwrap());
    let reader = source.reader();
    assert_eq!(
        reader
            .metadata()
            .row_groups()
            .iter()
            .map(|g| g.num_rows())
            .collect::<Vec<_>>(),
        [1024, 1024, 3]
    );
    assert_eq!(
        reader
            .metadata()
            .file_metadata()
            .schema_descr()
            .column(0)
            .name(),
        "B"
    );
    for (i, row) in reader.get_row_iter(None).unwrap().enumerate() {
        let row = row.unwrap();
        assert_eq!(row.get_long(0).unwrap(), -(i as i64));
        assert_eq!(row.get_long(1).unwrap(), i as i64);
    }
    let empty = Table {
        schema: table.schema,
        rows: vec![],
    };
    let source = Source::new(&render(&empty, &[1], limits()).unwrap());
    let reader = source.reader();
    assert_eq!(reader.metadata().file_metadata().num_rows(), 0);
    assert_eq!(
        reader
            .metadata()
            .file_metadata()
            .schema_descr()
            .column(0)
            .name(),
        "B"
    );
}

#[test]
fn invalid_projection_shape_staging_and_cell_types_produce_no_artifact() {
    let table = table(
        &[("S", ColumnType::Str)],
        vec![vec![Value::Str("x".repeat(4096))]],
    );
    for projection in [&[][..], &[0, 0], &[1]] {
        assert!(matches!(
            render(&table, projection, limits()),
            Err(Error::Projection)
        ));
    }
    for budget in [
        Limits {
            columns: 0,
            ..limits()
        },
        Limits {
            cells: 0,
            ..limits()
        },
        Limits {
            staged_bytes: 1024,
            ..limits()
        },
        Limits {
            output_bytes: 0,
            ..limits()
        },
    ] {
        assert!(matches!(render(&table, &[0], budget), Err(Error::Limit)));
    }
    let wrong = Table {
        schema: table.schema,
        rows: vec![vec![Value::Int(1)]],
    };
    assert!(matches!(
        render(&wrong, &[0], limits()),
        Err(Error::ValueType)
    ));
}
