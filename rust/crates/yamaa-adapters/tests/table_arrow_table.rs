//! Table contracts: column table, epoch conversions, CSV artifact, CSV
//! precision, CSV source, parquet artifact, predicate diagnostics and public
//! table.
//!
//! Each module names the seam its tests prove. What a save with output
//! decimals writes or refuses is a case of
//! `fixtures/diagnostics/csv_precision.json` (#3798).
mod table {
    mod column_table {
        // Seam, for every test here: the in-memory column table, its storage,
        // projection, appended columns, validity and limits, which no case builds.
        use crate::support::table_schema as logical_schema;
        use yamaa_adapters::{
            column_table::{Column, ColumnTable, TableError},
            columns::Values,
        };
        use yamaa_core::{
            limits::Limits,
            table::{CellError, TableAccess, TableSchema, ValueRef},
            temporal::{Date, DateTime, Precision as P},
            value::ColumnType as CT,
        };

        /// Read and require an actual date, with its collected precision.
        fn date(table: &ColumnTable, row: usize, column: usize) -> Date {
            let ValueRef::Date(value) = table.cell(row, column).unwrap() else {
                panic!("date required")
            };
            value
        }

        /// Read and require an actual datetime, with its collected precision.
        fn datetime(table: &ColumnTable, row: usize, column: usize) -> DateTime {
            let ValueRef::DateTime(value) = table.cell(row, column).unwrap() else {
                panic!("datetime required")
            };
            value
        }

        #[test]
        /// Retain full-range integers, exact text and finite bits; admission
        /// makes a nonfinite float missing.
        fn exact_scalars_are_kept_and_nonfinite_floats_are_missing() {
            let schema = logical_schema(&[
                ("text", CT::Str),
                ("integer", CT::Int),
                ("float", CT::Float),
            ]);
            let floats = [-0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN];
            let table = ColumnTable::new(
                schema,
                vec![
                    Column::Str(
                        [Some(""), Some("a\0\u{1f980}"), None, Some("last")]
                            .into_iter()
                            .collect(),
                    ),
                    Column::Int(
                        [i64::MIN, i64::MAX, 9_007_199_254_740_993, 0]
                            .map(Some)
                            .into_iter()
                            .collect(),
                    ),
                    Column::Float(floats.map(Some).into_iter().collect::<Values<f64>>()),
                ],
                4,
            )
            .unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""));
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("a\0\u{1f980}"));
            assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(3, 0).unwrap(), ValueRef::Str("last"));
            for (row, value) in [i64::MIN, i64::MAX, 9_007_199_254_740_993, 0]
                .into_iter()
                .enumerate()
            {
                assert_eq!(table.cell(row, 1).unwrap(), ValueRef::Int(value));
            }
            let ValueRef::Float(zero) = table.cell(0, 2).unwrap() else {
                panic!("float required")
            };
            assert_eq!(zero.get().to_bits(), 1 << 63);
            for row in 1..4 {
                assert_eq!(table.cell(row, 2).unwrap(), ValueRef::Missing);
            }
            assert_eq!(
                table.cell(4, 0),
                Err(CellError::OutOfBounds { row: 4, column: 0 })
            );
            assert_eq!(
                table.cell(0, 3),
                Err(CellError::OutOfBounds { row: 0, column: 3 })
            );
        }

        #[test]
        /// A projection and an appended column keep record and column order.
        fn projected_and_appended_columns_keep_record_and_column_order() {
            let schema = logical_schema(&[("B", CT::Str), ("A", CT::Int)]);
            let table = ColumnTable::new(
                schema,
                vec![
                    Column::Str(["first", "second", "third"].map(Some).into_iter().collect()),
                    Column::Int([10, 20, 30].map(Some).into_iter().collect()),
                ],
                3,
            )
            .unwrap();
            let projected = table
                .project(logical_schema(&[("A", CT::Int), ("B", CT::Str)]), &[1, 0])
                .unwrap();
            let appended = projected
                .appended(
                    logical_schema(&[("A", CT::Int), ("B", CT::Str), ("C", CT::Int)]),
                    Column::Int([1, 2, 3].map(Some).into_iter().collect()),
                )
                .unwrap();
            assert_eq!(
                appended
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>(),
                ["A", "B", "C"]
            );
            for (row, (value, name, position)) in
                [(10, "first", 1), (20, "second", 2), (30, "third", 3)]
                    .into_iter()
                    .enumerate()
            {
                assert_eq!(appended.cell(row, 0).unwrap(), ValueRef::Int(value));
                assert_eq!(appended.cell(row, 1).unwrap(), ValueRef::Str(name));
                assert_eq!(appended.cell(row, 2).unwrap(), ValueRef::Int(position));
            }
        }

        #[test]
        /// Keep imputed fields and every rung of the one collected-precision
        /// ladder beside the completed fields, for dates and datetimes alike.
        fn temporal_fields_and_every_collected_precision_survive() {
            let dates = [P::None, P::Year, P::Month, P::Day]
                .map(|precision| Date::new(2025, 3, 4, precision).unwrap());
            let day = Date::new(2025, 3, 4, P::Day).unwrap();
            let negative = Date::new(1969, 12, 31, P::Month).unwrap();
            let datetimes = [
                DateTime::new(day, 0, 0, 0, P::None).unwrap(),
                DateTime::new(day, 0, 0, 0, P::Year).unwrap(),
                DateTime::new(day, 0, 0, 0, P::Month).unwrap(),
                // Day precision still retains supplied or imputed time fields.
                DateTime::new(day, 12, 34, 56, P::Day).unwrap(),
                DateTime::new(day, 8, 59, 59, P::Hour).unwrap(),
                DateTime::new(day, 8, 30, 0, P::Minute).unwrap(),
                DateTime::new(negative, 23, 59, 59, P::Second).unwrap(),
            ];
            let schema = logical_schema(&[("date", CT::Date), ("datetime", CT::DateTime)]);
            let table = ColumnTable::new(
                schema,
                vec![
                    Column::Date(dates.map(Some).into_iter().chain([None; 4]).collect()),
                    Column::DateTime(datetimes.map(Some).into_iter().chain([None]).collect()),
                ],
                8,
            )
            .unwrap();
            for (row, expected) in dates.into_iter().enumerate() {
                assert_eq!(date(&table, row, 0).fields(), expected.fields());
                assert_eq!(
                    date(&table, row, 0).collected_precision(),
                    expected.collected_precision()
                );
            }
            for (row, expected) in datetimes.into_iter().enumerate() {
                assert_eq!(datetime(&table, row, 1).fields(), expected.fields());
                assert_eq!(
                    datetime(&table, row, 1).collected_precision(),
                    expected.collected_precision()
                );
            }
            assert_eq!(table.cell(4, 0).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(7, 1).unwrap(), ValueRef::Missing);
        }

        #[test]
        /// Retain declared schema with no records and record counts with no columns.
        fn zero_rows_and_zero_columns_retain_independent_shapes() {
            let schema = logical_schema(&[("D", CT::Date), ("T", CT::Str)]);
            let empty = ColumnTable::new(
                schema.clone(),
                vec![Column::empty(CT::Date), Column::empty(CT::Str)],
                0,
            )
            .unwrap();
            assert_eq!(empty.schema(), &schema);
            assert_eq!(empty.row_count(), 0);
            assert_eq!(
                empty.cell(0, 0),
                Err(CellError::OutOfBounds { row: 0, column: 0 })
            );
            let records = ColumnTable::new(TableSchema::new(vec![]).unwrap(), vec![], 3).unwrap();
            assert_eq!(records.row_count(), 3);
            assert_eq!(
                records.cell(0, 0),
                Err(CellError::OutOfBounds { row: 0, column: 0 })
            );
        }

        #[test]
        /// Fixed geometry precedes every column check; a missing column, or
        /// a column of another type or length, is refused in column order.
        fn fixed_geometry_precedes_column_checks_in_column_order() {
            let wide = TableSchema::new(
                (0..1_025)
                    .map(|column| yamaa_core::table::Column {
                        name: format!("C{column}"),
                        kind: CT::Int,
                    })
                    .collect(),
            )
            .unwrap();
            let columns = (0..1_025).map(|_| Column::empty(CT::Str)).collect();
            assert!(matches!(
                ColumnTable::new(wide, columns, 0),
                Err(TableError::Limit {
                    resource: "columns",
                    limit: 1_024,
                    required: Some(1_025),
                })
            ));
            assert!(matches!(
                ColumnTable::new(
                    TableSchema::new(vec![]).unwrap(),
                    vec![],
                    Limits::TABLE_ROWS + 1
                ),
                Err(TableError::Limit {
                    resource: "table_rows",
                    limit: Limits::TABLE_ROWS,
                    required: Some(_),
                })
            ));
            let schema = logical_schema(&[("I", CT::Int), ("S", CT::Str)]);
            let int = || Column::Int([Some(1)].into_iter().collect());
            for (columns, expected) in [
                (vec![int(), Column::empty(CT::Str)], 1),
                (vec![Column::empty(CT::Str), int()], 0),
                (vec![Column::empty(CT::Int), Column::empty(CT::Str)], 0),
            ] {
                assert!(matches!(
                    ColumnTable::new(schema.clone(), columns, 1),
                    Err(TableError::ArrayType { batch: 0, column }) if column == expected
                ));
            }
            assert!(matches!(
                ColumnTable::new(schema, vec![int()], 1),
                Err(TableError::SchemaMismatch { batch: 0 })
            ));
        }

        #[test]
        /// Keep explicit missing values separate from i64 sentinels and finite float bits.
        fn null_validity_never_collides_with_exact_scalar_payloads() {
            let schema = logical_schema(&[("I", CT::Int), ("F", CT::Float)]);
            let bits = [1, 0x7fefffffffffffff, 0xffefffffffffffff];
            let floats = bits.map(|bits| Some(f64::from_bits(bits)));
            let table = ColumnTable::new(
                schema,
                vec![
                    Column::Int(
                        [Some(i64::MIN), None, Some(i64::MAX), Some(0)]
                            .into_iter()
                            .collect(),
                    ),
                    Column::floats(floats.into_iter().chain([None])),
                ],
                4,
            )
            .unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(i64::MIN));
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(3, 0).unwrap(), ValueRef::Int(0));
            for (row, bits) in bits.into_iter().enumerate() {
                let ValueRef::Float(value) = table.cell(row, 1).unwrap() else {
                    panic!("finite float required")
                };
                assert_eq!(value.get().to_bits(), bits);
            }
            assert_eq!(table.cell(3, 1).unwrap(), ValueRef::Missing);
        }
    }
    mod epoch {
        // Seam, for every test here: the epoch kernel over generated calendar
        // and datetime ranges.
        use yamaa_adapters::epoch::{
            date_days, date_from_days, datetime_from_seconds, datetime_seconds,
        };
        use yamaa_core::temporal::{Date, Precision as P};

        #[test]
        /// Compare independent epoch anchors and every day of a Gregorian cycle.
        fn epoch_calendar_anchors_and_full_gregorian_cycle() {
            for (days, fields) in [
                (-719162, (1, 1, 1)),
                (-135140, (1600, 1, 1)),
                (-25508, (1900, 3, 1)),
                (-1, (1969, 12, 31)),
                (0, (1970, 1, 1)),
                (11016, (2000, 2, 29)),
                (20089, (2025, 1, 1)),
                (2932896, (9999, 12, 31)),
            ] {
                let date = date_from_days(days, P::Day).unwrap();
                assert_eq!(date.fields(), fields, "{days}");
                assert_eq!(i64::from(date_days(date)), days, "{days}");
            }
            for days in [-719163, 2932897, i64::from(i32::MIN), i64::MAX] {
                assert_eq!(date_from_days(days, P::Day), None, "{days}");
            }
            let mut civil = vec![];
            for year in 1600..2000 {
                for month in 1..=12 {
                    for day in 1..=31 {
                        if let Ok(value) = Date::new(year, month, day, P::Day) {
                            civil.push(value);
                        }
                    }
                }
            }
            assert_eq!(civil.len(), 146097);
            for (row, date) in civil.into_iter().enumerate() {
                assert_eq!(date_days(date), -135140 + row as i32);
                let decoded = date_from_days(-135140 + row as i64, P::Day).unwrap();
                assert_eq!(decoded.fields(), date.fields());
            }
        }

        #[test]
        /// Check negative epochs and range endpoints of civil seconds.
        fn datetime_range_and_negative_epochs_are_strict() {
            for seconds in [i64::MIN, i64::MAX, -62135596801, 253402300800] {
                assert_eq!(datetime_from_seconds(seconds, P::Second), None, "{seconds}");
            }
            for (seconds, fields) in [
                (-62135596800, (1, 1, 1, 0, 0, 0)),
                (-86401, (1969, 12, 30, 23, 59, 59)),
                (-86400, (1969, 12, 31, 0, 0, 0)),
                (-1, (1969, 12, 31, 23, 59, 59)),
                (0, (1970, 1, 1, 0, 0, 0)),
                (253402300799, (9999, 12, 31, 23, 59, 59)),
            ] {
                let datetime = datetime_from_seconds(seconds, P::Second).unwrap();
                assert_eq!(datetime.fields(), fields, "{seconds}");
                assert_eq!(datetime_seconds(datetime), seconds, "{seconds}");
            }
        }
    }
    mod csv_artifact {
        // Seam, for every test here: the CSV encoder over tables built in
        // memory, which a case reaches only through a run.
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
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
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
                csv_artifact::render(&table, &[0, 1, 2, 3], None).unwrap(),
                expected.as_bytes()
            );
        }
        #[test]
        fn header_only_and_projection_order_are_exact() {
            let table = text_table(&["FIRST", "LAST"], vec![]);
            assert_eq!(
                csv_artifact::render(&table, &[1, 0], None).unwrap(),
                b"LAST,FIRST\n"
            );
            for columns in [vec![], vec![0, 0], vec![2]] {
                assert!(matches!(
                    csv_artifact::render(&table, &columns, None),
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
                csv_artifact::render(&table, &[0, 1], None).unwrap(),
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
            let expected = b"S,F,I\n\"\",0.13,1\n\"a,b\",2.68,2\n,,3\nz,0.00,4\n";
            assert_eq!(
                csv_artifact::render(&table, &[2, 1, 0], Some("2")).unwrap(),
                expected
            );
            assert_eq!(table.rows[0][1], Value::float(0.125));
            assert_eq!(
                csv_artifact::render(&table, &[0], Some("999999999999999999999999999999")).unwrap(),
                b"I\n1\n2\n3\n4\n"
            );
        }

        /// The encoder's partitions (D264), a seam no benchmark reaches: a
        /// file written a partition at a time has the whole render's bytes,
        /// the display precision read with the first record even when an
        /// empty partition comes first. Each partition ends where its records
        /// do in the bytes above: the header is 6 bytes and the records 10,
        /// 13, 4 and 9.
        #[test]
        fn a_file_written_a_partition_at_a_time_has_the_whole_renders_bytes() {
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
            let expected = b"S,F,I\n\"\",0.13,1\n\"a,b\",2.68,2\n,,3\nz,0.00,4\n".to_vec();
            let cases = [
                ("one partition", std::iter::once(0..4).collect(), vec![42]),
                (
                    "one record each",
                    vec![0..1, 1..2, 2..3, 3..4],
                    vec![16, 29, 33, 42],
                ),
                (
                    "an empty first partition",
                    vec![0..0, 0..3, 3..4],
                    vec![6, 33, 42],
                ),
            ];
            for (label, partitions, ends) in cases {
                assert_eq!(
                    csv_artifact::render_partitions(&table, &[2, 1, 0], Some("2"), &partitions)
                        .unwrap(),
                    (expected.clone(), ends),
                    "{label}"
                );
            }
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
                    panic!("impossible output width must precede cell authority")
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
            for precision in ["10737418241", "999999999999999999999999999999"] {
                assert!(
                    matches!(csv_artifact::render(&table, &[0, 1], Some(precision)),
            Err(Error::Limit { resource: "output_bytes", limit, actual: None, decimal_formatting: true }) if limit == yamaa_core::limits::Limits::OUTPUT_BYTES)
                );
            }
        }

        #[test]
        fn retired_python_scalar_text_and_calendar_edges_keep_exact_csv_bytes() {
            let table = Table {
                schema: TableSchema::new(
                    [
                        ("ID", ColumnType::Str),
                        ("COUNT", ColumnType::Int),
                        ("WHEN", ColumnType::Date),
                        ("AT", ColumnType::DateTime),
                        ("TEXT", ColumnType::Str),
                    ]
                    .into_iter()
                    .map(|(name, kind)| Column {
                        name: name.into(),
                        kind,
                    })
                    .collect(),
                )
                .unwrap(),
                rows: vec![
                    vec![
                        Value::Str("1".into()),
                        Value::Int(-42),
                        Value::Date("2020-01-02".parse().unwrap()),
                        Value::DateTime("2020-01-02T03:04:05".parse().unwrap()),
                        Value::Str("\u{e9}\u{1d400}".into()),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Int(0),
                        Value::Missing,
                        Value::Missing,
                        Value::Missing,
                    ],
                ],
            };
            assert_eq!(
        csv_artifact::render(&table, &[0, 1, 2, 3, 4], None).unwrap(),
        "ID,COUNT,WHEN,AT,TEXT\n1,-42,2020-01-02,2020-01-02T03:04:05,\u{e9}\u{1d400}\n2,0,,,\n"
            .as_bytes()
    );
            let edges = Table {
                schema: TableSchema::new(vec![
                    Column {
                        name: "ID".into(),
                        kind: ColumnType::Str,
                    },
                    table.schema.columns()[2].clone(),
                    table.schema.columns()[3].clone(),
                ])
                .unwrap(),
                rows: vec![
                    vec![
                        Value::Str("1".into()),
                        Value::Date("0001-01-01".parse().unwrap()),
                        Value::DateTime("0001-01-01T00:00:00".parse().unwrap()),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Date("9999-12-31".parse().unwrap()),
                        Value::DateTime("9999-12-31T23:59:59".parse().unwrap()),
                    ],
                ],
            };
            assert_eq!(
                csv_artifact::render(&edges, &[0, 1, 2], None).unwrap(),
                b"ID,WHEN,AT\n1,0001-01-01,0001-01-01T00:00:00\n2,9999-12-31,9999-12-31T23:59:59\n"
            );
        }
    }
    #[cfg(feature = "yaml")]
    mod csv_precision {
        // What a save of a specification with output decimals writes, and how
        // its refusals publish nothing, are the cases of
        // `fixtures/diagnostics/csv_precision.json`. The tests left here
        // compare what no case states.
        use crate::support::ports::{NoFunctions, Sources};
        use crate::support::Edit as _;
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::PreparedRun,
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;

        const SPEC: &str = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: output.csv, columns: [ID, VALUE, DOUBLE], decimals: PRECISION}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n  - {name: DOUBLE, type: float, derivation: 'VALUE + VALUE'}\n";
        fn prepared(precision: &str) -> PreparedRun {
            let document = yamaa_adapters::shipped_schema::prepare(
                Source {
                    identity: "spec.yaml".into(),
                    bytes: SPEC.edit("PRECISION", precision).into_bytes(),
                },
                "spec.yaml".into(),
                &mut crate::support::inheritance::Files,
            )
            .unwrap();
            PreparedRun::prepare(document).unwrap()
        }
        /// The one CSV input of the csv-precision cases, served at `input.csv`.
        fn study() -> Sources {
            Sources::by_path([(
                "input.csv",
                b"ID,VALUE\n1,0.125\n2,1.234\n3,2.345\n".as_slice(),
            )])
        }
        fn identity() -> Identity<'static> {
            Identity {
                runtime: "python",
                runtime_version: "fixture",
                engine_version: "fixture",
                example: "csv-precision",
                specification: "spec.yaml",
                base_directory: ".",
            }
        }

        /// Seam: the scripted `Sources` port's read count and the result's
        /// observation records: no artifact before the save, and the saved
        /// report's artifact holding the rounded bytes once the run is gone.
        /// The unrounded output and the bytes a save writes are the case
        /// `csv_precision.json::two_decimals_round_the_saved_bytes_and_leave_the_output_unrounded`.
        #[test]
        fn a_result_reads_its_input_once_and_records_its_rounded_bytes_only_when_saved() {
            let run = prepared("2");
            let mut study = study();
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut study, Scope::default());
            let result =
                specification_report::build_result(&run, &attempt, identity(), &[]).unwrap();
            assert_eq!(
                result.observations()["artifacts"],
                yamaa_adapters::json!([])
            );
            drop(attempt);
            drop(run);
            // The saved bytes belong to the result, not to the run.
            let expected = b"ID,VALUE,DOUBLE\n1,0.13,0.25\n2,1.23,2.47\n3,2.35,4.69\n";
            assert_eq!(result.files(), [("output.csv", expected.as_slice())]);
            let report = result.saved_observations().unwrap();
            assert_eq!(
                report["artifacts"][0]["content"]
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                expected
            );
            assert_eq!(study.reads(), 1);
        }

        /// Seam: the compiled specification's retained decimals, which no
        /// case reads.
        #[test]
        fn compiler_keeps_arbitrary_width_precision_without_a_metadata_copy_allowance() {
            let precision = "9".repeat(1000);
            let document = yamaa_adapters::shipped_schema::prepare(
                Source {
                    identity: "spec.yaml".into(),
                    bytes: SPEC.edit("PRECISION", &precision).into_bytes(),
                },
                "spec.yaml".into(),
                &mut crate::support::inheritance::Files,
            )
            .unwrap();
            let run = PreparedRun::prepare(document).unwrap();
            assert_eq!(run.compiled().output_decimals(), Some(precision.as_str()));
        }
    }
    mod csv_source {
        // Seam: the untyped text table's owned cells and its limits.
        #[test]
        fn untyped_text_table_owns_exact_source_cells_and_limits() {
            use yamaa_adapters::csv_source::{parse_text_table, TextTableError};
            use yamaa_core::table::{TableAccess, ValueRef};
            let mut input = b"A,B\n9007199254740993,\"\"\nx\0y,NA".to_vec();
            let table = parse_text_table(&input).unwrap();
            input.fill(0);
            drop(input);
            assert_eq!(table.row_count(), 2);
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("9007199254740993"));
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("x\0y"));
            assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("NA"));
            assert!(table.cell(2, 0).is_err());
            assert_eq!(parse_text_table(b"A,B").unwrap().row_count(), 0);
            let wide = (0..1_025)
                .map(|i| format!("C{i}"))
                .collect::<Vec<_>>()
                .join(",");
            assert!(matches!(
                parse_text_table(wide.as_bytes()),
                Err(TextTableError::Csv(
                    yamaa_adapters::csv_source::Error::Limit {
                        resource: "columns",
                        limit: 1_024,
                        actual: Some(1_025)
                    }
                ))
            ));
        }
    }
    #[cfg(feature = "parquet")]
    mod parquet_artifact {
        // Seam, for every test here: the Parquet encoder over tables built in
        // memory, its file read back with the `parquet` crate.
        use parquet::{
            basic::{Compression, LogicalType, Repetition, Type},
            file::{reader::FileReader, serialized_reader::SerializedFileReader},
            record::{Field, RowAccessor},
        };
        use std::{
            convert::Infallible,
            fs::{File, OpenOptions},
            io::Write,
        };
        use yamaa_adapters::parquet_artifact::{render, Error};
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
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.rows
                    .get(row)
                    .and_then(|r| r.get(column))
                    .map(ValueRef::from)
                    .ok_or(CellError::OutOfBounds { row, column })
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
        /// A Parquet file in its own scratch directory.
        struct Source(crate::support::Scratch);
        impl Source {
            fn new(bytes: &[u8]) -> Self {
                let directory = crate::support::Scratch::new("parquet-test");
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("source.parquet"))
                    .unwrap()
                    .write_all(bytes)
                    .unwrap();
                Self(directory)
            }
            fn reader(&self) -> SerializedFileReader<File> {
                SerializedFileReader::new(File::open(self.0.join("source.parquet")).unwrap())
                    .unwrap()
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
            let bytes = render(&table, &[0, 1, 2, 3, 4]).unwrap();
            let source = Source::new(&bytes);
            let reader = source.reader();
            let meta = reader.metadata().file_metadata();
            assert!(meta.key_value_metadata().is_none());
            let schema = meta.schema_descr();
            // Textual schema truth avoids a release-specific LogicalType constructor.
            let timestamp = parquet::schema::parser::parse_message_type(
                "message expected { OPTIONAL INT64 at (TIMESTAMP(MICROS,false)); }",
            )
            .unwrap();
            let expected = [
                (Type::BYTE_ARRAY, Some(LogicalType::String)),
                (Type::INT64, None),
                (Type::DOUBLE, None),
                (Type::INT32, Some(LogicalType::Date)),
                (
                    Type::INT64,
                    timestamp.get_fields()[0].get_basic_info().logical_type(),
                ),
            ];
            for (i, (physical, logical)) in expected.into_iter().enumerate() {
                let field = schema.column(i);
                assert_eq!(field.physical_type(), physical);
                assert_eq!(field.logical_type(), logical);
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
            assert_eq!(rows[1].get_column_iter().next().unwrap().1, &Field::Null);
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
            assert_eq!(rows[2].get_column_iter().nth(3).unwrap().1, &Field::Null);
            assert_eq!(rows[2].get_column_iter().nth(4).unwrap().1, &Field::Null);
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
            let Err(Error::Cell(CellError::Access(original))) = render(&failed, &[0]) else {
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
            let rows = yamaa_adapters::parquet_artifact::records(&table, &[0, 1, 2], None).unwrap();
            assert_eq!(
                rows,
                [
                    r#"["S", "F", "I"]"#,
                    r#"["\ud83d\ude00\u007f\n\"", -0.0, -9223372036854775808]"#,
                    r#"[null, 1e-07, 9007199254740993]"#,
                ]
            );
        }

        #[test]
        fn ordered_row_groups_and_projection_survive_an_empty_source() {
            let table = table(
                &[("A", ColumnType::Int), ("B", ColumnType::Int)],
                (0..2051)
                    .map(|i| vec![Value::Int(i), Value::Int(-i)])
                    .collect(),
            );
            let source = Source::new(&render(&table, &[1, 0]).unwrap());
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
            let source = Source::new(&render(&empty, &[1]).unwrap());
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
        fn invalid_projection_and_cell_types_produce_no_artifact() {
            let table = table(
                &[("S", ColumnType::Str)],
                vec![vec![Value::Str("x".repeat(4096))]],
            );
            for projection in [&[][..], &[0, 0], &[1]] {
                assert!(matches!(render(&table, projection), Err(Error::Projection)));
            }
            let wrong = Table {
                schema: table.schema,
                rows: vec![vec![Value::Int(1)]],
            };
            assert!(matches!(render(&wrong, &[0]), Err(Error::ValueType)));
        }

        #[test]
        fn fixed_geometry_is_admitted_before_output_cell_access() {
            use yamaa_adapters::csv_artifact::{self, Error as CsvError};
            struct Geometry {
                schema: TableSchema,
                rows: usize,
            }
            impl TableAccess for Geometry {
                type Error = &'static str;
                fn schema(&self) -> &TableSchema {
                    &self.schema
                }
                fn row_count(&self) -> usize {
                    self.rows
                }
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                    Err(CellError::Access("admitted table"))
                }
            }
            for (rows, columns, resource, actual, exceeds) in [
                (9_999_999, 1, "table_rows", 9_999_999, false),
                (10_000_001, 1, "table_rows", 10_000_001, true),
                (1, 1023, "columns", 1023, false),
                (1, 1025, "columns", 1025, true),
                (3_575_418, 179, "table_cells", 639_999_822, false),
                (3_575_419, 179, "table_cells", 640_000_001, true),
            ] {
                let schema = TableSchema::new(
                    (0..columns)
                        .map(|column| Column {
                            name: format!("F{column}"),
                            kind: ColumnType::Int,
                        })
                        .collect(),
                )
                .unwrap();
                let table = Geometry { schema, rows };
                let projection = (0..columns).collect::<Vec<_>>();
                macro_rules! check {
                    ($result:expr, $error:ident) => {
                        match ($result, exceeds) {
                            (
                                Err($error::Limit {
                                    resource: observed,
                                    actual: Some(required),
                                    ..
                                }),
                                true,
                            ) => assert_eq!((observed, required), (resource, actual)),
                            (Err($error::Cell(CellError::Access("admitted table"))), false) => {}
                            (other, _) => panic!("unexpected {resource} admission: {other:?}"),
                        }
                    };
                }
                check!(csv_artifact::render(&table, &projection, None), CsvError);
                check!(render(&table, &projection), Error);
                check!(
                    yamaa_adapters::parquet_artifact::records(&table, &projection, None),
                    Error
                );
            }
        }

        #[test]
        fn artifacts_keep_complete_temporal_fields_and_drop_collected_precision() {
            use yamaa_core::temporal::{Date, DateTime, Precision};
            // VALUE-17: precision is internal provenance. Even imputed values carry
            // complete calendar fields; artifacts deliberately retain only those fields.
            let table = table(
                &[("D", ColumnType::Date), ("T", ColumnType::DateTime)],
                [
                    (Precision::Year, Precision::Day),
                    (Precision::Month, Precision::Day),
                    (Precision::Day, Precision::Second),
                ]
                .into_iter()
                .map(|(date_precision, time_precision)| {
                    vec![
                        Value::Date(Date::new(1970, 2, 28, date_precision).unwrap()),
                        Value::DateTime(
                            DateTime::new(
                                Date::new(1970, 1, 1, Precision::Day).unwrap(),
                                23,
                                59,
                                59,
                                time_precision,
                            )
                            .unwrap(),
                        ),
                    ]
                })
                .collect(),
            );
            let bytes = render(&table, &[0, 1]).unwrap();
            let source = Source::new(&bytes);
            let reader = source.reader();
            assert!(reader
                .metadata()
                .file_metadata()
                .key_value_metadata()
                .is_none());
            let rows = reader
                .get_row_iter(None)
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 3);
            for row in &rows {
                assert_eq!(row.get_column_iter().next().unwrap().1, &Field::Date(58));
                assert_eq!(row.get_timestamp_micros(1).unwrap(), 86_399_000_000);
            }
            assert_eq!(
                yamaa_adapters::parquet_artifact::records(&table, &[0, 1], None).unwrap(),
                [
                    r#"["D", "T"]"#,
                    r#"["1970-02-28", "1970-01-01T23:59:59"]"#,
                    r#"["1970-02-28", "1970-01-01T23:59:59"]"#,
                    r#"["1970-02-28", "1970-01-01T23:59:59"]"#,
                ],
            );
        }

        #[test]
        fn retired_python_double_operands_round_trip_without_display_rounding() {
            let values = [0.1, 1.0 / 3.0, f64::from_bits(1), 1e308, -0.0, 2.675];
            let table = table(
                &[("USUBJID", ColumnType::Str), ("VALUE", ColumnType::Float)],
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        vec![Value::Str(format!("S-{index}")), Value::float(value)]
                    })
                    .collect(),
            );
            let source = Source::new(&render(&table, &[0, 1]).unwrap());
            let reader = source.reader();
            let rows = reader
                .get_row_iter(None)
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), values.len());
            for (row, expected) in rows.iter().zip(values) {
                assert_eq!(row.get_double(1).unwrap().to_bits(), expected.to_bits());
            }
        }
    }
    #[cfg(feature = "yaml")]
    mod predicate_diagnostics {
        //! Complete original-document outcomes captured independently from Python.
        //!
        //! Already data: the authored reports of
        //! `fixtures/predicate_context_diagnostics.tsv`, with the scripted
        //! port's captured inputs.
        use crate::support::ports::NoFunctions;
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::PreparedRun,
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;

        #[test]
        fn original_regex_and_temporal_reports_match_independently_captured_reference() {
            use crate::support::ports::Sources;
            let mut count = 0;
            for row in include_str!("fixtures/predicate_context_diagnostics.tsv")
                .lines()
                .skip(1)
            {
                let fields = row.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 3);
                let prepared = yamaa_adapters::shipped_schema::prepare(
                    Source {
                        identity: "spec.yaml".into(),
                        bytes: fields[1].as_bytes().to_vec(),
                    },
                    "spec.yaml".into(),
                    &mut crate::support::inheritance::Files,
                )
                .unwrap();
                let run = PreparedRun::prepare(prepared).unwrap();
                let mut port = Sources::by_path([("source.csv", b"ID,V\n1,2\n".as_slice())]);
                let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
                let example = format!("predicate-context-{}", fields[0]);
                let result = specification_report::build_result(
                    &run,
                    &attempt,
                    Identity {
                        runtime: "python",
                        runtime_version: "fixture-runtime",
                        engine_version: "fixture-engine",
                        example: &example,
                        specification: "spec.yaml",
                        base_directory: ".",
                    },
                    &[],
                )
                .unwrap();
                let expected: yamaa_core::json::Value =
                    yamaa_adapters::json_wire::decode(fields[2]).unwrap();
                assert_eq!(result.observations(), expected, "{}", fields[0]);
                assert!(result.output().is_none());
                drop(attempt);
                drop(run);
                // A failed predicate attempt has no file to save.
                assert!(result.files().is_empty(), "{}", fields[0]);
                assert_eq!(result.observations(), expected);
                assert_eq!(port.captured(), ["SRC"]);
                count += 1;
            }
            assert_eq!(count, 2);
        }
    }
    #[cfg(feature = "arrow")]
    mod public_table {
        // Seam, for every test here: the public table's Arrow arrays.
        use arrow_array::{
            Array, Date32Array, Float64Array, Int64Array, StringArray, TimestampSecondArray,
        };
        use yamaa_adapters::{
            column_table::{Column, ColumnTable},
            public_table::PublicTable,
        };
        use yamaa_core::{
            table::{Column as Declared, TableSchema},
            temporal::{Date, DateTime, Precision},
            value::ColumnType,
        };

        fn source(rows: usize) -> ColumnTable {
            let schema = TableSchema::new(
                [
                    ("S", ColumnType::Str),
                    ("I", ColumnType::Int),
                    ("F", ColumnType::Float),
                    ("D", ColumnType::Date),
                    ("T", ColumnType::DateTime),
                ]
                .into_iter()
                .map(|(name, kind)| Declared {
                    name: name.into(),
                    kind,
                })
                .collect(),
            )
            .unwrap();
            let date = Date::new(1970, 1, 1, Precision::Year).unwrap();
            let time = DateTime::new(date, 0, 0, 0, Precision::Day).unwrap();
            let strings = [Some("nul\0text"), None, Some("")];
            let integers = [Some(i64::MIN), None, Some(i64::MAX)];
            let floats = [Some(-0.0), None, Some(1.25)];
            let dates = [
                Some(date),
                None,
                Some(Date::new(1969, 12, 31, Precision::Day).unwrap()),
            ];
            let times = [
                Some(time),
                None,
                Some(DateTime::new(date, 0, 0, 1, Precision::Second).unwrap()),
            ];
            let columns = vec![
                Column::Str(strings[..rows].iter().copied().collect()),
                Column::Int(integers[..rows].iter().copied().collect()),
                Column::floats(floats[..rows].iter().copied()),
                Column::Date(dates[..rows].to_vec()),
                Column::DateTime(times[..rows].to_vec()),
            ];
            ColumnTable::new(schema, columns, rows).unwrap()
        }

        /// The public Arrow form keeps every value exactly and each missing
        /// value as a null, a date as its epoch day and a datetime as its
        /// epoch second, without collected precision.
        #[test]
        fn public_columns_preserve_exact_values_missing_and_temporal_null_masks() {
            let table = PublicTable::from_table(&source(3), &[0, 1, 2, 3, 4]).unwrap();
            assert_eq!(table.rows(), 3);
            let [batch] = table.batches() else {
                panic!("one record batch")
            };
            let strings = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            assert_eq!(strings.value(0), "nul\0text");
            assert!(strings.is_null(1));
            assert_eq!(strings.value(2), "");
            let integers = batch
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap();
            assert_eq!(
                integers.iter().collect::<Vec<_>>(),
                [Some(i64::MIN), None, Some(i64::MAX)]
            );
            let floats = batch
                .column(2)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap();
            assert_eq!(floats.value(0).to_bits(), (-0.0f64).to_bits());
            assert!(floats.is_null(1));
            let dates = batch
                .column(3)
                .as_any()
                .downcast_ref::<Date32Array>()
                .unwrap();
            assert_eq!(dates.iter().collect::<Vec<_>>(), [Some(0), None, Some(-1)]);
            let times = batch
                .column(4)
                .as_any()
                .downcast_ref::<TimestampSecondArray>()
                .unwrap();
            assert_eq!(times.iter().collect::<Vec<_>>(), [Some(0), None, Some(1)]);
            // A null holds no payload of its own.
            assert_eq!(integers.values().as_ref(), [i64::MIN, 0, i64::MAX]);
            assert_eq!(strings.value_offsets(), [0, 8, 8, 8]);
        }

        #[test]
        fn empty_public_table_retains_every_declared_column_type() {
            let table = PublicTable::from_table(&source(0), &[0, 1, 2, 3, 4]).unwrap();
            assert_eq!(table.rows(), 0);
            let [batch] = table.batches() else {
                panic!("one empty record batch")
            };
            assert_eq!(batch.num_rows(), 0);
            assert_eq!(batch.num_columns(), 5);
            assert!(batch.column(0).as_any().is::<StringArray>());
            assert!(batch.column(1).as_any().is::<Int64Array>());
            assert!(batch.column(2).as_any().is::<Float64Array>());
            assert!(batch.column(3).as_any().is::<Date32Array>());
            assert!(batch.column(4).as_any().is::<TimestampSecondArray>());
        }
    }
}
