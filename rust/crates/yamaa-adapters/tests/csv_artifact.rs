use std::convert::Infallible;
use yamaa_adapters::csv_artifact::{self, Error};
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
fn text_table(names: &[&str], rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: TableSchema::new(
            names
                .iter()
                .map(|name| Column {
                    name: (*name).into(),
                    kind: ColumnType::Str,
                })
                .collect(),
        )
        .unwrap(),
        rows,
    }
}
#[test]
fn exact_quoting_preserves_empty_missing_whitespace_unicode_and_line_characters() {
    let table = text_table(
        &["A", "B", "C", "D"],
        vec![
            vec![
                Value::Missing,
                Value::Str("".into()),
                Value::Str("  text\t".into()),
                Value::Str("é漢".into()),
            ],
            vec![
                Value::Str("a,b".into()),
                Value::Str("a\"b".into()),
                Value::Str("a\nb".into()),
                Value::Str("a\rb".into()),
            ],
        ],
    );
    let expected = "A,B,C,D\n,\"\",  text\t,é漢\n\"a,b\",\"a\"\"b\",\"a\nb\",\"a\rb\"\n";
    assert_eq!(
        csv_artifact::render(&table, &[0, 1, 2, 3], expected.len()).unwrap(),
        expected.as_bytes()
    );
    assert!(matches!(
        csv_artifact::render(&table, &[0, 1, 2, 3], expected.len() - 1),
        Err(Error::Limit)
    ));
}
#[test]
fn header_only_and_projection_order_are_exact() {
    let table = text_table(&["FIRST", "LAST"], vec![]);
    assert_eq!(
        csv_artifact::render(&table, &[1, 0], 20).unwrap(),
        b"LAST,FIRST\n"
    );
    for columns in [vec![], vec![0, 0], vec![2]] {
        assert!(matches!(
            csv_artifact::render(&table, &columns, 100),
            Err(Error::Projection)
        ));
    }
}
#[test]
fn scalar_output_uses_exact_integer_and_shortest_positional_float_text() {
    let table = Table {
        schema: TableSchema::new(vec![
            Column {
                name: "I".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "F".into(),
                kind: ColumnType::Float,
            },
        ])
        .unwrap(),
        rows: vec![
            vec![Value::Int(i64::MAX), Value::float(-0.0)],
            vec![Value::Int(9007199254740993), Value::float(1e-7)],
        ],
    };
    assert_eq!(
        csv_artifact::render(&table, &[0, 1], 100).unwrap(),
        b"I,F\n9223372036854775807,-0\n9007199254740993,0.0000001\n"
    );
}

#[test]
fn precision_serializes_float_projection_only_and_preserves_missing_and_empty_text() {
    let table = Table {
        schema: TableSchema::new(vec![
            Column {
                name: "I".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "F".into(),
                kind: ColumnType::Float,
            },
            Column {
                name: "S".into(),
                kind: ColumnType::Str,
            },
        ])
        .unwrap(),
        rows: vec![
            vec![Value::Int(1), Value::float(0.125), Value::Str("".into())],
            vec![Value::Int(2), Value::float(2.675), Value::Str("a,b".into())],
            vec![Value::Int(3), Value::Missing, Value::Missing],
            vec![Value::Int(4), Value::float(-0.004), Value::Str("z".into())],
        ],
    };
    let expected = b"S,F,I\n\"\",0.13,1\n\"a,b\",2.67,2\n,,3\nz,0.00,4\n";
    assert_eq!(
        csv_artifact::render_with_decimals(&table, &[2, 1, 0], Some("2"), expected.len()).unwrap(),
        expected
    );
    assert!(matches!(
        csv_artifact::render_with_decimals(&table, &[2, 1, 0], Some("2"), expected.len() - 1),
        Err(Error::Limit)
    ));
    assert_eq!(table.rows[0][1], Value::float(0.125));
    assert_eq!(
        csv_artifact::render_with_decimals(
            &table,
            &[0],
            Some("999999999999999999999999999999"),
            10
        )
        .unwrap(),
        b"I\n1\n2\n3\n4\n"
    );
}

#[test]
fn excessive_precision_refuses_before_the_first_projected_cell_read() {
    struct NoCells(TableSchema);
    impl TableAccess for NoCells {
        type Error = Infallible;
        fn schema(&self) -> &TableSchema {
            &self.0
        }
        fn row_count(&self) -> usize {
            1
        }
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
            panic!("precision quota must precede cell authority")
        }
    }
    let table = NoCells(
        TableSchema::new(vec![
            Column {
                name: "I".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "F".into(),
                kind: ColumnType::Float,
            },
        ])
        .unwrap(),
    );
    for precision in ["10000", "999999999999999999999999999999"] {
        assert!(matches!(
            csv_artifact::render_with_decimals(&table, &[0, 1], Some(precision), 100),
            Err(Error::Limit)
        ));
    }
    struct FirstCellOnly(TableSchema);
    impl TableAccess for FirstCellOnly {
        type Error = Infallible;
        fn schema(&self) -> &TableSchema {
            &self.0
        }
        fn row_count(&self) -> usize {
            2
        }
        fn cell(&self, row: usize, _: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
            assert_eq!(row, 0, "remaining width quota precedes every later cell");
            Ok(ValueRef::Float(
                yamaa_core::value::FiniteFloat::new(1.0).unwrap(),
            ))
        }
    }
    let table = FirstCellOnly(
        TableSchema::new(vec![Column {
            name: "F".into(),
            kind: ColumnType::Float,
        }])
        .unwrap(),
    );
    // F\n1.00\n exhausts seven bytes; the second record's cell is never read.
    assert!(matches!(
        csv_artifact::render_with_decimals(&table, &[0], Some("2"), 7),
        Err(Error::Limit)
    ));
}
