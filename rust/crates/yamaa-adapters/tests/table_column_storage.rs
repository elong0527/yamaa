//! Table contracts: the engine's own table lends each column's typed storage
//! whole (#4190), and a range of its records copies each column in bulk.
//! A layout, never a value: every row read through the lent column, and
//! every record of a copied range, is the cell the table's port reads.
//!
//! The seam is `TableAccess::column` and `ColumnTable::rows`, which the
//! benchmark reader and the host fixtures observe only through a run's
//! output, never one cell against another.
mod table {
    mod column_storage {
        use yamaa_adapters::column_table::{Column, ColumnTable};
        use yamaa_adapters::columns::{Text, Values};
        use yamaa_core::table::{Column as Field, TableAccess, TableSchema, ValueRef};
        use yamaa_core::temporal::{Date, DateTime, Precision};
        use yamaa_core::value::{ColumnType, FiniteFloat};

        /// Rows enough to cross two bitmap words, so a range starts and
        /// ends inside a word.
        const ROWS: usize = 150;

        fn date(row: usize) -> Date {
            Date::new(
                2024,
                1 + (row % 12) as u8,
                1 + (row % 28) as u8,
                Precision::Day,
            )
            .unwrap()
        }

        fn datetime(row: usize) -> DateTime {
            DateTime::new(date(row), (row % 24) as u8, 0, 0, Precision::Second).unwrap()
        }

        /// The authored cell of `column` at `row`: every seventh row of the
        /// text column, every fifth of the others, missing; column 4's
        /// floats carry a signed zero.
        fn expected(column: usize, row: usize) -> ValueRef<'static> {
            const TEXT: [&str; 4] = ["", "é", "LBTEST", "mmol/L"];
            let missing = if column == 0 {
                row % 7 == 3
            } else {
                row % 5 == 1
            };
            if missing {
                return ValueRef::Missing;
            }
            match column {
                0 => ValueRef::Str(TEXT[row % 4]),
                1 => ValueRef::Int(row as i64 - 70),
                2 => ValueRef::Float(
                    FiniteFloat::new(if row == 0 { -0.0 } else { row as f64 / 8.0 }).unwrap(),
                ),
                3 => ValueRef::Date(date(row)),
                _ => ValueRef::DateTime(datetime(row)),
            }
        }

        fn table() -> ColumnTable {
            let fields = [
                ("TEXT", ColumnType::Str),
                ("INT", ColumnType::Int),
                ("FLOAT", ColumnType::Float),
                ("DATE", ColumnType::Date),
                ("DATETIME", ColumnType::DateTime),
            ];
            let schema = TableSchema::new(
                fields
                    .iter()
                    .map(|&(name, kind)| Field {
                        name: name.into(),
                        kind,
                    })
                    .collect(),
            )
            .unwrap();
            let cells = |column| (0..ROWS).map(move |row| expected(column, row));
            let columns = vec![
                Column::Str(
                    cells(0)
                        .map(|value| match value {
                            ValueRef::Str(text) => Some(text),
                            _ => None,
                        })
                        .collect::<Text>(),
                ),
                Column::Int(
                    cells(1)
                        .map(|value| match value {
                            ValueRef::Int(value) => Some(value),
                            _ => None,
                        })
                        .collect::<Values<i64>>(),
                ),
                Column::floats(cells(2).map(|value| match value {
                    ValueRef::Float(value) => Some(value.get()),
                    _ => None,
                })),
                Column::Date(
                    cells(3)
                        .map(|value| match value {
                            ValueRef::Date(value) => Some(value),
                            _ => None,
                        })
                        .collect(),
                ),
                Column::DateTime(
                    cells(4)
                        .map(|value| match value {
                            ValueRef::DateTime(value) => Some(value),
                            _ => None,
                        })
                        .collect(),
                ),
            ];
            ColumnTable::new(schema, columns, ROWS).unwrap()
        }

        /// A float compared by its bits, so a signed zero is its own value.
        fn exact(value: ValueRef<'_>) -> String {
            match value {
                ValueRef::Float(value) => format!("Float({:#x})", value.get().to_bits()),
                value => format!("{value:?}"),
            }
        }

        #[test]
        fn a_lent_column_reads_every_row_as_the_authored_cell_and_refuses_no_column() {
            let table = table();
            for column in 0..5 {
                let lent =
                    TableAccess::column(&table, column).expect("a column table lends its columns");
                let read: Vec<String> = (0..lent.len()).map(|row| exact(lent.get(row))).collect();
                let authored: Vec<String> =
                    (0..ROWS).map(|row| exact(expected(column, row))).collect();
                assert_eq!(
                    read, authored,
                    "column {column} read through its lent storage"
                );
                let sliced = lent.slice(3..130).expect("a range inside the column");
                let read: Vec<String> = (0..sliced.len())
                    .map(|row| exact(sliced.get(row)))
                    .collect();
                assert_eq!(
                    read,
                    authored[3..130],
                    "column {column} rows 3..130 of its lent storage"
                );
            }
            assert!(
                TableAccess::column(&table, 5).is_none(),
                "a column past the schema is not lent"
            );
            assert!(
                TableAccess::column(&table, 0)
                    .unwrap()
                    .slice(100..151)
                    .is_none(),
                "a range past the column's rows is not lent"
            );
        }

        #[test]
        fn a_range_of_records_keeps_every_value_and_missing_slot_of_each_column() {
            let table = table();
            for range in [0..ROWS, 3..130, 64..128, 70..70] {
                let copied = table.rows(range.clone()).unwrap();
                for column in 0..5 {
                    let read: Vec<String> = (0..copied.row_count())
                        .map(|row| exact(copied.cell(row, column).unwrap()))
                        .collect();
                    let authored: Vec<String> = range
                        .clone()
                        .map(|row| exact(expected(column, row)))
                        .collect();
                    assert_eq!(read, authored, "column {column} of records {range:?}");
                }
            }
        }
    }
}
